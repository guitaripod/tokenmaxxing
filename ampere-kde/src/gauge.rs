use crate::theme::{self, Rgb};
use gtk::cairo::{Context, FontSlant, FontWeight, LineCap};
use std::f64::consts::PI;

/// Draw a circular quota ring with an electric glow and a centered percentage.
pub fn draw_ring(cr: &Context, width: i32, height: i32, fraction: f64, color: Rgb, center: &str, sub: &str) {
    let (w, h) = (f64::from(width), f64::from(height));
    let (cx, cy) = (w / 2.0, h / 2.0);
    let extent = w.min(h);
    let radius = extent / 2.0 - extent * 0.11;
    let stroke = radius * 0.22;
    let start = -PI / 2.0;
    let sweep = fraction.clamp(0.0, 1.0) * 2.0 * PI;

    cr.set_line_cap(LineCap::Round);
    cr.new_path();

    cr.set_line_width(stroke);
    set(cr, theme::TRACK);
    cr.arc(cx, cy, radius, 0.0, 2.0 * PI);
    let _ = cr.stroke();

    if sweep > 0.0 {
        cr.set_line_width(stroke * 2.1);
        set_alpha(cr, color, 0.16);
        cr.arc(cx, cy, radius, start, start + sweep);
        let _ = cr.stroke();

        cr.set_line_width(stroke);
        set(cr, color);
        cr.arc(cx, cy, radius, start, start + sweep);
        let _ = cr.stroke();
    }

    cr.select_font_face("monospace", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(radius * 0.46);
    set(cr, theme::TEXT);
    centered_text(cr, center, cx, cy - radius * 0.05);

    cr.select_font_face("sans-serif", FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(radius * 0.17);
    set(cr, theme::MUTED);
    centered_text(cr, sub, cx, cy + radius * 0.38);
}

/// Draw the Ampere mark — a lime current-arc wrapping a cyan bolt — filling a
/// `size`×`size` box anchored at the origin. Shared by the tray icon and the
/// share card so the brand stays identical everywhere.
pub fn draw_logo(cr: &Context, size: f64) {
    cr.set_line_cap(LineCap::Round);
    cr.set_line_width(size * 0.07);
    cr.set_source_rgba(theme::LIME.0, theme::LIME.1, theme::LIME.2, 0.85);
    cr.arc(size * 0.5, size * 0.5, size * 0.4, PI * 0.62, PI * 2.55);
    let _ = cr.stroke();

    set(cr, theme::CYAN);
    let bolt = [
        (0.585, 0.12),
        (0.34, 0.55),
        (0.5, 0.55),
        (0.415, 0.9),
        (0.7, 0.43),
        (0.52, 0.43),
    ];
    cr.move_to(bolt[0].0 * size, bolt[0].1 * size);
    for point in &bolt[1..] {
        cr.line_to(point.0 * size, point.1 * size);
    }
    cr.close_path();
    let _ = cr.fill();
}

fn centered_text(cr: &Context, text: &str, cx: f64, baseline_center: f64) {
    if let Ok(ext) = cr.text_extents(text) {
        cr.move_to(
            cx - ext.width() / 2.0 - ext.x_bearing(),
            baseline_center + ext.height() / 2.0,
        );
        let _ = cr.show_text(text);
    }
}

fn set(cr: &Context, c: Rgb) {
    cr.set_source_rgb(c.0, c.1, c.2);
}

fn set_alpha(cr: &Context, c: Rgb, a: f64) {
    cr.set_source_rgba(c.0, c.1, c.2, a);
}
