use crate::model::Severity;

/// An RGB triple in the 0.0..=1.0 range Cairo expects.
pub type Rgb = (f64, f64, f64);

pub const BG: Rgb = (0.039, 0.055, 0.078);
pub const PANEL: Rgb = (0.075, 0.101, 0.157);
pub const TRACK: Rgb = (0.13, 0.16, 0.21);
pub const CYAN: Rgb = (0.0, 0.898, 1.0);
pub const LIME: Rgb = (0.714, 1.0, 0.0);
pub const AMBER: Rgb = (1.0, 0.69, 0.0);
pub const MAGENTA: Rgb = (1.0, 0.18, 0.533);
pub const AZURE: Rgb = (0.23, 0.56, 0.98);
pub const VIOLET: Rgb = (0.66, 0.4, 0.98);
pub const TEAL: Rgb = (0.18, 0.83, 0.75);
pub const ORANGE: Rgb = (1.0, 0.48, 0.27);
pub const TEXT: Rgb = (0.86, 0.90, 0.95);
pub const MUTED: Rgb = (0.42, 0.48, 0.57);

/// An electric categorical ramp for series (models, providers) — distinct hues
/// that stay on-brand. Wraps for long lists.
pub const RAMP: [Rgb; 8] = [CYAN, LIME, VIOLET, AMBER, TEAL, MAGENTA, AZURE, ORANGE];

pub fn series_color(index: usize) -> Rgb {
    RAMP[index % RAMP.len()]
}

/// Colors for the five token tiers, in composition order.
pub const TOKEN_INPUT: Rgb = CYAN;
pub const TOKEN_OUTPUT: Rgb = LIME;
pub const TOKEN_CACHE_WRITE: Rgb = VIOLET;
pub const TOKEN_CACHE_READ: Rgb = AZURE;
pub const TOKEN_REASONING: Rgb = AMBER;

/// A gauge keeps its provider accent until it is stressed, then escalates.
pub fn gauge_color(accent: Rgb, severity: Severity) -> Rgb {
    match severity {
        Severity::Nominal => accent,
        Severity::Warn => AMBER,
        Severity::Critical => MAGENTA,
    }
}

/// Chrome stylesheet for the canvas dashboard. The panels themselves are drawn
/// in Cairo (see [`crate::render`]); this only styles the window chrome, the
/// scroll surface, and the screenshot-mode action bar. `scale` nudges the header
/// title size so it tracks the interface scale.
pub fn stylesheet(scale: f64) -> String {
    let title = (13.0 * scale).round() as i64;
    format!(
        "window.tokenmaxxing {{ background-color: #0A0E14; color: #DCE3F0; }}\n\
         .tokenmaxxing headerbar {{ background: #0A0E14; border: none; box-shadow: none; }}\n\
         .tokenmaxxing headerbar .title {{ font-weight: 800; font-size: {title}px; }}\n\
         .tokenmaxxing scrolledwindow, .tokenmaxxing viewport, .tokenmaxxing drawingarea {{ background-color: #0A0E14; }}\n\
         .action-bar {{ background: rgba(19, 26, 41, 0.92); border: 1px solid #223048; border-radius: 14px; padding: 8px 10px; box-shadow: 0 8px 24px rgba(0,0,0,0.5); }}\n\
         .action-bar label {{ color: #AEB8CC; margin: 0 6px; }}\n\
         popover.settings label {{ font-size: 13px; }}\n"
    )
}
