use crate::gauge;
use crate::model::{Authority, Gauge, Snapshot, Unit};
use crate::theme::{self, Rgb};
use gtk::cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use std::path::Path;

const W: f64 = 1180.0;
const MARGIN: f64 = 52.0;
const HEADER_H: f64 = 170.0;
const CARD_GAP: f64 = 26.0;
const CARD_PAD: f64 = 40.0;
const NAME_ROW: f64 = 100.0;
const LABEL_LINE: f64 = 30.0;
const SUB_LINE: f64 = 27.0;
const NOTE_LINE: f64 = 30.0;
const DETAIL_ROW: f64 = 32.0;
const FOOTER_H: f64 = 84.0;
const MUTED_GREEN: Rgb = (0.443, 0.878, 0.776);

/// Render the current quota state as a standalone, high-resolution share image.
pub fn render(snapshots: &[Snapshot], path: &Path) -> Result<(), String> {
    let measure = measuring_context()?;
    let cards: Vec<CardLayout> = snapshots
        .iter()
        .map(|snap| CardLayout::measure(&measure, snap))
        .collect();

    let height = MARGIN + HEADER_H
        + cards.iter().map(|c| c.height + CARD_GAP).sum::<f64>()
        + FOOTER_H
        + MARGIN;

    let surface = ImageSurface::create(Format::ARgb32, W as i32, height as i32)
        .map_err(|e| e.to_string())?;
    {
        let cr = Context::new(&surface).map_err(|e| e.to_string())?;
        paint_background(&cr, height);
        paint_header(&cr);

        let mut y = MARGIN + HEADER_H;
        for (snap, layout) in snapshots.iter().zip(&cards) {
            paint_card(&cr, snap, layout, y);
            y += layout.height + CARD_GAP;
        }
        paint_footer(&cr, height);
    }

    let mut file = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    surface
        .write_to_png(&mut file)
        .map_err(|e| format!("write png: {e}"))
}

struct CardLayout {
    note_lines: Vec<String>,
    columns: usize,
    ring_diameter: f64,
    cell_height: f64,
    height: f64,
}

impl CardLayout {
    fn measure(cr: &Context, snap: &Snapshot) -> Self {
        let note_width = W - 2.0 * MARGIN - 2.0 * CARD_PAD;
        let note_lines = snap
            .note
            .as_deref()
            .map(|text| wrap(cr, text, note_width, 22.0, usize::MAX))
            .unwrap_or_default();

        let count = snap.gauges.len().max(1);
        let columns = count.min(3);
        let rows = count.div_ceil(columns);
        let inner = W - 2.0 * MARGIN - 2.0 * CARD_PAD;
        let cell_w = inner / columns as f64;
        let ring_diameter = (cell_w * 0.56).min(196.0);
        let cell_height = ring_diameter + 16.0 + 2.0 * LABEL_LINE + 2.0 * SUB_LINE + 8.0;
        let grid = rows as f64 * cell_height;

        let note_block = if note_lines.is_empty() {
            0.0
        } else {
            note_lines.len() as f64 * NOTE_LINE + 16.0
        };
        let details_block = if snap.details.is_empty() {
            0.0
        } else {
            snap.details.len() as f64 * DETAIL_ROW + 24.0
        };
        let error_block = if snap.error.is_some() { 44.0 } else { 0.0 };
        let height = CARD_PAD + NAME_ROW + grid + note_block + details_block + error_block + CARD_PAD;

        Self {
            note_lines,
            columns,
            ring_diameter,
            cell_height,
            height,
        }
    }
}

fn paint_background(cr: &Context, height: f64) {
    set(cr, theme::BG);
    cr.rectangle(0.0, 0.0, W, height);
    let _ = cr.fill();
}

fn paint_header(cr: &Context) {
    cr.save().ok();
    cr.translate(MARGIN, MARGIN + 8.0);
    gauge::draw_logo(cr, 86.0);
    cr.restore().ok();

    text(cr, "tokenmaxxing", MARGIN + 108.0, MARGIN + 58.0, 52.0, FontWeight::Bold, theme::CYAN, false);
    text(cr, "LLM token quotas", MARGIN + 110.0, MARGIN + 96.0, 24.0, FontWeight::Normal, theme::MUTED, false);

    let stamp = chrono::Local::now().format("%Y-%m-%d  %H:%M").to_string();
    text_right(cr, &stamp, W - MARGIN, MARGIN + 58.0, 24.0, FontWeight::Normal, theme::MUTED, true);

    set(cr, theme::TRACK);
    cr.set_line_width(2.0);
    cr.move_to(MARGIN, MARGIN + HEADER_H - 24.0);
    cr.line_to(W - MARGIN, MARGIN + HEADER_H - 24.0);
    let _ = cr.stroke();
}

fn paint_card(cr: &Context, snap: &Snapshot, layout: &CardLayout, y: f64) {
    let x = MARGIN;
    let width = W - 2.0 * MARGIN;
    rounded_rect(cr, x, y, width, layout.height, 26.0);
    set(cr, (0.075, 0.101, 0.157));
    let _ = cr.fill();
    rounded_rect(cr, x, y, width, layout.height, 26.0);
    set(cr, (0.118, 0.157, 0.220));
    cr.set_line_width(1.5);
    let _ = cr.stroke();

    let accent = theme::provider_accent(&snap.provider_id);

    text(cr, &snap.provider_name, x + CARD_PAD, y + CARD_PAD + 34.0, 30.0, FontWeight::Bold, accent, false);
    text(cr, &snap.subtitle, x + CARD_PAD, y + CARD_PAD + 64.0, 20.0, FontWeight::Normal, theme::MUTED, false);
    text(cr, &snap.source, x + CARD_PAD, y + CARD_PAD + 90.0, 18.0, FontWeight::Normal, theme::MUTED, true);
    paint_badge(cr, snap.authority, x + width - CARD_PAD, y + CARD_PAD + 20.0);

    let grid_top = y + CARD_PAD + NAME_ROW;
    let inner = width - 2.0 * CARD_PAD;
    let cell_w = inner / layout.columns as f64;
    for (i, gauge) in snap.gauges.iter().enumerate() {
        let col = i % layout.columns;
        let row = i / layout.columns;
        let cell_cx = x + CARD_PAD + col as f64 * cell_w + cell_w / 2.0;
        let cell_top = grid_top + row as f64 * layout.cell_height;
        paint_gauge_cell(cr, gauge, accent, cell_cx, cell_top, cell_w, layout.ring_diameter);
    }

    let rows = snap.gauges.len().max(1).div_ceil(layout.columns);
    let mut ny = grid_top + rows as f64 * layout.cell_height + 8.0;
    for line in &layout.note_lines {
        text(cr, line, x + CARD_PAD, ny, 22.0, FontWeight::Normal, MUTED_GREEN, false);
        ny += NOTE_LINE;
    }

    if !snap.details.is_empty() {
        ny += 14.0;
        set(cr, theme::TRACK);
        cr.set_line_width(1.5);
        cr.move_to(x + CARD_PAD, ny);
        cr.line_to(x + width - CARD_PAD, ny);
        let _ = cr.stroke();
        ny += 28.0;
        for (key, value) in &snap.details {
            text(cr, key, x + CARD_PAD, ny, 21.0, FontWeight::Normal, theme::MUTED, false);
            text_right(cr, value, x + width - CARD_PAD, ny, 21.0, FontWeight::Bold, theme::TEXT, true);
            ny += DETAIL_ROW;
        }
    }

    if let Some(err) = &snap.error {
        text(cr, err, x + CARD_PAD, ny + 8.0, 22.0, FontWeight::Normal, theme::MAGENTA, false);
    }
}

fn paint_gauge_cell(cr: &Context, g: &Gauge, accent: Rgb, cx: f64, top: f64, cell_w: f64, ring_d: f64) {
    let color = theme::gauge_color(accent, g.severity());
    cr.save().ok();
    cr.translate(cx - ring_d / 2.0, top);
    gauge::draw_ring(cr, ring_d as i32, ring_d as i32, g.fraction, color, &g.percent_text(), "");
    cr.restore().ok();

    let text_width = cell_w - 16.0;
    let label_lines = wrap(cr, &g.label, text_width, 22.0, 2);
    let mut ty = top + ring_d + 28.0;
    for line in &label_lines {
        centered(cr, line, cx, ty, 22.0, FontWeight::Normal, (0.682, 0.722, 0.800), false);
        ty += LABEL_LINE;
    }

    ty = top + ring_d + 28.0 + 2.0 * LABEL_LINE;
    if let Some(sub) = sub_line(g) {
        for line in wrap(cr, &sub, text_width, 18.0, 2) {
            centered(cr, &line, cx, ty, 18.0, FontWeight::Normal, theme::MUTED, true);
            ty += SUB_LINE;
        }
    }
}

fn paint_badge(cr: &Context, authority: Authority, right: f64, y: f64) {
    let label = authority.badge();
    let (bg, fg) = match authority {
        Authority::Live => (theme::CYAN, (0.016, 0.133, 0.165)),
        Authority::Estimated => (theme::LIME, (0.102, 0.149, 0.0)),
        Authority::Unavailable => (theme::MAGENTA, (0.165, 0.027, 0.078)),
    };
    cr.select_font_face("sans-serif", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(20.0);
    let tw = cr.text_extents(label).map(|e| e.width()).unwrap_or(40.0);
    let pill_w = tw + 34.0;
    let pill_h = 34.0;
    let x = right - pill_w;
    rounded_rect(cr, x, y, pill_w, pill_h, pill_h / 2.0);
    set(cr, bg);
    let _ = cr.fill();
    set(cr, fg);
    cr.move_to(x + 17.0, y + 23.0);
    let _ = cr.show_text(label);
}

fn paint_footer(cr: &Context, height: f64) {
    let y = height - MARGIN - 12.0;
    set(cr, theme::TRACK);
    cr.set_line_width(2.0);
    cr.move_to(MARGIN, y - 34.0);
    cr.line_to(W - MARGIN, y - 34.0);
    let _ = cr.stroke();
    text(cr, "github.com/guitaripod/tokenmaxxing", MARGIN, y, 21.0, FontWeight::Normal, theme::MUTED, true);
    text_right(cr, "tokenmaxxing 0.1.0", W - MARGIN, y, 21.0, FontWeight::Normal, theme::MUTED, true);
}

fn sub_line(g: &Gauge) -> Option<String> {
    let mut parts = Vec::new();
    if let (Unit::Usd, Some(u), Some(l)) = (g.unit, g.used, g.limit) {
        parts.push(format!("${u:.2} / ${l:.0}"));
    }
    if let Some(detail) = &g.detail {
        parts.push(detail.clone());
    }
    if let Some(reset) = g.resets_at {
        let seconds = (reset - chrono::Utc::now()).num_seconds().max(0);
        let human = if seconds < 3_600 {
            format!("{}m", seconds / 60)
        } else if seconds < 86_400 {
            format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
        } else {
            format!("{}d {}h", seconds / 86_400, (seconds % 86_400) / 3_600)
        };
        parts.push(if g.trusted_reset {
            format!("resets {human}")
        } else {
            format!("~resets {human}")
        });
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn measuring_context() -> Result<Context, String> {
    let surface = ImageSurface::create(Format::ARgb32, 4, 4).map_err(|e| e.to_string())?;
    Context::new(&surface).map_err(|e| e.to_string())
}

fn wrap(cr: &Context, text: &str, max_width: f64, size: f64, max_lines: usize) -> Vec<String> {
    cr.select_font_face("sans-serif", FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(size);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        let width = cr.text_extents(&candidate).map(|e| e.width()).unwrap_or(0.0);
        if width > max_width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
            if lines.len() == max_lines {
                break;
            }
        } else {
            current = candidate;
        }
    }
    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[allow(clippy::too_many_arguments)]
fn text(cr: &Context, s: &str, x: f64, y: f64, size: f64, weight: FontWeight, color: Rgb, mono: bool) {
    cr.select_font_face(font(mono), FontSlant::Normal, weight);
    cr.set_font_size(size);
    set(cr, color);
    cr.move_to(x, y);
    let _ = cr.show_text(s);
}

#[allow(clippy::too_many_arguments)]
fn text_right(cr: &Context, s: &str, right: f64, y: f64, size: f64, weight: FontWeight, color: Rgb, mono: bool) {
    cr.select_font_face(font(mono), FontSlant::Normal, weight);
    cr.set_font_size(size);
    let width = cr.text_extents(s).map(|e| e.width()).unwrap_or(0.0);
    set(cr, color);
    cr.move_to(right - width, y);
    let _ = cr.show_text(s);
}

#[allow(clippy::too_many_arguments)]
fn centered(cr: &Context, s: &str, cx: f64, y: f64, size: f64, weight: FontWeight, color: Rgb, mono: bool) {
    cr.select_font_face(font(mono), FontSlant::Normal, weight);
    cr.set_font_size(size);
    let width = cr.text_extents(s).map(|e| e.width()).unwrap_or(0.0);
    set(cr, color);
    cr.move_to(cx - width / 2.0, y);
    let _ = cr.show_text(s);
}

fn font(mono: bool) -> &'static str {
    if mono {
        "monospace"
    } else {
        "sans-serif"
    }
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    let r = r.min(w / 2.0).min(h / 2.0);
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

/// Default location for exported cards: `$XDG_PICTURES_DIR` (or ~/Pictures).
pub fn default_output() -> std::path::PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let pictures = std::env::var_os("XDG_PICTURES_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::creds::home().join("Pictures"));
    let dir = if pictures.is_dir() {
        pictures
    } else {
        crate::creds::home()
    };
    dir.join(format!("tokenmaxxing-{stamp}.png"))
}
