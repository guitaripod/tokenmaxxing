//! Hand-drawn Cairo chart primitives for the dashboard. Each takes an already
//! prepared data slice and paints into a `w`×`h` box anchored at the origin, so
//! the same code serves both the live `DrawingArea`s and the share-card render.

use crate::theme::{self, Rgb};
use gtk::cairo::{Context, FontSlant, FontWeight, LineCap, LinearGradient};
use std::f64::consts::PI;

pub struct BarRow {
    pub label: String,
    pub value: f64,
    pub caption: String,
    pub color: Rgb,
}

pub struct Slice {
    pub value: f64,
    pub color: Rgb,
}

/// A filled area chart of one series (daily cost or tokens), with a bright top
/// line, a highlighted last point, and a faint peak marker.
pub fn area(cr: &Context, w: f64, h: f64, series: &[f64], accent: Rgb) {
    let pad = h * 0.14;
    let plot_w = w - pad * 2.0;
    let plot_h = h - pad * 2.0;
    baseline(cr, pad, h - pad, w - pad);
    if series.len() < 2 {
        return;
    }
    let max = series.iter().cloned().fold(0.0_f64, f64::max).max(f64::MIN_POSITIVE);
    let point = |i: usize, v: f64| {
        let x = pad + (i as f64 / (series.len() - 1) as f64) * plot_w;
        let y = (h - pad) - (v / max) * plot_h;
        (x, y)
    };

    cr.new_path();
    cr.move_to(pad, h - pad);
    for (i, &v) in series.iter().enumerate() {
        let (x, y) = point(i, v);
        cr.line_to(x, y);
    }
    cr.line_to(w - pad, h - pad);
    cr.close_path();
    let fill = LinearGradient::new(0.0, pad, 0.0, h - pad);
    fill.add_color_stop_rgba(0.0, accent.0, accent.1, accent.2, 0.34);
    fill.add_color_stop_rgba(1.0, accent.0, accent.1, accent.2, 0.015);
    let _ = cr.set_source(&fill);
    let _ = cr.fill();

    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    cr.set_line_width((h * 0.022).max(1.4));
    set(cr, accent);
    cr.new_path();
    for (i, &v) in series.iter().enumerate() {
        let (x, y) = point(i, v);
        if i == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();

    let (lx, ly) = point(series.len() - 1, *series.last().unwrap());
    set_alpha(cr, accent, 0.28);
    cr.arc(lx, ly, h * 0.052, 0.0, 2.0 * PI);
    let _ = cr.fill();
    set(cr, accent);
    cr.arc(lx, ly, h * 0.03, 0.0, 2.0 * PI);
    let _ = cr.fill();
}

/// A top-N horizontal bar ranking: label left, caption right, proportional bar.
pub fn bars(cr: &Context, w: f64, h: f64, rows: &[BarRow]) {
    if rows.is_empty() {
        return;
    }
    let n = rows.len();
    let row_h = h / n as f64;
    let max = rows.iter().map(|r| r.value).fold(0.0_f64, f64::max).max(f64::MIN_POSITIVE);
    let bar_h = (row_h * 0.24).clamp(4.0, 10.0);
    let text_size = (row_h * 0.34).clamp(11.0, 16.0);

    for (i, row) in rows.iter().enumerate() {
        let y = i as f64 * row_h;
        let label_baseline = y + row_h * 0.42;
        font(cr, text_size, FontWeight::Normal);
        set(cr, (0.72, 0.77, 0.85));
        text_left(cr, &ellipsize(cr, &row.label, w * 0.62), 0.0, label_baseline);
        font(cr, text_size * 0.92, FontWeight::Bold);
        set(cr, theme::TEXT);
        text_right(cr, &row.caption, w, label_baseline);

        let track_y = y + row_h * 0.62;
        rounded(cr, 0.0, track_y, w, bar_h, bar_h / 2.0);
        set(cr, theme::TRACK);
        let _ = cr.fill();
        let frac = (row.value / max).clamp(0.0, 1.0);
        if frac > 0.0 {
            rounded(cr, 0.0, track_y, (w * frac).max(bar_h), bar_h, bar_h / 2.0);
            set(cr, row.color);
            let _ = cr.fill();
        }
    }
}

/// A donut of proportional slices with two lines of centered text.
pub fn donut(cr: &Context, w: f64, h: f64, slices: &[Slice], center_top: &str, center_bottom: &str) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let radius = (w.min(h) / 2.0) - h * 0.06;
    let thickness = radius * 0.44;
    let total: f64 = slices.iter().map(|s| s.value).sum();

    cr.set_line_cap(LineCap::Butt);
    cr.set_line_width(thickness);
    set(cr, theme::TRACK);
    cr.arc(cx, cy, radius - thickness / 2.0, 0.0, 2.0 * PI);
    let _ = cr.stroke();

    if total > 0.0 {
        let gap = 0.03;
        let mut start = -PI / 2.0;
        for s in slices {
            let sweep = (s.value / total) * (2.0 * PI);
            if sweep <= 0.0 {
                continue;
            }
            set(cr, s.color);
            cr.arc(cx, cy, radius - thickness / 2.0, start + gap, start + sweep - gap.min(sweep / 2.0));
            let _ = cr.stroke();
            start += sweep;
        }
    }

    font(cr, radius * 0.44, FontWeight::Bold);
    set(cr, theme::TEXT);
    centered(cr, center_top, cx, cy - radius * 0.02);
    font(cr, radius * 0.2, FontWeight::Normal);
    set(cr, theme::MUTED);
    centered(cr, center_bottom, cx, cy + radius * 0.32);
}

/// A single proportional stacked bar (token composition, free/paid split, …).
pub fn stacked_bar(cr: &Context, x: f64, y: f64, w: f64, h: f64, segments: &[Slice]) {
    let total: f64 = segments.iter().map(|s| s.value).sum();
    rounded(cr, x, y, w, h, h / 2.0);
    set(cr, theme::TRACK);
    let _ = cr.fill();
    if total <= 0.0 {
        return;
    }
    cr.save().ok();
    rounded(cr, x, y, w, h, h / 2.0);
    cr.clip();
    let mut cx = x;
    for s in segments {
        let seg_w = (s.value / total) * w;
        if seg_w <= 0.0 {
            continue;
        }
        cr.rectangle(cx, y, seg_w, h);
        set(cr, s.color);
        let _ = cr.fill();
        cx += seg_w;
    }
    cr.restore().ok();
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// A 7×24 activity punch card: rows are weekdays (Mon top), columns are hours,
/// cell brightness scales with activity (square-root so light days stay visible).
pub fn heatmap(cr: &Context, w: f64, h: f64, counts: &[[u64; 24]; 7], max: u64, accent: Rgb) {
    let gutter_l = w * 0.075;
    let gutter_b = h * 0.16;
    let grid_w = w - gutter_l;
    let grid_h = h - gutter_b;
    let cell_w = grid_w / 24.0;
    let cell_h = grid_h / 7.0;
    let inset = (cell_w.min(cell_h) * 0.12).min(2.0);
    let max = max.max(1) as f64;

    font(cr, (cell_h * 0.44).clamp(8.0, 13.0), FontWeight::Normal);
    for (row, label) in WEEKDAYS.iter().enumerate() {
        let cy = row as f64 * cell_h;
        set(cr, theme::MUTED);
        text_right(cr, label, gutter_l - inset * 2.0, cy + cell_h * 0.66);
        for hour in 0..24 {
            let x = gutter_l + hour as f64 * cell_w;
            let count = counts[row][hour];
            rounded(cr, x + inset, cy + inset, cell_w - inset * 2.0, cell_h - inset * 2.0, inset.max(1.5));
            if count == 0 {
                set_alpha(cr, theme::TRACK, 0.5);
            } else {
                let intensity = (count as f64 / max).sqrt().clamp(0.12, 1.0);
                set_alpha(cr, accent, intensity);
            }
            let _ = cr.fill();
        }
    }

    font(cr, (cell_h * 0.4).clamp(8.0, 12.0), FontWeight::Normal);
    set(cr, theme::MUTED);
    for hour in [0, 6, 12, 18, 23] {
        let x = gutter_l + (hour as f64 + 0.5) * cell_w;
        centered(cr, &format!("{hour}"), x, h - gutter_b * 0.28);
    }
}

fn baseline(cr: &Context, x0: f64, y: f64, x1: f64) {
    set_alpha(cr, theme::TRACK, 0.8);
    cr.set_line_width(1.0);
    cr.move_to(x0, y);
    cr.line_to(x1, y);
    let _ = cr.stroke();
}

fn ellipsize(cr: &Context, text: &str, max_w: f64) -> String {
    if cr.text_extents(text).map(|e| e.width()).unwrap_or(0.0) <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while out.chars().count() > 1
        && cr.text_extents(&format!("{out}…")).map(|e| e.width()).unwrap_or(0.0) > max_w
    {
        out.pop();
    }
    format!("{out}…")
}

fn font(cr: &Context, size: f64, weight: FontWeight) {
    cr.select_font_face("sans-serif", FontSlant::Normal, weight);
    cr.set_font_size(size);
}

fn text_left(cr: &Context, s: &str, x: f64, baseline: f64) {
    cr.move_to(x, baseline);
    let _ = cr.show_text(s);
}

fn text_right(cr: &Context, s: &str, right: f64, baseline: f64) {
    let width = cr.text_extents(s).map(|e| e.width()).unwrap_or(0.0);
    cr.move_to(right - width, baseline);
    let _ = cr.show_text(s);
}

fn centered(cr: &Context, s: &str, cx: f64, baseline: f64) {
    let ext = match cr.text_extents(s) {
        Ok(e) => e,
        Err(_) => return,
    };
    cr.move_to(cx - ext.width() / 2.0 - ext.x_bearing(), baseline);
    let _ = cr.show_text(s);
}

fn rounded(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

fn set(cr: &Context, c: Rgb) {
    cr.set_source_rgb(c.0, c.1, c.2);
}

fn set_alpha(cr: &Context, c: Rgb, a: f64) {
    cr.set_source_rgba(c.0, c.1, c.2, a);
}
