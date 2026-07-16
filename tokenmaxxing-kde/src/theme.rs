use crate::model::{Authority, Severity};
use std::sync::atomic::{AtomicBool, Ordering};

/// An RGB triple in the 0.0..=1.0 range Cairo expects.
pub type Rgb = (f64, f64, f64);

static DARK: AtomicBool = AtomicBool::new(true);

pub fn set_dark(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

// ---- brand accents ---------------------------------------------------------
// Claude  → Anthropic terracotta  #D97757
// Grok    → xAI monochrome silver (white on dark, ink on light)
// opencode → product green        #03B000

/// Claude / Anthropic brand orange.
pub const CYAN: Rgb = (0.851, 0.467, 0.341); // #D97757 — kept name for call-sites
/// opencode brand green.
pub const LIME: Rgb = (0.012, 0.690, 0.000); // #03B000
/// Grok / xAI silver (nominal rings on dark).
pub const VIOLET: Rgb = (0.910, 0.910, 0.920); // #E8E8EB
/// Severity warn.
pub const AMBER: Rgb = (0.93, 0.68, 0.22);
/// Severity critical / binding — distinct from Claude orange.
pub const MAGENTA: Rgb = (0.88, 0.28, 0.32);
pub const AZURE: Rgb = (0.42, 0.61, 0.86); // Anthropic accent blue #6A9BCC-ish
pub const TEAL: Rgb = (0.30, 0.70, 0.62);
pub const ORANGE: Rgb = (0.90, 0.50, 0.28);

/// Chart series — brand-led, then supporting accents.
pub const RAMP: [Rgb; 8] = [CYAN, VIOLET, LIME, AMBER, AZURE, TEAL, MAGENTA, ORANGE];

pub fn series_color(index: usize) -> Rgb {
    RAMP[index % RAMP.len()]
}

pub const TOKEN_INPUT: Rgb = CYAN;
pub const TOKEN_OUTPUT: Rgb = AZURE;
pub const TOKEN_CACHE_WRITE: Rgb = TEAL;
pub const TOKEN_CACHE_READ: Rgb = LIME;
pub const TOKEN_REASONING: Rgb = AMBER;

// ---- surfaces --------------------------------------------------------------

const DARK_BG: Rgb = (0.078, 0.078, 0.075); // #141413 Anthropic dark
const DARK_PANEL: Rgb = (0.118, 0.118, 0.114); // elevated ink
const DARK_TRACK: Rgb = (0.22, 0.22, 0.21);
const DARK_BORDER: Rgb = (0.20, 0.20, 0.19);
const DARK_TEXT: Rgb = (0.96, 0.95, 0.93); // warm white
const DARK_MUTED: Rgb = (0.62, 0.61, 0.58); // #B0AEA5-ish, higher contrast
const DARK_SECONDARY: Rgb = (0.82, 0.81, 0.78);

const LIGHT_BG: Rgb = (0.980, 0.976, 0.961); // #FAF9F5 Anthropic light
const LIGHT_PANEL: Rgb = (1.0, 1.0, 1.0);
const LIGHT_TRACK: Rgb = (0.88, 0.87, 0.84);
const LIGHT_BORDER: Rgb = (0.86, 0.85, 0.82);
const LIGHT_TEXT: Rgb = (0.078, 0.078, 0.075); // #141413
const LIGHT_MUTED: Rgb = (0.45, 0.44, 0.41);
const LIGHT_SECONDARY: Rgb = (0.30, 0.29, 0.27);

/// Grok ink accent on light surfaces (xAI monochrome).
const GROK_LIGHT: Rgb = (0.12, 0.12, 0.12);

pub fn bg() -> Rgb {
    if is_dark() {
        DARK_BG
    } else {
        LIGHT_BG
    }
}
pub fn panel() -> Rgb {
    if is_dark() {
        DARK_PANEL
    } else {
        LIGHT_PANEL
    }
}
pub fn track() -> Rgb {
    if is_dark() {
        DARK_TRACK
    } else {
        LIGHT_TRACK
    }
}
pub fn border() -> Rgb {
    if is_dark() {
        DARK_BORDER
    } else {
        LIGHT_BORDER
    }
}
pub fn text() -> Rgb {
    if is_dark() {
        DARK_TEXT
    } else {
        LIGHT_TEXT
    }
}
pub fn muted() -> Rgb {
    if is_dark() {
        DARK_MUTED
    } else {
        LIGHT_MUTED
    }
}
pub fn secondary() -> Rgb {
    if is_dark() {
        DARK_SECONDARY
    } else {
        LIGHT_SECONDARY
    }
}

/// Provider accent that adapts Grok's monochrome to the surface.
pub fn provider_accent(provider: ProviderAccent) -> Rgb {
    match provider {
        ProviderAccent::Claude => CYAN,
        ProviderAccent::Grok => {
            if is_dark() {
                VIOLET
            } else {
                GROK_LIGHT
            }
        }
        ProviderAccent::OpenCode => LIME,
    }
}

#[derive(Clone, Copy)]
pub enum ProviderAccent {
    Claude,
    Grok,
    OpenCode,
}

pub fn on_badge() -> Rgb {
    (0.06, 0.06, 0.06)
}

/// Solid, high-contrast status pills (white ink on saturated fill).
pub fn badge_fill(authority: Authority) -> Rgb {
    match authority {
        // Semantic green — reads as "up", distinct from OpenCode brand green.
        Authority::Live => (0.12, 0.55, 0.42),
        Authority::Estimated => (0.42, 0.40, 0.48),
        Authority::Unavailable => (0.72, 0.24, 0.28),
    }
}

pub fn badge_ink(_authority: Authority) -> Rgb {
    (0.98, 0.98, 0.97)
}

pub fn gauge_color(accent: Rgb, severity: Severity) -> Rgb {
    match severity {
        Severity::Nominal => accent,
        Severity::Warn => AMBER,
        Severity::Critical => MAGENTA,
    }
}

fn hex(c: Rgb) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c.0 * 255.0).round() as i32,
        (c.1 * 255.0).round() as i32,
        (c.2 * 255.0).round() as i32
    )
}

pub fn stylesheet(scale: f64) -> String {
    let title = (13.0 * scale).round() as i64;
    let bg = hex(bg());
    let text = hex(text());
    let muted = hex(muted());
    let panel = hex(panel());
    let border = hex(border());
    let accent = hex(CYAN);
    let action_bg = if is_dark() {
        "rgba(24, 24, 23, 0.96)"
    } else {
        "rgba(255, 255, 255, 0.97)"
    };
    let shadow = if is_dark() {
        "0 10px 28px rgba(0,0,0,0.50)"
    } else {
        "0 10px 28px rgba(20,18,14,0.10)"
    };
    format!(
        "window.tokenmaxxing {{ background-color: {bg}; color: {text}; }}\n\
         .tokenmaxxing headerbar {{ background: {bg}; color: {text}; border: none; box-shadow: none; }}\n\
         .tokenmaxxing headerbar .title {{ font-weight: 700; font-size: {title}px; color: {text}; letter-spacing: 0.01em; }}\n\
         .tokenmaxxing headerbar .subtitle {{ color: {muted}; }}\n\
         .tokenmaxxing scrolledwindow, .tokenmaxxing viewport, .tokenmaxxing drawingarea {{ background-color: {bg}; }}\n\
         .tokenmaxxing button.suggested-action {{ background: {accent}; color: #141413; font-weight: 600; border-radius: 8px; }}\n\
         .action-bar {{ background: {action_bg}; border: 1px solid {border}; border-radius: 14px; padding: 8px 10px; box-shadow: {shadow}; }}\n\
         .action-bar label {{ color: {muted}; margin: 0 6px; }}\n\
         popover.settings {{ background: {panel}; color: {text}; }}\n\
         popover.settings label {{ font-size: 13px; color: {text}; }}\n"
    )
}
