use crate::model::Severity;

/// An RGB triple in the 0.0..=1.0 range Cairo expects.
pub type Rgb = (f64, f64, f64);

pub const BG: Rgb = (0.039, 0.055, 0.078);
pub const TRACK: Rgb = (0.13, 0.16, 0.21);
pub const CYAN: Rgb = (0.0, 0.898, 1.0);
pub const LIME: Rgb = (0.714, 1.0, 0.0);
pub const AMBER: Rgb = (1.0, 0.69, 0.0);
pub const MAGENTA: Rgb = (1.0, 0.18, 0.533);
pub const TEXT: Rgb = (0.86, 0.90, 0.95);
pub const MUTED: Rgb = (0.42, 0.48, 0.57);

/// The resting accent for a provider, used when a gauge is not warning/critical.
pub fn provider_accent(provider_id: &str) -> Rgb {
    match provider_id {
        "anthropic" => CYAN,
        _ => LIME,
    }
}

pub fn provider_accent_class(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "accent-cyan",
        _ => "accent-lime",
    }
}

/// A gauge keeps its provider accent until it is stressed, then escalates.
pub fn gauge_color(accent: Rgb, severity: Severity) -> Rgb {
    match severity {
        Severity::Nominal => accent,
        Severity::Warn => AMBER,
        Severity::Critical => MAGENTA,
    }
}

/// Build the stylesheet with every dimension multiplied by the interface scale,
/// so a single control resizes the whole UI.
pub fn stylesheet(scale: f64) -> String {
    let px = |base: f64| (base * scale).round() as i64;
    format!(
        "window.ampere {{ background-color: #0A0E14; color: #DCE3F0; }}\n\
         .ampere headerbar {{ background: #0A0E14; border: none; box-shadow: none; min-height: {header}px; }}\n\
         .ampere headerbar .title {{ font-weight: 800; font-size: {title}px; }}\n\
         .card {{ background: linear-gradient(155deg, #131A29, #0D121C); border: 1px solid #1E2838; border-radius: {radius}px; padding: {pad_v}px {pad_h}px; }}\n\
         .provider-name {{ font-weight: 800; font-size: {name}px; }}\n\
         .accent-cyan {{ color: #00E5FF; }}\n\
         .accent-lime {{ color: #B6FF00; }}\n\
         .subtitle {{ font-size: {subtitle}px; }}\n\
         .muted {{ color: #6B7688; }}\n\
         .footer {{ font-size: {footer}px; font-family: monospace; }}\n\
         .badge {{ font-size: {badge}px; font-weight: 800; padding: {badge_v}px {badge_h}px; border-radius: 999px; }}\n\
         .badge-live {{ background: #00E5FF; color: #04222A; }}\n\
         .badge-est {{ background: #B6FF00; color: #1A2600; }}\n\
         .badge-offline {{ background: #FF2E88; color: #2A0714; }}\n\
         .metric-label {{ font-size: {mlabel}px; color: #AEB8CC; }}\n\
         .metric-value {{ font-size: {mvalue}px; font-weight: 700; font-family: monospace; color: #EAF0FA; }}\n\
         .metric-sub {{ font-size: {msub}px; color: #6B7688; font-family: monospace; }}\n\
         .note {{ font-size: {note}px; color: #71E0C6; margin-top: 6px; }}\n\
         .error {{ font-size: {error}px; color: #FF2E88; margin-top: 6px; }}\n\
         popover.settings label {{ font-size: {mlabel}px; }}\n",
        header = px(42.0),
        title = px(13.0),
        radius = px(16.0),
        pad_v = px(14.0),
        pad_h = px(16.0),
        name = px(16.0),
        subtitle = px(12.0),
        footer = px(12.0),
        badge = px(10.5),
        badge_v = px(2.0),
        badge_h = px(9.0),
        mlabel = px(13.0),
        mvalue = px(13.0),
        msub = px(11.0),
        note = px(11.5),
        error = px(12.0),
    )
}
