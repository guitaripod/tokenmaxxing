use gtk::cairo::{Context, Format, ImageSurface, LinearGradient, LineCap, RadialGradient};
use std::f64::consts::PI;
use std::path::Path;

/// Render the full-color app icon (squircle · glowing gauge · bolt) at `size`px
/// in the electric colorway, and write it as a PNG.
pub fn render(size: i32, path: &Path) -> Result<(), String> {
    let surface = ImageSurface::create(Format::ARgb32, size, size).map_err(|e| e.to_string())?;
    {
        let cr = Context::new(&surface).map_err(|e| e.to_string())?;
        draw(&cr, f64::from(size));
    }
    let mut file = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    surface.write_to_png(&mut file).map_err(|e| format!("write png: {e}"))
}

/// Paint the full app icon into a `s`×`s` box at the origin.
pub fn draw(cr: &Context, s: f64) {
    rounded_rect(cr, 0.0, 0.0, s, s, s * 0.225);
    cr.clip();

    let backdrop = LinearGradient::new(0.0, 0.0, s * 0.2, s);
    backdrop.add_color_stop_rgb(0.0, 0.063, 0.086, 0.125);
    backdrop.add_color_stop_rgb(1.0, 0.016, 0.027, 0.043);
    let _ = cr.set_source(&backdrop);
    let _ = cr.paint();

    let (cx, cy) = (s * 0.5, s * 0.47);

    let glow = RadialGradient::new(cx, cy, s * 0.04, cx, cy, s * 0.56);
    glow.add_color_stop_rgba(0.0, 0.0, 0.898, 1.0, 0.24);
    glow.add_color_stop_rgba(0.45, 0.714, 1.0, 0.0, 0.10);
    glow.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
    let _ = cr.set_source(&glow);
    let _ = cr.paint();

    let radius = s * 0.30;
    let stroke = s * 0.11;
    let start = -PI / 2.0;
    let sweep = 1.68 * PI;
    cr.set_line_cap(LineCap::Round);

    cr.set_line_width(stroke * 1.9);
    cr.set_source_rgba(0.0, 0.898, 1.0, 0.14);
    cr.arc(cx, cy, radius, start, start + sweep);
    let _ = cr.stroke();

    let ring = LinearGradient::new(cx - radius, cy - radius, cx + radius, cy + radius);
    ring.add_color_stop_rgb(0.0, 0.0, 0.898, 1.0);
    ring.add_color_stop_rgb(1.0, 0.714, 1.0, 0.0);
    cr.set_line_width(stroke);
    let _ = cr.set_source(&ring);
    cr.arc(cx, cy, radius, start, start + sweep);
    let _ = cr.stroke();

    let bolt = LinearGradient::new(cx, cy - radius * 0.8, cx, cy + radius * 0.8);
    bolt.add_color_stop_rgb(0.0, 0.94, 0.99, 1.0);
    bolt.add_color_stop_rgb(1.0, 0.0, 0.898, 1.0);
    let _ = cr.set_source(&bolt);
    bolt_path(cr, cx, cy, s * 0.5);
    let _ = cr.fill();
}

fn bolt_path(cr: &Context, cx: f64, cy: f64, box_size: f64) {
    let b = box_size;
    let (ox, oy) = (cx - b * 0.52, cy - b * 0.51);
    let points = [
        (0.585, 0.12),
        (0.34, 0.55),
        (0.5, 0.55),
        (0.415, 0.9),
        (0.7, 0.43),
        (0.52, 0.43),
    ];
    cr.new_path();
    cr.move_to(points[0].0 * b + ox, points[0].1 * b + oy);
    for point in &points[1..] {
        cr.line_to(point.0 * b + ox, point.1 * b + oy);
    }
    cr.close_path();
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}
