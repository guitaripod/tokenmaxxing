# Data sources

Both apps read the same local credentials Claude Code and opencode already write, and render the same [quota model](model.md).

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

## opencode go — estimated locally

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

**Wiring a live reading later:** capture the console's own request (DevTools → Network → the `usage/me` or `balance/summary` call → *Copy as cURL*) to get the exact base URL, Bearer token, and response shape, then add a provider that prefers the live endpoint and falls back to the local estimate when the token is absent or expired. The account token comes from an OAuth device flow (`/accounts/deviceauth/{usercode,token}`), which is the durable, renewable path.
