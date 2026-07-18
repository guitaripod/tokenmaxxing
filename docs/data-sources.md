# Data sources

Both apps read the same local credentials Claude Code, Grok, and opencode already write, and render the same [quota model](model.md).

## Claude (Anthropic Max / Pro) — live

**Credentials:** `~/.claude/.credentials.json` → `claudeAiOauth.{accessToken, refreshToken, expiresAt}`.

**Request** (the same one Claude Code's `/usage` command makes):

```
GET https://api.anthropic.com/api/oauth/usage
authorization: Bearer <accessToken>
anthropic-beta: oauth-2025-04-20
accept: application/json
```

**Response** (fields used):

- `five_hour.utilization` / `seven_day.utilization` — 0–100 percentages, each with `resets_at` (ISO-8601 UTC).
- `limits[]` — the authoritative list; each entry is `{kind, group, percent, severity, resets_at, scope}`. `kind` is `session`, `weekly_all`, or `weekly_scoped` (per-model, e.g. `scope.model.display_name` = a model name). This is what drives one ring per window.
- `extra_usage` — `{is_enabled, monthly_limit, used_credits, utilization}` for pay-as-you-go overflow credits.

**Token refresh:** if `expiresAt` (ms) is within 2 minutes, or a call returns 401/403, the app POSTs to `https://platform.claude.com/v1/oauth/token` with `{grant_type: refresh_token, refresh_token, client_id: 9d1c250a-e61b-44d9-88ed-5944d1962f5e}`, then writes the rotated tokens **atomically** back into the credentials file, preserving every sibling key.

> This endpoint is unofficial and undocumented. The apps send an honest `User-Agent` and poll gently (Claude every ~2 min, forced on manual refresh).

## Grok (Grok Build / SuperGrok) — live

**Credentials:** `~/.grok/auth.json` → OIDC session keyed by `https://auth.x.ai::<client_id>`, fields `key` (access token), `refresh_token`, `expires_at`, `oidc_issuer`, `oidc_client_id`.

**Request** (the same one the Grok CLI's `/usage` command makes):

```
GET https://cli-chat-proxy.grok.com/v1/billing?format=credits
authorization: Bearer <accessToken>
accept: application/json
x-grok-client-version: <cli version>
x-grok-client-mode: cli
```

Plus a second call without `?format=credits` for monthly dollar spend:

```
GET https://cli-chat-proxy.grok.com/v1/billing
```

**Response fields used (credits format):**

- `config.creditUsagePercent` — overall weekly included-credit utilization (0–100).
- `config.currentPeriod.{start,end,type}` — weekly window; `end` is the trusted reset.
- `config.productUsage[]` — per-product rings (`GrokBuild`, `Api`, …) with optional `usagePercent`.
- `config.onDemandCap` / `onDemandUsed` / `prepaidBalance` — `{val}` money objects in **cents**.
- `config.billingPeriodStart` / `billingPeriodEnd`.

**Dollar format:** `config.monthlyLimit` / `used` / history rows — also cents.

**Token refresh:** if `expires_at` is within 2 minutes, or a call returns 401/403, the app POSTs to `{oidc_issuer}/oauth2/token` with `grant_type=refresh_token`, then writes the rotated tokens **atomically** back into `auth.json`, preserving every sibling key.

> Same family of unofficial-but-stable endpoints the CLI itself uses. Poll gently (~2 min), force on manual refresh.

### Grok local usage history

The CLI does **not** persist per-turn token usage on disk. Sessions under `~/.grok/sessions/<workspace>/<id>/` yield activity only:

- `events.jsonl` `turn_started` lines → daily turn series, model, heatmap.
- `summary.json` → fallback message counts, model id, timestamps.
- Project name derived from the percent-encoded workspace folder.

Dollar/token composition panels are omitted for Grok; KPIs are turns, sessions, models, and projects.

## opencode go — estimated from your machines

**Credentials:** `~/.local/share/opencode/auth.json` → `opencode-go` (type `api`, an `sk-` key). This key only authenticates the **Zen model gateway** (`https://opencode.ai/zen/go/v1/...`), not the account/billing API.

**Why there is no live reading:** OpenCode Go's quota is a set of server-side rolling **dollar spend caps** — ~$12 per 5-hour window, ~$30 per week, ~$60 per month. The numbers are served to the web console (`app.opencode.ai` / `console.opencode.ai`) from endpoints like `/org/{orgId}/usage/me` and `/balance/summary`, authenticated with a **Bearer account token minted by opencode's OAuth/GitHub login** — *not* the `sk-` inference key. There is no documented public API (tracked in opencode issue #10448), so a headless client can't read it without that account token.

**What the apps do instead:** open `~/.local/share/opencode/opencode.db` read-only (`query_only`, WAL-safe) and aggregate, per rolling window:

```sql
SELECT COALESCE(SUM(json_extract(data,'$.cost')),0.0), COUNT(*)
FROM message
WHERE json_extract(data,'$.providerID')='opencode-go'
  AND json_extract(data,'$.cost') IS NOT NULL
  AND time_created >= :cutoff_ms;
```

`fraction = min(1, window_spend / cap)`. Plus all-time totals (spend, sessions, tokens in/out, cache read) for the detail block. The card is labeled **EST** with a disclaimer.

**Other machines over SSH:** the caps are account-wide, so a single machine's `opencode.db` under-counts whenever more than one machine runs opencode. Both builds therefore read `opencode_remote_hosts` from `~/.config/tokenmaxxing/config.json` (an array of hostnames — Tailscale peers work well) and, for each host, run the same window/all-time aggregation remotely via non-interactive `ssh <host> sqlite3 -readonly -json ~/.local/share/opencode/opencode.db`, streaming the SQL over stdin. Only the ~300-byte aggregate row crosses the wire, never the database. Per-host results are cached for 60 s; if a host stops answering, its last reading is reused for up to 15 min and then dropped, with the card's note and a **Machines** detail row spelling out exactly which machines are included, cached, or unreachable. The merged spend is summed **before** the `min(1, spend/cap)` clamp. Requirements per remote host: key-based SSH (`BatchMode`), a `sqlite3` CLI, and opencode's default data path.

**Wiring a live reading later:** capture the console's own request (DevTools → Network → the `usage/me` or `balance/summary` call → *Copy as cURL*) to get the exact base URL, Bearer token, and response shape, then add a provider that prefers the live endpoint and falls back to the local estimate when the token is absent or expired. The account token comes from an OAuth device flow (`/accounts/deviceauth/{usercode,token}`), which is the durable, renewable path.

## Local usage history — the analytics layer

Beyond the live quota rings, both apps aggregate everything the two agents have written to disk into a full usage history (daily time-series, per-model / per-project / per-provider breakdowns, token composition, an activity heatmap, and lifetime totals). All of this is **local, exact for token counts, and estimated for dollars** (see the value note below).

### Claude — from Claude Code's transcripts (ccusage-style)

Claude Code records every turn to `~/.claude/projects/**/*.jsonl`. Each assistant line carries the authoritative token usage the model returned. The apps walk these files (incrementally — a per-file cache keyed by size + mtime, so only changed files are re-read), keep only `type == "assistant"` lines with a real `message.model` and non-zero usage, and **de-duplicate by `message.id` + `requestId`** (a turn present in multiple transcripts is counted once; a turn missing either id is always counted).

Fields used per turn: `timestamp`, `message.model`, `sessionId`, `cwd` (→ project name), and `message.usage.{input_tokens, output_tokens, cache_creation_input_tokens (split via cache_creation.ephemeral_5m/1h), cache_read_input_tokens, server_tool_use.{web_search_requests, web_fetch_requests}}`.

### opencode — all providers from `opencode.db`

The same read-only `opencode.db` that backs the estimated caps also holds every message opencode has run, across **all** providers (the paid Go gateway plus any free/local models — anthropic, ollama, xai, …). The apps aggregate it with `GROUP BY` queries into daily spend, per-provider and per-model breakdowns, a token composition (including `reasoning`), an activity heatmap (`strftime` local weekday × hour), and free-vs-paid token split. Only `opencode-go` rows carry a `cost`; free providers contribute tokens with `$0`.

### API-equivalent value (why the dollars are estimates)

A Max/Pro subscription bills a flat fee, so a Claude turn has no per-message price. To give the history a dollar axis, the apps price each turn at **what the same tokens would cost on the metered API** — the value the subscription returns — using an embedded per-model rate table (Opus 4.8 `$5/$25`, Fable 5 `$10/$50`, Sonnet 5 `$3/$15`, Haiku 4.5 `$1/$5` per MTok) with the API's cache multipliers (5-minute writes ×1.25, 1-hour writes ×2, reads ×0.1 of the input rate). These figures are always labelled **estimates**; the token counts they are derived from are exact. For opencode, `opencode-go`'s `cost` is the provider's own figure; free providers show `$0`.
