use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use crate::config;
use crate::gauge;
use crate::model::{Gauge, Snapshot, Unit};
use crate::sharecard;
use crate::theme;
use crate::worker::FromUi;

const BADGE_CLASSES: [&str; 3] = ["badge-live", "badge-est", "badge-offline"];
const PROVIDERS: [(&str, &str); 2] = [("anthropic", "Claude"), ("opencode-go", "opencode go")];

#[derive(Clone)]
pub struct AppUi(Rc<Inner>);

struct Inner {
    window: adw::ApplicationWindow,
    toast: adw::ToastOverlay,
    provider: gtk::CssProvider,
    cards_box: gtk::Box,
    cards: RefCell<Vec<Card>>,
    updated: gtk::Label,
    from_ui: Sender<FromUi>,
    scale: Cell<f64>,
    latest: RefCell<Vec<Snapshot>>,
}

impl AppUi {
    pub fn new(app: &adw::Application, from_ui: Sender<FromUi>) -> Self {
        let scale = config::load().ui_scale;

        let provider = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        provider.load_from_string(&theme::stylesheet(scale));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Ampere")
            .default_width((452.0 * scale) as i32)
            .default_height((780.0 * scale) as i32)
            .width_request((420.0 * scale) as i32)
            .build();
        window.add_css_class("ampere");

        let cards_box = gtk::Box::new(gtk::Orientation::Vertical, (14.0 * scale) as i32);
        let cards: Vec<Card> = PROVIDERS
            .iter()
            .map(|(id, name)| {
                let card = Card::new(id, name, scale);
                cards_box.append(&card.root);
                card
            })
            .collect();

        let updated = gtk::Label::new(Some("Connecting…"));
        updated.add_css_class("muted");
        updated.add_css_class("footer");
        updated.set_halign(gtk::Align::Start);
        updated.set_xalign(0.0);

        let content = gtk::Box::new(gtk::Orientation::Vertical, (14.0 * scale) as i32);
        content.set_margin_top(14);
        content.set_margin_bottom(14);
        content.set_margin_start(14);
        content.set_margin_end(14);
        content.append(&cards_box);
        content.append(&updated);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_vexpand(true);
        scroll.set_child(Some(&content));

        let header = adw::HeaderBar::new();
        header.add_css_class("flat");
        header.set_title_widget(Some(&adw::WindowTitle::new("Ampere", "token quotas")));
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.add_css_class("flat");
        refresh.set_tooltip_text(Some("Refresh now"));
        header.pack_end(&refresh);
        let settings = gtk::MenuButton::new();
        settings.set_icon_name("open-menu-symbolic");
        settings.add_css_class("flat");
        settings.set_tooltip_text(Some("Settings"));
        header.pack_end(&settings);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scroll));

        let toast = adw::ToastOverlay::new();
        toast.set_child(Some(&toolbar));
        window.set_content(Some(&toast));

        let ui = AppUi(Rc::new(Inner {
            window,
            toast,
            provider,
            cards_box,
            cards: RefCell::new(cards),
            updated,
            from_ui,
            scale: Cell::new(scale),
            latest: RefCell::new(Vec::new()),
        }));

        settings.set_popover(Some(&ui.build_settings_popover()));

        let on_refresh = ui.clone();
        refresh.connect_clicked(move |_| {
            let _ = on_refresh.0.from_ui.send(FromUi::RefreshNow);
            on_refresh.0.updated.set_text("Refreshing…");
        });

        ui
    }

    fn build_settings_popover(&self) -> gtk::Popover {
        let popover = gtk::Popover::new();
        popover.add_css_class("settings");
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 8);
        box_.set_margin_top(12);
        box_.set_margin_bottom(12);
        box_.set_margin_start(12);
        box_.set_margin_end(12);

        let scale_label = gtk::Label::new(Some("Interface scale"));
        scale_label.set_halign(gtk::Align::Start);
        scale_label.add_css_class("muted");
        box_.append(&scale_label);

        let choices: Vec<String> = config::SCALE_STEPS
            .iter()
            .map(|s| format!("{}%", (s * 100.0) as i64))
            .collect();
        let choice_refs: Vec<&str> = choices.iter().map(String::as_str).collect();
        let dropdown = gtk::DropDown::from_strings(&choice_refs);
        dropdown.set_selected(config::scale_index(self.0.scale.get()));
        let on_scale = self.clone();
        dropdown.connect_selected_notify(move |dd| {
            let index = dd.selected() as usize;
            if let Some(&scale) = config::SCALE_STEPS.get(index) {
                on_scale.set_scale(scale);
            }
        });
        box_.append(&dropdown);

        box_.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let export = gtk::Button::with_label("Export share card…");
        export.add_css_class("flat");
        let on_export = self.clone();
        let export_popover = popover.clone();
        export.connect_clicked(move |_| {
            export_popover.popdown();
            on_export.export_share_card();
        });
        box_.append(&export);

        let console = gtk::Button::with_label("Open opencode console");
        console.add_css_class("flat");
        let console_popover = popover.clone();
        console.connect_clicked(move |_| {
            console_popover.popdown();
            let _ = std::process::Command::new("xdg-open")
                .arg("https://opencode.ai/auth")
                .spawn();
        });
        box_.append(&console);

        popover.set_child(Some(&box_));
        popover
    }

    pub fn present(&self) {
        self.0.window.present();
    }

    pub fn toggle(&self) {
        let window = &self.0.window;
        if window.is_visible() {
            window.set_visible(false);
        } else {
            window.present();
        }
    }

    pub fn window(&self) -> adw::ApplicationWindow {
        self.0.window.clone()
    }

    pub fn update(&self, snapshots: &[Snapshot]) {
        *self.0.latest.borrow_mut() = snapshots.to_vec();
        self.apply_latest();
        let now = chrono::Local::now().format("%H:%M:%S");
        self.0
            .updated
            .set_text(&format!("updated {now}   ·   ampere 0.1.0"));
    }

    pub fn export_share_card(&self) {
        let latest = self.0.latest.borrow().clone();
        if latest.is_empty() {
            self.notify("Nothing to export yet — still loading");
            return;
        }
        let path = sharecard::default_output();
        match sharecard::render(&latest, &path) {
            Ok(()) => {
                let toast = adw::Toast::new(&format!("Saved {}", path.display()));
                toast.set_button_label(Some("Open"));
                let opened = path.clone();
                toast.connect_button_clicked(move |_| {
                    let _ = std::process::Command::new("xdg-open").arg(&opened).spawn();
                });
                self.0.toast.add_toast(toast);
            }
            Err(error) => self.notify(&format!("Export failed: {error}")),
        }
    }

    fn set_scale(&self, scale: f64) {
        let scale = scale.clamp(1.0, 2.0);
        if (scale - self.0.scale.get()).abs() < 0.001 {
            return;
        }
        self.0.scale.set(scale);
        config::save(&config::Config { ui_scale: scale });
        self.0.provider.load_from_string(&theme::stylesheet(scale));
        self.rebuild_cards();
    }

    fn rebuild_cards(&self) {
        let scale = self.0.scale.get();
        while let Some(child) = self.0.cards_box.first_child() {
            self.0.cards_box.remove(&child);
        }
        let cards: Vec<Card> = PROVIDERS
            .iter()
            .map(|(id, name)| {
                let card = Card::new(id, name, scale);
                self.0.cards_box.append(&card.root);
                card
            })
            .collect();
        *self.0.cards.borrow_mut() = cards;
        self.apply_latest();
    }

    fn apply_latest(&self) {
        let latest = self.0.latest.borrow().clone();
        let cards = self.0.cards.borrow();
        for snap in &latest {
            if let Some(card) = cards.iter().find(|c| c.provider_id == snap.provider_id) {
                card.apply(snap);
            }
        }
    }

    fn notify(&self, message: &str) {
        self.0.toast.add_toast(adw::Toast::new(message));
    }
}

struct Card {
    provider_id: String,
    scale: f64,
    root: gtk::Box,
    subtitle: gtk::Label,
    source: gtk::Label,
    badge: gtk::Label,
    gauges: gtk::FlowBox,
    details: gtk::Box,
    note: gtk::Label,
    error: gtk::Label,
}

impl Card {
    fn new(provider_id: &str, name: &str, scale: f64) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, (8.0 * scale) as i32);
        root.add_css_class("card");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let name_label = gtk::Label::new(Some(name));
        name_label.add_css_class("provider-name");
        name_label.add_css_class(theme::provider_accent_class(provider_id));
        name_label.set_halign(gtk::Align::Start);
        name_label.set_hexpand(true);
        name_label.set_xalign(0.0);
        let badge = gtk::Label::new(Some("…"));
        badge.add_css_class("badge");
        badge.set_valign(gtk::Align::Center);
        header.append(&name_label);
        header.append(&badge);
        root.append(&header);

        let subtitle = gtk::Label::new(None);
        subtitle.add_css_class("muted");
        subtitle.add_css_class("subtitle");
        subtitle.set_halign(gtk::Align::Start);
        subtitle.set_xalign(0.0);
        root.append(&subtitle);

        let source = gtk::Label::new(None);
        source.add_css_class("metric-sub");
        source.set_halign(gtk::Align::Start);
        source.set_xalign(0.0);
        root.append(&source);

        let gauges = gtk::FlowBox::new();
        gauges.set_orientation(gtk::Orientation::Horizontal);
        gauges.set_selection_mode(gtk::SelectionMode::None);
        gauges.set_min_children_per_line(1);
        gauges.set_max_children_per_line(3);
        gauges.set_homogeneous(true);
        gauges.set_row_spacing((10.0 * scale) as u32);
        gauges.set_column_spacing((6.0 * scale) as u32);
        gauges.set_margin_top(8);
        gauges.set_can_focus(false);
        root.append(&gauges);

        let details = gtk::Box::new(gtk::Orientation::Vertical, (2.0 * scale) as i32);
        details.set_margin_top(8);
        root.append(&details);

        let note = muted_wrapped("note");
        root.append(&note);
        let error = muted_wrapped("error");
        root.append(&error);

        Self {
            provider_id: provider_id.into(),
            scale,
            root,
            subtitle,
            source,
            badge,
            gauges,
            details,
            note,
            error,
        }
    }

    fn apply(&self, snap: &Snapshot) {
        self.subtitle.set_text(&snap.subtitle);
        self.source.set_text(&snap.source);

        self.badge.set_text(snap.authority.badge());
        for class in BADGE_CLASSES {
            self.badge.remove_css_class(class);
        }
        self.badge.add_css_class(snap.authority.css_class());

        let accent = theme::provider_accent(&snap.provider_id);

        while let Some(child) = self.gauges.first_child() {
            self.gauges.remove(&child);
        }
        for gauge in &snap.gauges {
            self.gauges.append(&gauge_cell(gauge, accent, self.scale));
        }

        while let Some(child) = self.details.first_child() {
            self.details.remove(&child);
        }
        if !snap.details.is_empty() {
            self.details
                .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            for (key, value) in &snap.details {
                self.details.append(&detail_row(key, value));
            }
        }

        set_optional(&self.note, snap.note.as_deref());
        set_optional(&self.error, snap.error.as_deref());
    }
}

fn detail_row(key: &str, value: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let key_label = gtk::Label::new(Some(key));
    key_label.add_css_class("metric-sub");
    key_label.set_halign(gtk::Align::Start);
    key_label.set_hexpand(true);
    key_label.set_xalign(0.0);
    let value_label = gtk::Label::new(Some(value));
    value_label.add_css_class("metric-value");
    value_label.set_halign(gtk::Align::End);
    row.append(&key_label);
    row.append(&value_label);
    row
}

/// One quota window shown as its own labelled ring gauge.
fn gauge_cell(g: &Gauge, accent: theme::Rgb, scale: f64) -> gtk::Box {
    let cell = gtk::Box::new(gtk::Orientation::Vertical, (3.0 * scale) as i32);
    cell.set_halign(gtk::Align::Center);
    cell.set_hexpand(true);

    let ring = gtk::DrawingArea::new();
    let ring_px = (108.0 * scale) as i32;
    ring.set_content_width(ring_px);
    ring.set_content_height(ring_px);
    ring.set_halign(gtk::Align::Center);
    let fraction = g.fraction;
    let color = theme::gauge_color(accent, g.severity());
    let center = g.percent_text();
    ring.set_draw_func(move |_, cr, w, h| {
        gauge::draw_ring(cr, w, h, fraction, color, &center, "");
    });
    cell.append(&ring);

    let label = gtk::Label::new(Some(&g.label));
    label.add_css_class("metric-label");
    label.set_wrap(true);
    label.set_justify(gtk::Justification::Center);
    label.set_max_width_chars(16);
    label.set_halign(gtk::Align::Center);
    cell.append(&label);

    if let Some(sub) = sub_line(g) {
        let sub_label = gtk::Label::new(Some(&sub));
        sub_label.add_css_class("metric-sub");
        sub_label.set_wrap(true);
        sub_label.set_justify(gtk::Justification::Center);
        sub_label.set_max_width_chars(18);
        sub_label.set_halign(gtk::Align::Center);
        cell.append(&sub_label);
    }
    cell
}

fn sub_line(g: &Gauge) -> Option<String> {
    let mut parts = Vec::new();
    if g.unit == Unit::Usd {
        if let (Some(used), Some(limit)) = (g.used, g.limit) {
            parts.push(format!("${used:.2} / ${limit:.0}"));
        }
    }
    if let Some(detail) = &g.detail {
        parts.push(detail.clone());
    }
    if let Some(reset) = g.resets_at {
        let human = humanize_until(reset);
        parts.push(if g.trusted_reset {
            format!("resets in {human}")
        } else {
            format!("~resets in {human}")
        });
    }
    (!parts.is_empty()).then(|| parts.join("   ·   "))
}

fn humanize_until(dt: chrono::DateTime<chrono::Utc>) -> String {
    let seconds = (dt - chrono::Utc::now()).num_seconds();
    if seconds <= 0 {
        return "now".into();
    }
    let (days, hours, minutes) = (seconds / 86_400, (seconds % 86_400) / 3_600, (seconds % 3_600) / 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn muted_wrapped(class: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class(class);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_halign(gtk::Align::Start);
    label.set_visible(false);
    label
}

fn set_optional(label: &gtk::Label, text: Option<&str>) {
    match text {
        Some(value) if !value.is_empty() => {
            label.set_text(value);
            label.set_visible(true);
        }
        _ => {
            label.set_text("");
            label.set_visible(false);
        }
    }
}
