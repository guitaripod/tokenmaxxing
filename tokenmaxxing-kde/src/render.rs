//! The dashboard's single layout-and-paint engine. Everything the user sees is
//! composed here into a Cairo context: the live window embeds it in a resizable
//! `DrawingArea`, and the screenshot/share export re-runs the exact same code
//! into a PNG — so exports are pixel-for-pixel what's on screen (WYSIWYG), and
//! selecting a subset of panels for a screenshot is just filtering the card list.

use crate::charts::{self, BarRow, Slice};
use crate::format;
use crate::gauge;
use crate::model::{Authority, Dashboard, Gauge, Segment, Snapshot, Usage};
use crate::theme::{self, Rgb};
use gtk::cairo::{Context, FontSlant, FontWeight};
use std::collections::HashSet;

const MARGIN: f64 = 16.0;
const GAP: f64 = 12.0;
const PAD: f64 = 14.0;
const RADIUS: f64 = 14.0;
const TITLE_H: f64 = 22.0;
const LIMITS_GAP: f64 = 10.0;

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

/// A laid-out unit: a section header band or a content panel, with the width
/// hint the flow packer honours.
struct Card {
    id: String,
    grow: f64,
    min_w: f64,
    height: f64,
    panel: Panel,
}

enum Panel {
    Section { title: String, badge: Option<Authority>, source: Option<String>, accent: Rgb },
    /// Compact provider strip used by the limits (small) window.
    LimitsProvider {
        name: String,
        subtitle: String,
        badge: Authority,
        gauges: Vec<Gauge>,
        accent: Rgb,
        error: Option<String>,
    },
    Kpi { value: String, label: String, sub: Option<String>, accent: Rgb },
    HeroRing { gauge: Gauge, accent: Rgb, authority: Authority },
    ResetHorizon { title: String, ticks: Vec<ResetTick> },
    Rings { title: String, gauges: Vec<Gauge>, accent: Rgb },
    Callout { title: String, headline: String, body: String, accent: Rgb },
    Area { title: String, series: Vec<f64>, accent: Rgb, caption: String },
    Bars { title: String, rows: Vec<BarRow>, caption: String },
    Donut { title: String, slices: Vec<Slice>, center_top: String, center_bottom: String, legend: Vec<(String, Rgb, String)> },
    Heatmap { title: String, counts: Box<[[u64; 24]; 7]>, max: u64, accent: Rgb, caption: String },
    Composition { title: String, segments: Vec<Slice>, legend: Vec<(String, Rgb, String)>, caption: String },
    Stat { title: String, rows: Vec<(String, String)> },
}

/// A placed card ready to paint, plus the id for hit-testing during selection.
struct Placed {
    rect: Rect,
    card: Card,
}

pub struct Plan {
    pub height: f64,
    placed: Vec<Placed>,
}

impl Plan {
    /// Ids of the content panels (not section headers) at a point — for
    /// click-to-select in screenshot mode.
    pub fn panel_at(&self, x: f64, y: f64) -> Option<String> {
        self.placed
            .iter()
            .find(|p| !matches!(p.card.panel, Panel::Section { .. }) && p.rect.contains(x, y))
            .map(|p| p.card.id.clone())
    }

    /// Every selectable panel id, in visual order — for "select all".
    pub fn selectable_ids(&self) -> Vec<String> {
        self.placed
            .iter()
            .filter(|p| !matches!(p.card.panel, Panel::Section { .. }))
            .map(|p| p.card.id.clone())
            .collect()
    }
}

/// One upcoming reset marker on the cross-provider horizon timeline.
struct ResetTick {
    label: String,
    seconds: i64,
    trusted: bool,
    color: Rgb,
}

pub struct PaintOpts<'a> {
    pub selecting: bool,
    pub selected: &'a HashSet<String>,
}

/// Which slice of the dashboard to build: just the current-limits sections (the
/// compact default view) or the whole thing (the full analytics dashboard).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Limits,
    Full,
}

/// Lay the whole dashboard out at the given content width using a greedy flow
/// packer: section headers take a full line, panels pack left-to-right until the
/// next one wouldn't fit, then share the line by their grow weights.
pub fn plan(dash: &Dashboard, width: f64, scope: Scope) -> Plan {
    let cards = build_cards(dash, scope);
    let gap = if scope == Scope::Limits { LIMITS_GAP } else { GAP };
    let margin = if scope == Scope::Limits { 14.0 } else { MARGIN };
    let usable = (width - margin * 2.0).max(200.0);
    let mut placed = Vec::with_capacity(cards.len());
    let mut y = margin;
    let mut iter = cards.into_iter().peekable();

    while let Some(card) = iter.next() {
        if matches!(card.panel, Panel::Section { .. }) {
            let h = card.height;
            placed.push(Placed { rect: Rect { x: margin, y, w: usable, h }, card });
            y += h + gap * 0.55;
            continue;
        }

        // Limits cards always stack full-width — never squeeze three providers
        // onto one row of half-sized rings.
        if matches!(card.panel, Panel::LimitsProvider { .. }) {
            let h = card.height;
            placed.push(Placed { rect: Rect { x: margin, y, w: usable, h }, card });
            y += h + gap;
            continue;
        }

        let mut line = vec![card];
        let mut used = line[0].min_w;
        while let Some(next) = iter.peek() {
            if matches!(next.panel, Panel::Section { .. } | Panel::LimitsProvider { .. }) {
                break;
            }
            if used + gap + next.min_w > usable {
                break;
            }
            used += gap + next.min_w;
            line.push(iter.next().unwrap());
        }

        let total_grow: f64 = line.iter().map(|c| c.grow).sum::<f64>().max(0.001);
        let avail = usable - gap * (line.len() as f64 - 1.0);
        let row_h = line.iter().map(|c| c.height).fold(0.0, f64::max);
        let mut x = margin;
        for card in line {
            let w = avail * card.grow / total_grow;
            placed.push(Placed { rect: Rect { x, y, w, h: row_h }, card });
            x += w + gap;
        }
        y += row_h + gap;
    }

    Plan { height: y - gap + margin, placed }
}

pub fn paint(cr: &Context, plan: &Plan, width: f64, opts: &PaintOpts) {
    background(cr, width, plan.height);
    for item in &plan.placed {
        let selected = opts.selected.contains(&item.card.id);
        let is_panel = !matches!(item.card.panel, Panel::Section { .. });
        if opts.selecting && is_panel && !selected {
            cr.push_group();
        }
        paint_card(cr, item.rect, &item.card);
        if opts.selecting && is_panel {
            if selected {
                let r = &item.rect;
                rounded(cr, r.x, r.y, r.w, r.h, RADIUS);
                set(cr, theme::CYAN);
                cr.set_line_width(2.5);
                let _ = cr.stroke();
            } else {
                cr.pop_group_to_source().ok();
                cr.paint_with_alpha(0.35).ok();
            }
        }
    }
}

fn paint_card(cr: &Context, r: Rect, card: &Card) {
    match &card.panel {
        Panel::Section { title, badge, source, accent } => {
            paint_section(cr, r, title, *badge, source.as_deref(), *accent)
        }
        panel => {
            rounded(cr, r.x, r.y, r.w, r.h, RADIUS);
            set(cr, theme::panel());
            let _ = cr.fill();
            // Hairline edge — cool border, not a harsh track ring.
            rounded(cr, r.x, r.y, r.w, r.h, RADIUS);
            set_alpha(cr, theme::border(), if theme::is_dark() { 0.95 } else { 1.0 });
            cr.set_line_width(1.0);
            let _ = cr.stroke();
            paint_panel(cr, r, panel);
        }
    }
}

fn paint_section(cr: &Context, r: Rect, title: &str, badge: Option<Authority>, source: Option<&str>, accent: Rgb) {
    let cy = r.y + r.h * 0.62;
    set(cr, accent);
    cr.set_line_width(3.0);
    cr.move_to(r.x, r.y + 5.0);
    cr.line_to(r.x, r.y + r.h - 5.0);
    let _ = cr.stroke();
    font(cr, 15.5, FontWeight::Bold);
    set(cr, theme::text());
    text_left(cr, title, r.x + 12.0, cy);
    let title_w = cr.text_extents(title).map(|e| e.width()).unwrap_or(0.0);
    let mut x = r.x + 12.0 + title_w + 10.0;
    if let Some(auth) = badge {
        x += badge_pill(cr, auth, x, r.y + r.h * 0.30) + 8.0;
    }
    if let Some(src) = source {
        font(cr, 11.0, FontWeight::Normal);
        set(cr, theme::muted());
        text_left(cr, src, x, cy);
    }
}

fn paint_panel(cr: &Context, r: Rect, panel: &Panel) {
    let inner = Rect { x: r.x + PAD, y: r.y + PAD, w: r.w - PAD * 2.0, h: r.h - PAD * 2.0 };
    match panel {
        Panel::Section { .. } => {}
        Panel::LimitsProvider { name, subtitle, badge, gauges, accent, error } => {
            paint_limits_provider(cr, r, name, subtitle, *badge, gauges, *accent, error.as_deref())
        }
        Panel::Kpi { value, label, sub, accent } => paint_kpi(cr, inner, value, label, sub.as_deref(), *accent),
        Panel::HeroRing { gauge, accent, authority } => paint_hero_ring(cr, inner, gauge, *accent, *authority),
        Panel::ResetHorizon { title, ticks } => paint_reset_horizon(cr, inner, title, ticks),
        Panel::Rings { title, gauges, accent } => paint_rings(cr, inner, title, gauges, *accent),
        Panel::Callout { title, headline, body, accent } => paint_callout(cr, inner, title, headline, body, *accent),
        Panel::Area { title, series, accent, caption } => {
            let body = title_row(cr, inner, title, None, Some(caption));
            cr.save().ok();
            cr.translate(body.x, body.y);
            charts::area(cr, body.w, body.h, series, *accent);
            cr.restore().ok();
        }
        Panel::Bars { title, rows, caption } => {
            let body = title_row(cr, inner, title, None, Some(caption));
            cr.save().ok();
            cr.translate(body.x, body.y);
            charts::bars(cr, body.w, body.h, rows);
            cr.restore().ok();
        }
        Panel::Donut { title, slices, center_top, center_bottom, legend } => {
            paint_donut(cr, inner, title, slices, center_top, center_bottom, legend)
        }
        Panel::Heatmap { title, counts, max, accent, caption } => {
            let body = title_row(cr, inner, title, None, Some(caption));
            cr.save().ok();
            cr.translate(body.x, body.y);
            charts::heatmap(cr, body.w, body.h, counts, *max, *accent);
            cr.restore().ok();
        }
        Panel::Composition { title, segments, legend, caption } => {
            paint_composition(cr, inner, title, segments, legend, caption)
        }
        Panel::Stat { title, rows } => paint_stat(cr, inner, title, rows),
    }
}

/// Draw a panel title (and optional right-aligned caption) at the top of the
/// inner rect and return the remaining body rect.
fn title_row(cr: &Context, inner: Rect, title: &str, badge: Option<Authority>, caption: Option<&str>) -> Rect {
    font(cr, 13.0, FontWeight::Bold);
    set(cr, theme::text());
    text_left(cr, title, inner.x, inner.y + 12.0);
    let mut right = inner.x + inner.w;
    if let Some(cap) = caption {
        font(cr, 11.0, FontWeight::Normal);
        set(cr, theme::muted());
        let w = cr.text_extents(cap).map(|e| e.width()).unwrap_or(0.0);
        text_left(cr, cap, right - w, inner.y + 12.0);
        right -= w + 8.0;
    }
    if let Some(auth) = badge {
        badge_pill(cr, auth, right - 46.0, inner.y);
    }
    Rect { x: inner.x, y: inner.y + TITLE_H, w: inner.w, h: inner.h - TITLE_H }
}

fn paint_kpi(cr: &Context, inner: Rect, value: &str, label: &str, sub: Option<&str>, accent: Rgb) {
    font(cr, 11.0, FontWeight::Bold);
    set(cr, theme::muted());
    text_left(cr, &label.to_uppercase(), inner.x, inner.y + 11.0);
    let vsize = (inner.h * 0.44).clamp(20.0, 34.0);
    mono(cr, vsize, FontWeight::Bold);
    set(cr, accent);
    text_left(cr, value, inner.x, inner.y + inner.h * 0.62);
    if let Some(sub) = sub {
        font(cr, 11.0, FontWeight::Normal);
        set(cr, theme::muted());
        text_left(cr, sub, inner.x, inner.y + inner.h - 2.0);
    }
}

/// The hero: the single binding limit as an oversized ring, severity-coloured,
/// with the model/window, reset ETA, and an ACTIVE flag when the server marks it.
fn paint_hero_ring(cr: &Context, inner: Rect, gauge: &Gauge, accent: Rgb, authority: Authority) {
    font(cr, 11.0, FontWeight::Bold);
    set(cr, theme::muted());
    text_left(cr, "CLOSEST LIMIT", inner.x, inner.y + 11.0);
    badge_pill(cr, authority, inner.x + inner.w - 46.0, inner.y);

    let color = theme::gauge_color(accent, gauge.severity());
    let diameter = (inner.h - 26.0).clamp(60.0, 128.0);
    let ring_x = inner.x;
    let ring_y = inner.y + 20.0;
    cr.save().ok();
    cr.translate(ring_x, ring_y);
    gauge::draw_ring(cr, diameter as i32, diameter as i32, gauge.fraction, color, &gauge.percent_text(), "");
    cr.restore().ok();

    let tx = ring_x + diameter + 18.0;
    let tw = inner.x + inner.w - tx;
    font(cr, 15.0, FontWeight::Bold);
    set(cr, theme::text());
    let mut ty = ring_y + diameter * 0.34;
    for line in wrap(cr, &gauge.label, tw, 2) {
        text_left(cr, &line, tx, ty);
        ty += 19.0;
    }
    if let Some(reset) = gauge.resets_at {
        let secs = (reset - chrono::Utc::now()).num_seconds();
        font(cr, 12.5, FontWeight::Normal);
        set(cr, color);
        text_left(cr, &format!("{}resets in {}", if gauge.trusted_reset { "" } else { "~" }, format::until(secs)), tx, ty + 4.0);
        ty += 22.0;
    }
    if gauge.is_active {
        font(cr, 9.5, FontWeight::Bold);
        let pill = "BINDING";
        let pw = cr.text_extents(pill).map(|e| e.width()).unwrap_or(0.0) + 14.0;
        rounded(cr, tx, ty - 4.0, pw, 17.0, 8.5);
        set(cr, theme::MAGENTA);
        let _ = cr.fill();
        set(cr, theme::on_badge());
        text_left(cr, pill, tx + 7.0, ty + 8.0);
    }
}

/// A single soonest-first timeline collapsing every reset across both providers
/// onto one axis — filled ticks are trusted resets, hollow ticks estimated.
fn paint_reset_horizon(cr: &Context, inner: Rect, title: &str, ticks: &[ResetTick]) {
    let body = title_row(cr, inner, title, None, Some("next 7 days"));
    let axis_y = body.y + body.h * 0.5;
    let span = 7.0 * 86_400.0;
    set_alpha(cr, theme::track(), 1.0);
    cr.set_line_width(2.0);
    cr.move_to(body.x, axis_y);
    cr.line_to(body.x + body.w, axis_y);
    let _ = cr.stroke();
    // day gridlines
    font(cr, 9.5, FontWeight::Normal);
    for d in 0..=7 {
        let x = body.x + (d as f64 / 7.0) * body.w;
        set_alpha(cr, theme::track(), 0.6);
        cr.set_line_width(1.0);
        cr.move_to(x, axis_y - 4.0);
        cr.line_to(x, axis_y + 4.0);
        let _ = cr.stroke();
        set(cr, theme::muted());
        centered(cr, &format!("{d}d"), x, body.y + body.h - 1.0);
    }
    let mut last_x = f64::NEG_INFINITY;
    let mut below = false;
    for t in ticks {
        let frac = (t.seconds as f64 / span).clamp(0.0, 1.0);
        let x = body.x + frac * body.w;
        set(cr, t.color);
        cr.new_sub_path(); // arc() appends to the current path — start fresh so no line links the ticks
        cr.arc(x, axis_y, 5.0, 0.0, std::f64::consts::PI * 2.0);
        if t.trusted {
            let _ = cr.fill();
        } else {
            cr.set_line_width(2.0);
            let _ = cr.stroke();
        }
        below = if (x - last_x).abs() < body.w * 0.16 { !below } else { false };
        last_x = x;
        let ly = if below { axis_y + 18.0 } else { axis_y - 12.0 };
        font(cr, 10.0, FontWeight::Bold);
        set(cr, t.color);
        let label = charts_ellipsize(cr, &format!("{} · {}", short_reset_label(&t.label), format::until(t.seconds)), body.w * 0.32);
        let tw = cr.text_extents(&label).map(|e| e.width()).unwrap_or(0.0);
        let lx = (x - tw / 2.0).clamp(body.x, (body.x + body.w - tw).max(body.x));
        text_left(cr, &label, lx, ly);
    }
}

/// Compact provider card for the limits window: accent rail, title + badge,
/// then a tight row of rings. Designed so three providers fit a short window.
fn paint_limits_provider(
    cr: &Context,
    r: Rect,
    name: &str,
    subtitle: &str,
    badge: Authority,
    gauges: &[Gauge],
    accent: Rgb,
    error: Option<&str>,
) {
    // Left accent rail — slightly soft so it reads as trim, not a neon bar.
    rounded(cr, r.x + 1.0, r.y + 10.0, 3.0, r.h - 20.0, 1.5);
    set_alpha(cr, accent, if theme::is_dark() { 0.92 } else { 0.88 });
    let _ = cr.fill();

    let x0 = r.x + 16.0;
    let y0 = r.y + 12.0;
    let w0 = r.w - 28.0;

    // Header: name · badge · subtitle
    font(cr, 13.5, FontWeight::Bold);
    set(cr, theme::text());
    text_left(cr, name, x0, y0 + 12.0);
    let name_w = cr.text_extents(name).map(|e| e.width()).unwrap_or(0.0);
    let mut hx = x0 + name_w + 8.0;
    hx += badge_pill(cr, badge, hx, y0 + 1.0) + 8.0;
    if !subtitle.is_empty() {
        font(cr, 11.0, FontWeight::Normal);
        set(cr, theme::muted());
        let max = (r.x + r.w - 14.0 - hx).max(40.0);
        text_left(cr, &charts_ellipsize(cr, subtitle, max), hx, y0 + 12.0);
    }

    let body = Rect {
        x: x0,
        y: y0 + 22.0,
        w: w0,
        h: r.h - 40.0,
    };
    if let Some(err) = error {
        font(cr, 12.0, FontWeight::Normal);
        set(cr, theme::MAGENTA);
        for (i, line) in wrap(cr, err, body.w, 3).iter().enumerate() {
            text_left(cr, line, body.x, body.y + 18.0 + i as f64 * 16.0);
        }
        return;
    }
    if gauges.is_empty() {
        empty_note(cr, body, "no windows");
        return;
    }

    let n = gauges.len().min(5);
    let slot = body.w / n as f64;
    let diameter = (slot * 0.62).min(body.h - 36.0).clamp(44.0, 72.0);
    let ring_y = body.y + ((body.h - (diameter + 32.0)) / 2.0).max(2.0);
    for (i, g) in gauges.iter().take(n).enumerate() {
        let cx = body.x + slot * (i as f64 + 0.5);
        let color = theme::gauge_color(accent, g.severity());
        cr.save().ok();
        cr.translate(cx - diameter / 2.0, ring_y);
        gauge::draw_ring(cr, diameter as i32, diameter as i32, g.fraction, color, &g.percent_text(), "");
        cr.restore().ok();
        if g.is_active {
            // Small accent dot instead of a noisy ACTIVE pill.
            set(cr, theme::MAGENTA);
            cr.new_sub_path();
            cr.arc(cx + diameter * 0.32, ring_y + 4.0, 3.2, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
        }
        font(cr, 10.5, FontWeight::Normal);
        set(cr, theme::secondary());
        centered(cr, &charts_ellipsize(cr, &g.label, slot - 8.0), cx, ring_y + diameter + 12.0);
        if let Some(sub) = ring_subline(g) {
            font(cr, 9.5, FontWeight::Normal);
            set(cr, theme::muted());
            centered(cr, &charts_ellipsize(cr, &sub, slot - 8.0), cx, ring_y + diameter + 25.0);
        }
    }
}

fn paint_rings(cr: &Context, inner: Rect, title: &str, gauges: &[Gauge], accent: Rgb) {
    let body = if title.is_empty() { inner } else { title_row(cr, inner, title, None, None) };
    if gauges.is_empty() {
        empty_note(cr, body, "no live windows");
        return;
    }
    let n = gauges.len().min(5);
    let slot = body.w / n as f64;
    let diameter = (slot * 0.68).min(body.h - 36.0).max(44.0);
    let ring_y = body.y + ((body.h - (diameter + 40.0)) / 2.0).max(0.0);
    for (i, g) in gauges.iter().take(n).enumerate() {
        let cx = body.x + slot * (i as f64 + 0.5);
        let ring_x = cx - diameter / 2.0;
        let color = theme::gauge_color(accent, g.severity());
        cr.save().ok();
        cr.translate(ring_x, ring_y);
        gauge::draw_ring(cr, diameter as i32, diameter as i32, g.fraction, color, &g.percent_text(), "");
        cr.restore().ok();
        if g.is_active {
            set(cr, theme::MAGENTA);
            cr.new_sub_path();
            cr.arc(cx + diameter * 0.30, ring_y + 3.0, 3.4, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
        }
        font(cr, 11.0, FontWeight::Normal);
        set(cr, theme::secondary());
        centered(cr, &charts_ellipsize(cr, &g.label, slot - 6.0), cx, ring_y + diameter + 13.0);
        if let Some(sub) = ring_subline(g) {
            font(cr, 9.5, FontWeight::Normal);
            set(cr, theme::muted());
            centered(cr, &charts_ellipsize(cr, &sub, slot - 6.0), cx, ring_y + diameter + 27.0);
        }
    }
}

fn paint_callout(cr: &Context, inner: Rect, title: &str, headline: &str, body: &str, accent: Rgb) {
    font(cr, 11.0, FontWeight::Bold);
    set(cr, theme::muted());
    text_left(cr, &title.to_uppercase(), inner.x, inner.y + 11.0);
    mono(cr, (inner.h * 0.3).clamp(18.0, 26.0), FontWeight::Bold);
    set(cr, accent);
    text_left(cr, headline, inner.x, inner.y + inner.h * 0.55);
    font(cr, 12.0, FontWeight::Normal);
    set(cr, theme::secondary());
    for (i, line) in wrap(cr, body, inner.w, 2).iter().enumerate() {
        text_left(cr, line, inner.x, inner.y + inner.h * 0.72 + i as f64 * 16.0);
    }
}

fn paint_donut(cr: &Context, inner: Rect, title: &str, slices: &[Slice], top: &str, bottom: &str, legend: &[(String, Rgb, String)]) {
    let body = title_row(cr, inner, title, None, None);
    let donut_w = body.w * 0.52;
    cr.save().ok();
    cr.translate(body.x, body.y);
    charts::donut(cr, donut_w, body.h, slices, top, bottom);
    cr.restore().ok();
    paint_legend(cr, Rect { x: body.x + donut_w + 6.0, y: body.y, w: body.w - donut_w - 6.0, h: body.h }, legend);
}

fn paint_composition(cr: &Context, inner: Rect, title: &str, segments: &[Slice], legend: &[(String, Rgb, String)], caption: &str) {
    let body = title_row(cr, inner, title, None, Some(caption));
    let bar_h = 18.0;
    charts::stacked_bar(cr, body.x, body.y, body.w, bar_h, segments);
    paint_legend(cr, Rect { x: body.x, y: body.y + bar_h + 10.0, w: body.w, h: body.h - bar_h - 10.0 }, legend);
}

fn paint_legend(cr: &Context, r: Rect, legend: &[(String, Rgb, String)]) {
    let line_h = (r.h / legend.len().max(1) as f64).clamp(16.0, 26.0);
    for (i, (label, color, value)) in legend.iter().enumerate() {
        let y = r.y + line_h * (i as f64 + 0.5);
        rounded(cr, r.x, y - 4.0, 9.0, 9.0, 2.0);
        set(cr, *color);
        let _ = cr.fill();
        // Value is right-aligned; measure it first so the label never runs into it.
        mono(cr, 11.0, FontWeight::Bold);
        let vw = cr.text_extents(value).map(|e| e.width()).unwrap_or(0.0);
        set(cr, theme::text());
        text_left(cr, value, r.x + r.w - vw, y + 4.0);
        font(cr, 11.5, FontWeight::Normal);
        set(cr, theme::secondary());
        let label_max = (r.w - vw - 24.0).max(24.0);
        text_left(cr, &charts_ellipsize(cr, label, label_max), r.x + 15.0, y + 4.0);
    }
}

fn paint_stat(cr: &Context, inner: Rect, title: &str, rows: &[(String, String)]) {
    let body = title_row(cr, inner, title, None, None);
    if rows.is_empty() {
        empty_note(cr, body, "no data");
        return;
    }
    let line_h = (body.h / rows.len() as f64).clamp(16.0, 30.0);
    for (i, (k, v)) in rows.iter().enumerate() {
        let y = body.y + line_h * (i as f64 + 0.5) + 4.0;
        font(cr, 12.0, FontWeight::Normal);
        set(cr, theme::muted());
        text_left(cr, k, body.x, y);
        mono(cr, 12.5, FontWeight::Bold);
        set(cr, theme::text());
        let vw = cr.text_extents(v).map(|e| e.width()).unwrap_or(0.0);
        text_left(cr, v, body.x + body.w - vw, y);
    }
}

fn empty_note(cr: &Context, r: Rect, msg: &str) {
    font(cr, 12.0, FontWeight::Normal);
    set(cr, theme::muted());
    centered(cr, msg, r.x + r.w / 2.0, r.y + r.h / 2.0);
}

// ---- card construction from the dashboard model ----------------------------

fn build_cards(dash: &Dashboard, scope: Scope) -> Vec<Card> {
    let mut cards = Vec::new();
    match scope {
        // Compact provider strips — dense enough that three fit a short window.
        Scope::Limits => {
            limits_provider_card(
                &mut cards,
                &dash.claude_quota,
                theme::provider_accent(theme::ProviderAccent::Claude),
            );
            limits_provider_card(
                &mut cards,
                &dash.grok_quota,
                theme::provider_accent(theme::ProviderAccent::Grok),
            );
            limits_provider_card(
                &mut cards,
                &dash.opencode_quota,
                theme::provider_accent(theme::ProviderAccent::OpenCode),
            );
        }
        Scope::Full => {
            claude_quota_cards(&mut cards, &dash.claude_quota);
            let roi = plan_monthly_usd(&dash.claude_quota.subtitle);
            usage_cards(&mut cards, &dash.claude_usage, "Claude usage", UsageKind::Claude, roi);
            grok_quota_cards(&mut cards, &dash.grok_quota);
            usage_cards(&mut cards, &dash.grok_usage, "Grok usage", UsageKind::Grok, None);
            opencode_quota_cards(&mut cards, &dash.opencode_quota);
            usage_cards(&mut cards, &dash.opencode_usage, "opencode usage", UsageKind::OpenCode, None);
        }
    }
    cards
}

/// One compact card per provider for the limits window.
fn limits_provider_card(cards: &mut Vec<Card>, snap: &Snapshot, accent: Rgb) {
    let name = match snap.provider_id.as_str() {
        "anthropic" => "Claude",
        "xai" => "Grok",
        "opencode-go" => "opencode",
        other => other,
    };
    cards.push(Card {
        id: format!("limits-{}", snap.provider_id),
        grow: 1.0,
        min_w: 100_000.0,
        height: 138.0,
        panel: Panel::LimitsProvider {
            name: name.into(),
            subtitle: snap.subtitle.clone(),
            badge: snap.authority,
            gauges: snap.gauges.clone(),
            accent,
            error: snap.error.clone(),
        },
    });
}

/// Best-effort monthly list price for the plan named in the quota subtitle, so
/// the value-returned tile can show a rough return multiple. Deliberately coarse
/// and only ever shown with a `~`.
fn plan_monthly_usd(subtitle: &str) -> Option<f64> {
    let s = subtitle.to_ascii_lowercase();
    if s.contains("max") {
        if s.contains("20") {
            Some(200.0)
        } else if s.contains('5') {
            Some(100.0)
        } else {
            Some(100.0)
        }
    } else if s.contains("pro") {
        Some(20.0)
    } else {
        None
    }
}

/// Collect upcoming resets across the quota snapshot into soonest-first ticks.
fn reset_ticks(snap: &Snapshot, accent: Rgb) -> Vec<ResetTick> {
    let now = chrono::Utc::now();
    let mut ticks: Vec<ResetTick> = snap
        .gauges
        .iter()
        .filter_map(|g| {
            let reset = g.resets_at?;
            let seconds = (reset - now).num_seconds();
            if !(0..=7 * 86_400).contains(&seconds) {
                return None;
            }
            Some(ResetTick {
                label: g.label.clone(),
                seconds,
                trusted: g.trusted_reset,
                color: theme::gauge_color(accent, g.severity()),
            })
        })
        .collect();
    ticks.sort_by_key(|t| t.seconds);
    ticks
}

fn section_accent(id: &str, title: &str, badge: Option<Authority>, source: Option<&str>, accent: Rgb) -> Card {
    Card {
        id: id.into(),
        grow: 1.0,
        min_w: 100_000.0,
        height: 30.0,
        panel: Panel::Section {
            title: title.into(),
            badge,
            source: source.map(str::to_string),
            accent,
        },
    }
}

fn claude_quota_cards(cards: &mut Vec<Card>, snap: &Snapshot) {
    let accent = theme::provider_accent(theme::ProviderAccent::Claude);
    cards.push(section_accent("sec-claude-quota", "Claude — live quota", Some(snap.authority), Some(&snap.source), accent));
    if let Some(err) = &snap.error {
        cards.push(callout_card("claude-quota-err", "Claude quota unavailable", "OFFLINE", err, theme::MAGENTA, 3.0, 340.0, 96.0));
        return;
    }
    if let Some(binding) = snap.binding_gauge() {
        cards.push(Card {
            id: "claude-binding".into(),
            grow: 1.5,
            min_w: 320.0,
            height: 150.0,
            panel: Panel::HeroRing { gauge: binding.clone(), accent, authority: snap.authority },
        });
    }
    cards.push(Card {
        id: "claude-rings".into(),
        grow: 2.4,
        min_w: 400.0,
        height: 150.0,
        panel: Panel::Rings { title: "Rate-limit windows".into(), gauges: snap.gauges.clone(), accent },
    });
    if let Some(spend) = &snap.spend {
        let (value, sub) = if spend.enabled {
            (format::usd_cents(spend.used), spend.limit.map(|l| format!("of {} cap", format::usd_cents(l))).unwrap_or_else(|| "used".into()))
        } else {
            ("off".to_string(), "extra-usage credits".to_string())
        };
        cards.push(kpi_card("claude-credits", &value, "Overflow credits", Some(&sub), theme::LIME, 0.9, 150.0, 150.0));
    }
    let ticks = reset_ticks(snap, accent);
    if !ticks.is_empty() {
        cards.push(Card {
            id: "claude-reset-horizon".into(),
            grow: 3.0,
            min_w: 360.0,
            height: 108.0,
            panel: Panel::ResetHorizon { title: "Reset horizon — next unlocks".into(), ticks },
        });
    }
}

fn grok_quota_cards(cards: &mut Vec<Card>, snap: &Snapshot) {
    let accent = theme::provider_accent(theme::ProviderAccent::Grok);
    cards.push(section_accent("sec-grok-quota", "Grok — live credits", Some(snap.authority), Some(&snap.source), accent));
    if let Some(err) = &snap.error {
        cards.push(callout_card("grok-quota-err", "Grok quota unavailable", "OFFLINE", err, theme::MAGENTA, 3.0, 340.0, 96.0));
        return;
    }
    if let Some(binding) = snap.binding_gauge() {
        cards.push(Card {
            id: "grok-binding".into(),
            grow: 1.5,
            min_w: 320.0,
            height: 150.0,
            panel: Panel::HeroRing { gauge: binding.clone(), accent, authority: snap.authority },
        });
    }
    cards.push(Card {
        id: "grok-rings".into(),
        grow: 2.4,
        min_w: 400.0,
        height: 150.0,
        panel: Panel::Rings { title: "Credit windows".into(), gauges: snap.gauges.clone(), accent },
    });
    if let Some(spend) = &snap.spend {
        let value = spend
            .balance
            .map(format::usd_cents)
            .unwrap_or_else(|| "—".into());
        let sub = if spend.enabled {
            "prepaid remaining".to_string()
        } else {
            "no prepaid balance".to_string()
        };
        cards.push(kpi_card("grok-prepaid", &value, "Prepaid balance", Some(&sub), theme::TEAL, 0.9, 150.0, 150.0));
    }
    let ticks = reset_ticks(snap, accent);
    if !ticks.is_empty() {
        cards.push(Card {
            id: "grok-reset-horizon".into(),
            grow: 3.0,
            min_w: 360.0,
            height: 108.0,
            panel: Panel::ResetHorizon { title: "Reset horizon — next unlocks".into(), ticks },
        });
    }
}

fn opencode_quota_cards(cards: &mut Vec<Card>, snap: &Snapshot) {
    let accent = theme::provider_accent(theme::ProviderAccent::OpenCode);
    cards.push(section_accent("sec-oc-quota", "opencode — rolling caps", Some(snap.authority), Some(&snap.source), accent));
    if let Some(err) = &snap.error {
        cards.push(callout_card("oc-quota-err", "opencode caps unavailable", "OFFLINE", err, theme::MAGENTA, 3.0, 340.0, 96.0));
        return;
    }
    cards.push(Card {
        id: "oc-rings".into(),
        grow: 3.0,
        min_w: 380.0,
        height: 150.0,
        panel: Panel::Rings { title: "Estimated spend vs Go caps".into(), gauges: snap.gauges.clone(), accent },
    });
    if let Some(note) = &snap.note {
        cards.push(Card {
            id: "oc-note".into(),
            grow: 1.6,
            min_w: 260.0,
            height: 150.0,
            panel: Panel::Callout { title: "Estimate — read this".into(), headline: "EST only".into(), body: note.clone(), accent: theme::TEAL },
        });
    }
}

#[derive(Clone, Copy)]
enum UsageKind {
    Claude,
    Grok,
    OpenCode,
}

impl UsageKind {
    fn accent(self) -> Rgb {
        match self {
            UsageKind::Claude => theme::provider_accent(theme::ProviderAccent::Claude),
            UsageKind::Grok => theme::provider_accent(theme::ProviderAccent::Grok),
            UsageKind::OpenCode => theme::provider_accent(theme::ProviderAccent::OpenCode),
        }
    }

    fn is_claude(self) -> bool {
        matches!(self, UsageKind::Claude)
    }

    fn activity_only(self) -> bool {
        matches!(self, UsageKind::Grok)
    }
}

fn usage_cards(cards: &mut Vec<Card>, usage: &Usage, section_title: &str, kind: UsageKind, roi_base: Option<f64>) {
    let source = usage.source.clone();
    cards.push(section_accent(
        &format!("sec-{}", slug(section_title)),
        section_title,
        Some(usage.authority),
        Some(&source),
        kind.accent(),
    ));
    if usage.is_empty() {
        let msg = usage.error.clone().unwrap_or_else(|| "no local usage yet".into());
        cards.push(callout_card(&format!("{}-empty", slug(section_title)), "No usage history", "", &msg, theme::muted(), 3.0, 340.0, 90.0));
        return;
    }
    let accent = kind.accent();
    let is_claude = kind.is_claude();
    let activity_only = kind.activity_only();
    let p = slug(section_title);
    let t = &usage.totals;

    // Value-returned hero: API-equivalent value this month vs the plan's list price.
    if let Some(plan) = roi_base {
        let sub = if plan > 0.0 {
            format!("≈ {:.1}× your ~{}/mo plan", usage.windows.thirty.cost / plan, format::usd(plan))
        } else {
            "API-equivalent value".into()
        };
        cards.push(Card {
            id: format!("{p}-value-hero"),
            grow: 1.4,
            min_w: 240.0,
            height: 92.0,
            panel: Panel::Kpi { value: format::usd(usage.windows.thirty.cost), label: "Value returned · 30d".into(), sub: Some(sub), accent: theme::LIME },
        });
    }

    if activity_only {
        cards.push(kpi_card(&format!("{p}-kpi-30d"), &format::count(usage.windows.thirty.messages), "Turns 30d", Some(&format!("{} today", format::count(usage.windows.today.messages))), accent, 1.0, 150.0, 92.0));
        cards.push(kpi_card(&format!("{p}-kpi-alltime"), &format::count(t.messages), "Turns all-time", Some(&format!("over {} days", t.active_days)), theme::LIME, 1.0, 150.0, 92.0));
        cards.push(kpi_card(&format!("{p}-kpi-sessions"), &t.sessions.to_string(), "Sessions", Some(&format!("{} active days", t.active_days)), theme::TEAL, 1.0, 130.0, 92.0));
        cards.push(kpi_card(&format!("{p}-kpi-models"), &usage.by_model.len().to_string(), "Models used", Some(usage.by_model.first().map(|s| s.label.as_str()).unwrap_or("—")), theme::AZURE, 1.0, 130.0, 92.0));
    } else {
        // KPI strip — skip the 30-day tile for Claude, whose value-returned hero already leads with it.
        if roi_base.is_none() {
            cards.push(kpi_card(&format!("{p}-kpi-30d"), &format::usd(usage.windows.thirty.cost), "Spend 30d", Some(&format!("{} today", format::usd(usage.windows.today.cost))), accent, 1.0, 150.0, 92.0));
        }
        cards.push(kpi_card(&format!("{p}-kpi-alltime"), &format::usd(t.cost_usd), if is_claude { "Value all-time" } else { "Spend all-time" }, Some(&format!("over {} days", t.active_days)), theme::LIME, 1.0, 150.0, 92.0));
        cards.push(kpi_card(&format!("{p}-kpi-tokens"), &format::count(t.total_tokens()), "Tokens all-time", Some(&format!("{} msgs", format::count(t.messages))), theme::VIOLET, 1.0, 150.0, 92.0));
        cards.push(kpi_card(&format!("{p}-kpi-sessions"), &t.sessions.to_string(), "Sessions", Some(&format!("{} active days", t.active_days)), theme::TEAL, 1.0, 130.0, 92.0));
        cards.push(kpi_card(&format!("{p}-kpi-cache"), &format::percent(usage.cache_hit_rate()), "Cache hit rate", Some(&format!("{} cached", format::count(t.cache_read))), theme::AZURE, 1.0, 130.0, 92.0));
        if is_claude {
            cards.push(kpi_card(&format!("{p}-kpi-tools"), &format::count(t.web_search + t.web_fetch), "Web tool calls", Some(&format!("{} search · {} fetch", t.web_search, t.web_fetch)), theme::ORANGE, 1.0, 130.0, 92.0));
        } else {
            cards.push(kpi_card(&format!("{p}-kpi-reason"), &format::count(usage.tokens.reasoning), "Reasoning tokens", Some("across providers"), theme::ORANGE, 1.0, 130.0, 92.0));
        }
    }

    if activity_only {
        let avg = if t.active_days == 0 { 0.0 } else { t.messages as f64 / t.active_days as f64 };
        cards.push(Card {
            id: format!("{p}-burn"),
            grow: 1.2,
            min_w: 240.0,
            height: 132.0,
            panel: Panel::Callout {
                title: "Activity — turns/day".into(),
                headline: format!("{avg:.1}"),
                body: format!("≈ {:.0}/mo at this pace · today {}", avg * 30.0, format::count(usage.windows.today.messages)),
                accent,
            },
        });
        let msg_series: Vec<f64> = tail(&usage.daily, 45).iter().map(|d| d.messages as f64).collect();
        cards.push(Card {
            id: format!("{p}-daily-msgs"),
            grow: 2.0,
            min_w: 320.0,
            height: 132.0,
            panel: Panel::Area {
                title: "Daily turns (45d)".into(),
                series: msg_series,
                accent,
                caption: format!("peak {}", format::count(usage.daily.iter().map(|d| d.messages).max().unwrap_or(0))),
            },
        });
    } else {
        // burn-rate projection callout
        let avg = usage.avg_daily_cost();
        let proj = avg * 30.0;
        cards.push(Card {
            id: format!("{p}-burn"),
            grow: 1.2,
            min_w: 240.0,
            height: 132.0,
            panel: Panel::Callout {
                title: if is_claude { "Burn rate — value/day".into() } else { "Burn rate — spend/day".into() },
                headline: format::usd(avg),
                body: format!("≈ {}/mo at this pace · today {}", format::usd(proj), format::usd(usage.windows.today.cost)),
                accent,
            },
        });

        // daily area charts
        let cost_series: Vec<f64> = tail(&usage.daily, 45).iter().map(|d| d.cost).collect();
        cards.push(Card {
            id: format!("{p}-daily-cost"),
            grow: 2.0,
            min_w: 320.0,
            height: 132.0,
            panel: Panel::Area { title: if is_claude { "Daily value (45d)".into() } else { "Daily spend (45d)".into() }, series: cost_series, accent, caption: format!("peak {}", format::usd(usage.daily.iter().map(|d| d.cost).fold(0.0, f64::max))) },
        });
        let token_series: Vec<f64> = tail(&usage.daily, 45).iter().map(|d| d.tokens as f64).collect();
        cards.push(Card {
            id: format!("{p}-daily-tokens"),
            grow: 2.0,
            min_w: 300.0,
            height: 132.0,
            panel: Panel::Area { title: "Daily tokens (45d)".into(), series: token_series, accent: theme::VIOLET, caption: format!("peak {}", format::count(usage.daily.iter().map(|d| d.tokens).max().unwrap_or(0))) },
        });
    }

    // breakdowns
    cards.push(bars_card(&format!("{p}-by-model"), "By model", &usage.by_model));
    if !usage.by_provider.is_empty() {
        cards.push(bars_card(&format!("{p}-by-provider"), "By provider", &usage.by_provider));
        cards.push(free_paid_card(&format!("{p}-freepaid"), usage));
    }
    if !usage.by_project.is_empty() {
        cards.push(bars_card(&format!("{p}-by-project"), "By project", &usage.by_project));
    }

    if !activity_only {
        cards.push(composition_card(&format!("{p}-tokens"), usage));
    }

    // heatmap
    cards.push(Card {
        id: format!("{p}-heatmap"),
        grow: 2.4,
        min_w: 360.0,
        height: 176.0,
        panel: Panel::Heatmap {
            title: "Activity — when you work".into(),
            counts: Box::new(usage.heatmap.counts),
            max: usage.heatmap.max,
            accent,
            caption: "msgs / hour".into(),
        },
    });

    // detail stat table
    let mut rows = vec![
        ("First activity".into(), usage.totals.first_day.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".into())),
        ("Latest activity".into(), usage.totals.last_day.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".into())),
    ];
    if activity_only {
        rows.push(("Turns".into(), format::count(t.messages)));
        rows.push(("Sessions".into(), t.sessions.to_string()));
        rows.push(("Projects".into(), usage.by_project.len().to_string()));
        rows.push(("Models".into(), usage.by_model.len().to_string()));
    } else {
        rows.push(("Input tokens".into(), format::count(usage.tokens.input)));
        rows.push(("Output tokens".into(), format::count(usage.tokens.output)));
        rows.push(("Cache write".into(), format::count(usage.tokens.cache_write)));
        rows.push(("Cache read".into(), format::count(usage.tokens.cache_read)));
        if usage.tokens.reasoning > 0 {
            rows.push(("Reasoning".into(), format::count(usage.tokens.reasoning)));
        }
    }
    cards.push(Card {
        id: format!("{p}-detail"),
        grow: 1.4,
        min_w: 220.0,
        height: 176.0,
        panel: Panel::Stat { title: "Detail".into(), rows },
    });
}

fn kpi_card(id: &str, value: &str, label: &str, sub: Option<&str>, accent: Rgb, grow: f64, min_w: f64, height: f64) -> Card {
    Card { id: id.into(), grow, min_w, height, panel: Panel::Kpi { value: value.into(), label: label.into(), sub: sub.map(str::to_string), accent } }
}

fn callout_card(id: &str, title: &str, headline: &str, body: &str, accent: Rgb, grow: f64, min_w: f64, height: f64) -> Card {
    Card { id: id.into(), grow, min_w, height, panel: Panel::Callout { title: title.into(), headline: headline.into(), body: body.into(), accent } }
}

/// A ranked bar breakdown. Bars are scaled by **tokens** (a metric every row
/// has, priced or free) so lengths are comparable; the caption shows dollars
/// where the row is priced, otherwise the token count.
fn bars_card(id: &str, title: &str, segments: &[Segment]) -> Card {
    // Prefer tokens when present; fall back to message counts for activity-only
    // providers (Grok sessions don't store per-turn token usage on disk).
    let by_tokens = segments.iter().any(|s| s.tokens > 0);
    let mut ordered: Vec<&Segment> = segments.iter().collect();
    if by_tokens {
        ordered.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    } else {
        ordered.sort_by(|a, b| b.messages.cmp(&a.messages));
    }
    let rows: Vec<BarRow> = ordered
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, s)| {
            let value = if by_tokens { s.tokens } else { s.messages };
            BarRow {
                label: s.label.clone(),
                value: value as f64,
                caption: if s.cost > 0.0 {
                    format::usd(s.cost)
                } else {
                    format::count(value)
                },
                color: theme::series_color(i),
            }
        })
        .collect();
    Card { id: id.into(), grow: 1.6, min_w: 250.0, height: 176.0, panel: Panel::Bars { title: title.into(), rows, caption: format!("top {}", segments.len().min(6)) } }
}

/// A donut splitting token volume between paid (priced) and free/local
/// providers — the "how much am I arbitraging onto free models" view.
fn free_paid_card(id: &str, usage: &Usage) -> Card {
    let paid: u64 = usage.by_provider.iter().filter(|s| s.cost > 0.0).map(|s| s.tokens).sum();
    let free: u64 = usage.by_provider.iter().filter(|s| s.cost <= 0.0).map(|s| s.tokens).sum();
    Card {
        id: id.into(),
        grow: 1.4,
        min_w: 230.0,
        height: 176.0,
        panel: Panel::Donut {
            title: "Free vs paid".into(),
            slices: vec![
                Slice { value: paid as f64, color: theme::LIME },
                Slice { value: free as f64, color: theme::AZURE },
            ],
            center_top: format::count(paid + free),
            center_bottom: "tokens".into(),
            legend: vec![
                ("Paid (Go)".into(), theme::LIME, format::usd(usage.totals.cost_usd)),
                ("Free / local".into(), theme::AZURE, format::count(free)),
            ],
        },
    }
}

fn composition_card(id: &str, usage: &Usage) -> Card {
    let t = &usage.tokens;
    let mut segments = vec![
        Slice { value: t.input as f64, color: theme::TOKEN_INPUT },
        Slice { value: t.output as f64, color: theme::TOKEN_OUTPUT },
        Slice { value: t.cache_write as f64, color: theme::TOKEN_CACHE_WRITE },
        Slice { value: t.cache_read as f64, color: theme::TOKEN_CACHE_READ },
    ];
    let mut legend = vec![
        ("Input".into(), theme::TOKEN_INPUT, format::count(t.input)),
        ("Output".into(), theme::TOKEN_OUTPUT, format::count(t.output)),
        ("Cache write".into(), theme::TOKEN_CACHE_WRITE, format::count(t.cache_write)),
        ("Cache read".into(), theme::TOKEN_CACHE_READ, format::count(t.cache_read)),
    ];
    if t.reasoning > 0 {
        segments.push(Slice { value: t.reasoning as f64, color: theme::TOKEN_REASONING });
        legend.push(("Reasoning".into(), theme::TOKEN_REASONING, format::count(t.reasoning)));
    }
    Card {
        id: id.into(),
        grow: 1.8,
        min_w: 260.0,
        height: 176.0,
        panel: Panel::Composition { title: "Token composition".into(), segments, legend, caption: format::count(t.total()) },
    }
}

// ---- headless / share export ----------------------------------------------

/// WYSIWYG capture of what the live canvas paints — same plan width and UI
/// scale as the window, plus a very subtle credit line. `pixel_scale`
/// multiplies the on-screen size (2.0 ≈ retina of the real window).
pub fn export_live_view(
    dash: &Dashboard,
    plan_width: f64,
    ui_scale: f64,
    scope: Scope,
    path: &std::path::Path,
    pixel_scale: f64,
) -> Result<(), String> {
    use gtk::cairo::{Format, ImageSurface};

    let plan = plan(dash, plan_width, scope);
    let credit_h = 22.0;
    let total_h = plan.height + credit_h;
    let out_scale = (ui_scale * pixel_scale).max(1.0);
    let width_px = (plan_width * out_scale).ceil() as i32;
    let height_px = (total_h * out_scale).ceil() as i32;
    let surface = ImageSurface::create(Format::ARgb32, width_px.max(1), height_px.max(1))
        .map_err(|e| e.to_string())?;
    {
        let cr = Context::new(&surface).map_err(|e| e.to_string())?;
        cr.scale(out_scale, out_scale);
        let empty = HashSet::new();
        let opts = PaintOpts {
            selecting: false,
            selected: &empty,
        };
        paint(&cr, &plan, plan_width, &opts);
        paint_credit_line(&cr, plan_width, plan.height, credit_h);
    }
    let mut file = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    surface.write_to_png(&mut file).map_err(|e| format!("write png: {e}"))
}

/// Tiny footer used only on screenshots — easy to miss, but identifiable.
fn paint_credit_line(cr: &Context, width: f64, y0: f64, height: f64) {
    set(cr, theme::bg());
    cr.rectangle(0.0, y0, width, height);
    let _ = cr.fill();

    let label = concat!(
        "tokenmaxxing ",
        env!("CARGO_PKG_VERSION"),
        "  ·  github.com/guitaripod/tokenmaxxing"
    );
    mono(cr, 9.5, FontWeight::Normal);
    // Very quiet — just enough to read if you look for it.
    set_alpha(cr, theme::muted(), if theme::is_dark() { 0.55 } else { 0.50 });
    let tw = cr.text_extents(label).map(|e| e.width()).unwrap_or(0.0);
    let x = ((width - tw) / 2.0).max(8.0);
    let y = y0 + height * 0.68;
    text_left(cr, label, x, y);
}

/// Render the dashboard (or a selected subset of panels) to a standalone PNG
/// with brand header and footer, at `scale`× for crisp output.
pub fn export(dash: &Dashboard, width: f64, scale: f64, selected: Option<&HashSet<String>>, scope: Scope, path: &std::path::Path) -> Result<(), String> {
    use gtk::cairo::{Format, ImageSurface};

    let full = plan(dash, width, scope);
    let items: Vec<&Placed> = full
        .placed
        .iter()
        .filter(|p| match selected {
            None => true,
            Some(sel) => matches!(p.card.panel, Panel::Section { .. }) || sel.contains(&p.card.id),
        })
        .collect();
    // Drop section headers that have no visible panel under them.
    let items = prune_empty_sections(items);

    let compact = scope == Scope::Limits;
    let header_h = if compact { 58.0 } else { 88.0 };
    let footer_h = if compact { 34.0 } else { 48.0 };
    // Re-flow the kept cards top-to-bottom to remove the gaps left by filtered panels.
    let (reflowed, body_h) = reflow(&items, width, compact);
    let total_h = header_h + body_h + footer_h;

    let surface = ImageSurface::create(Format::ARgb32, (width * scale) as i32, (total_h * scale) as i32)
        .map_err(|e| e.to_string())?;
    {
        let cr = Context::new(&surface).map_err(|e| e.to_string())?;
        cr.scale(scale, scale);
        background(&cr, width, total_h);
        paint_export_header(&cr, width, header_h, dash, compact);
        cr.save().ok();
        cr.translate(0.0, header_h);
        for (rect, idx) in &reflowed {
            paint_card(&cr, *rect, &items[*idx].card);
        }
        cr.restore().ok();
        paint_export_footer(&cr, width, total_h, footer_h, compact);
    }
    let mut file = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    surface.write_to_png(&mut file).map_err(|e| format!("write png: {e}"))
}

fn prune_empty_sections(items: Vec<&Placed>) -> Vec<&Placed> {
    let mut out: Vec<&Placed> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        if matches!(item.card.panel, Panel::Section { .. }) {
            let has_child = items[i + 1..]
                .iter()
                .take_while(|p| !matches!(p.card.panel, Panel::Section { .. }))
                .next()
                .is_some();
            if !has_child {
                continue;
            }
        }
        out.push(item);
    }
    out
}

/// Repack the kept cards into a fresh top-to-bottom layout at `width`, so an
/// exported subset has no holes. Returns `(rect, index-into-items)` pairs.
fn reflow(items: &[&Placed], width: f64, compact: bool) -> (Vec<(Rect, usize)>, f64) {
    let gap = if compact { LIMITS_GAP } else { GAP };
    let margin = if compact { 14.0 } else { MARGIN };
    let usable = (width - margin * 2.0).max(200.0);
    let mut out: Vec<(Rect, usize)> = Vec::with_capacity(items.len());
    let mut y = margin;
    let mut idx = 0;
    while idx < items.len() {
        let card = &items[idx].card;
        if matches!(card.panel, Panel::Section { .. } | Panel::LimitsProvider { .. }) {
            out.push((Rect { x: margin, y, w: usable, h: card.height }, idx));
            y += card.height
                + if matches!(card.panel, Panel::Section { .. }) {
                    gap * 0.55
                } else {
                    gap
                };
            idx += 1;
            continue;
        }
        let mut line = vec![idx];
        let mut used = card.min_w;
        idx += 1;
        while idx < items.len()
            && !matches!(
                items[idx].card.panel,
                Panel::Section { .. } | Panel::LimitsProvider { .. }
            )
        {
            let next = &items[idx].card;
            if used + gap + next.min_w > usable {
                break;
            }
            used += gap + next.min_w;
            line.push(idx);
            idx += 1;
        }
        let total_grow: f64 = line.iter().map(|j| items[*j].card.grow).sum::<f64>().max(0.001);
        let avail = usable - gap * (line.len() as f64 - 1.0);
        let row_h = line.iter().map(|j| items[*j].card.height).fold(0.0, f64::max);
        let mut x = margin;
        for j in line {
            let w = avail * items[j].card.grow / total_grow;
            out.push((Rect { x, y, w, h: row_h }, j));
            x += w + gap;
        }
        y += row_h + gap;
    }
    (out, y - gap + margin)
}

fn paint_export_header(cr: &Context, width: f64, h: f64, dash: &Dashboard, compact: bool) {
    let logo = if compact { h * 0.48 } else { h * 0.52 };
    cr.save().ok();
    cr.translate(MARGIN, (h - logo) * 0.45);
    gauge::draw_logo(cr, logo);
    cr.restore().ok();
    let x = MARGIN + logo + 12.0;
    mono(cr, if compact { 20.0 } else { 28.0 }, FontWeight::Bold);
    set(cr, theme::CYAN);
    text_left(cr, "tokenmaxxing", x, h * 0.48);
    if !compact {
        font(cr, 12.5, FontWeight::Normal);
        set(cr, theme::muted());
        text_left(cr, "LLM usage dashboard", x, h * 0.72);
    }
    let stamp = dash.generated_at.format("%Y-%m-%d  %H:%M").to_string();
    font(cr, if compact { 11.0 } else { 12.5 }, FontWeight::Normal);
    set(cr, theme::muted());
    let sw = cr.text_extents(&stamp).map(|e| e.width()).unwrap_or(0.0);
    text_left(cr, &stamp, width - MARGIN - sw, h * 0.52);
    set_alpha(cr, theme::track(), 1.0);
    cr.set_line_width(1.0);
    cr.move_to(MARGIN, h - 4.0);
    cr.line_to(width - MARGIN, h - 4.0);
    let _ = cr.stroke();
}

fn paint_export_footer(cr: &Context, width: f64, total_h: f64, footer_h: f64, compact: bool) {
    let y = total_h - footer_h * 0.35;
    set_alpha(cr, theme::track(), 1.0);
    cr.set_line_width(1.0);
    cr.move_to(MARGIN, y - (if compact { 12.0 } else { 16.0 }));
    cr.line_to(width - MARGIN, y - (if compact { 12.0 } else { 16.0 }));
    let _ = cr.stroke();
    mono(cr, if compact { 10.5 } else { 11.5 }, FontWeight::Normal);
    set(cr, theme::muted());
    if compact {
        text_left(cr, "github.com/guitaripod/tokenmaxxing", MARGIN, y);
        let v = concat!("tokenmaxxing ", env!("CARGO_PKG_VERSION"));
        let vw = cr.text_extents(v).map(|e| e.width()).unwrap_or(0.0);
        text_left(cr, v, width - MARGIN - vw, y);
    } else {
        text_left(cr, "github.com/guitaripod/tokenmaxxing", MARGIN, y);
        let v = concat!(
            "tokenmaxxing ",
            env!("CARGO_PKG_VERSION"),
            " · $ figures are API-equivalent estimates"
        );
        let vw = cr.text_extents(v).map(|e| e.width()).unwrap_or(0.0);
        text_left(cr, v, width - MARGIN - vw, y);
    }
}

// ---- shared drawing helpers ------------------------------------------------

fn background(cr: &Context, w: f64, h: f64) {
    set(cr, theme::bg());
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();
}

fn badge_pill(cr: &Context, auth: Authority, x: f64, y: f64) -> f64 {
    let fill = theme::badge_fill(auth);
    let ink = theme::badge_ink(auth);
    let label = auth.badge();
    font(cr, 9.0, FontWeight::Bold);
    let tw = cr.text_extents(label).map(|e| e.width()).unwrap_or(0.0);
    let pw = tw + 12.0;
    let ph = 15.0;
    rounded(cr, x, y, pw, ph, ph / 2.0);
    set(cr, fill);
    let _ = cr.fill();
    set(cr, ink);
    text_left(cr, label, x + 6.0, y + 10.5);
    pw
}

fn ring_subline(g: &Gauge) -> Option<String> {
    if let (crate::model::Unit::Usd, Some(u), Some(l)) = (g.unit, g.used, g.limit) {
        return Some(format!("${u:.0}/${l:.0}"));
    }
    if let Some(reset) = g.resets_at {
        let secs = (reset - chrono::Utc::now()).num_seconds();
        return Some(format!("{}{}", if g.trusted_reset { "" } else { "~" }, format::until(secs)));
    }
    g.detail.clone()
}

fn wrap(cr: &Context, text: &str, max_w: f64, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if cr.text_extents(&candidate).map(|e| e.width()).unwrap_or(0.0) > max_w && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
            if lines.len() == max_lines {
                return lines;
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

fn charts_ellipsize(cr: &Context, text: &str, max_w: f64) -> String {
    if cr.text_extents(text).map(|e| e.width()).unwrap_or(0.0) <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while out.chars().count() > 1 && cr.text_extents(&format!("{out}…")).map(|e| e.width()).unwrap_or(0.0) > max_w {
        out.pop();
    }
    format!("{out}…")
}

fn font(cr: &Context, size: f64, weight: FontWeight) {
    cr.select_font_face("sans-serif", FontSlant::Normal, weight);
    cr.set_font_size(size);
}

fn mono(cr: &Context, size: f64, weight: FontWeight) {
    cr.select_font_face("monospace", FontSlant::Normal, weight);
    cr.set_font_size(size);
}

fn text_left(cr: &Context, s: &str, x: f64, baseline: f64) {
    cr.move_to(x, baseline);
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
    use std::f64::consts::PI;
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

/// Default export location: `$XDG_PICTURES_DIR` (or ~/Pictures), timestamped.
pub fn default_output() -> std::path::PathBuf {
    default_output_named("dashboard")
}

/// Timestamped export path with a short label, e.g. `tokenmaxxing-limits-…png`.
pub fn default_output_named(label: &str) -> std::path::PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let pictures = std::env::var_os("XDG_PICTURES_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::creds::home().join("Pictures"));
    let dir = if pictures.is_dir() {
        pictures
    } else {
        crate::creds::home()
    };
    dir.join(format!("tokenmaxxing-{label}-{stamp}.png"))
}

/// Trim a gauge label to the distinguishing tail for the cramped reset axis:
/// "Weekly · Fable" → "Fable", "5-hour session" → "5-hour session".
fn short_reset_label(label: &str) -> String {
    label.rsplit('·').next().unwrap_or(label).trim().to_string()
}

fn slug(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect()
}

fn tail<T>(v: &[T], n: usize) -> &[T] {
    if v.len() > n {
        &v[v.len() - n..]
    } else {
        v
    }
}
