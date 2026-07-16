use adw::prelude::*;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use crate::config;
use crate::model::Dashboard;
use crate::render::{self, PaintOpts, Plan, Scope};
use crate::theme;
use crate::worker::FromUi;

/// Shared render state across both windows. Holds no widget, so the canvas draw
/// closures don't form a reference cycle.
struct RenderState {
    dashboard: RefCell<Option<Dashboard>>,
    scale: Cell<f64>,
    selecting: Cell<bool>,
    selected: RefCell<HashSet<String>>,
}

/// One canvas window (the compact limits view or the full dashboard).
struct Surface {
    window: adw::ApplicationWindow,
    canvas: gtk::DrawingArea,
    plan: Rc<RefCell<Option<(f64, Plan)>>>,
    subtitle: adw::WindowTitle,
    scope: Scope,
}

#[derive(Clone)]
pub struct AppUi(Rc<Inner>);

struct Inner {
    limits: Surface,
    full: Surface,
    state: Rc<RenderState>,
    provider: gtk::CssProvider,
    /// Toasts for the full dashboard window.
    toast: adw::ToastOverlay,
    /// Toasts for the compact limits window (screenshot confirmations, …).
    limits_toast: adw::ToastOverlay,
    action_bar: gtk::Revealer,
    selected_btn: gtk::Button,
    config: RefCell<config::Config>,
    from_ui: Sender<FromUi>,
}

impl AppUi {
    pub fn new(app: &adw::Application, from_ui: Sender<FromUi>) -> Self {
        let cfg = config::load();
        let scale = cfg.ui_scale;
        let state = Rc::new(RenderState {
            dashboard: RefCell::new(None),
            scale: Cell::new(scale),
            selecting: Cell::new(false),
            selected: RefCell::new(HashSet::new()),
        });

        // Sync canvas palette to the desktop light/dark preference.
        let style = adw::StyleManager::default();
        theme::set_dark(style.is_dark());

        let provider = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        provider.load_from_string(&theme::stylesheet(scale));

        // ---- compact limits window (the default view) ----------------------
        let limits_canvas = new_canvas();
        let limits = Surface {
            window: adw::ApplicationWindow::builder()
                .application(app)
                .title("tokenmaxxing")
                .default_width(config::LIMITS_WIDTH)
                .default_height(config::LIMITS_HEIGHT)
                .width_request(config::LIMITS_WIDTH)
                .height_request(config::LIMITS_HEIGHT)
                .resizable(false)
                .build(),
            subtitle: adw::WindowTitle::new("tokenmaxxing", "current limits"),
            plan: Rc::new(RefCell::new(None)),
            canvas: limits_canvas.clone(),
            scope: Scope::Limits,
        };
        limits.window.add_css_class("tokenmaxxing");
        install_draw(&limits.canvas, &state, &limits.plan, false, Scope::Limits);

        let limits_header = adw::HeaderBar::new();
        limits_header.add_css_class("flat");
        limits_header.set_title_widget(Some(&limits.subtitle));
        let limits_refresh = flat_icon("view-refresh-symbolic", "Refresh now");
        limits_header.pack_start(&limits_refresh);
        let limits_shot = flat_icon("camera-photo-symbolic", "Screenshot this window");
        limits_header.pack_end(&limits_shot);
        let open_full = gtk::Button::with_label("Full dashboard");
        open_full.add_css_class("suggested-action");
        open_full.set_tooltip_text(Some("Open the full usage dashboard"));
        limits_header.pack_end(&open_full);

        let limits_scroll = scroll_for(&limits.canvas);
        let limits_toolbar = adw::ToolbarView::new();
        limits_toolbar.add_top_bar(&limits_header);
        limits_toolbar.set_content(Some(&limits_scroll));
        let limits_toast = adw::ToastOverlay::new();
        limits_toast.set_child(Some(&limits_toolbar));
        limits.window.set_content(Some(&limits_toast));

        // ---- full dashboard window -----------------------------------------
        let full_canvas = new_canvas();
        let full = Surface {
            window: adw::ApplicationWindow::builder()
                .application(app)
                .title("tokenmaxxing — dashboard")
                .default_width(cfg.dashboard_width.unwrap_or(1360))
                .default_height(cfg.dashboard_height.unwrap_or(900))
                .width_request(720)
                .height_request(520)
                .build(),
            subtitle: adw::WindowTitle::new("tokenmaxxing", "usage dashboard"),
            plan: Rc::new(RefCell::new(None)),
            canvas: full_canvas.clone(),
            scope: Scope::Full,
        };
        full.window.add_css_class("tokenmaxxing");
        // Never auto-show: full dashboard only appears when the user asks.
        full.window.set_visible(false);
        install_draw(&full.canvas, &state, &full.plan, true, Scope::Full);

        let full_header = adw::HeaderBar::new();
        full_header.add_css_class("flat");
        full_header.set_title_widget(Some(&full.subtitle));
        let full_refresh = flat_icon("view-refresh-symbolic", "Refresh now");
        full_header.pack_start(&full_refresh);
        let shot = flat_icon("camera-photo-symbolic", "Screenshot — pick panels or export everything");
        full_header.pack_end(&shot);
        let fullscreen = flat_icon("view-fullscreen-symbolic", "Toggle fullscreen");
        full_header.pack_end(&fullscreen);
        let settings = gtk::MenuButton::new();
        settings.set_icon_name("open-menu-symbolic");
        settings.add_css_class("flat");
        full_header.pack_end(&settings);

        let full_scroll = scroll_for(&full.canvas);
        let actions = build_action_bar();
        let action_bar = actions.revealer.clone();
        let selected_btn = actions.selected.clone();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&full_scroll));
        overlay.add_overlay(&action_bar);

        let full_toolbar = adw::ToolbarView::new();
        full_toolbar.add_top_bar(&full_header);
        full_toolbar.set_content(Some(&overlay));
        let toast = adw::ToastOverlay::new();
        toast.set_child(Some(&full_toolbar));
        full.window.set_content(Some(&toast));

        let ui = AppUi(Rc::new(Inner {
            limits,
            full,
            state,
            provider,
            toast,
            limits_toast,
            action_bar,
            selected_btn,
            config: RefCell::new(cfg),
            from_ui,
        }));

        settings.set_popover(Some(&ui.build_settings_popover()));
        ui.wire(
            &limits_refresh,
            &limits_shot,
            &open_full,
            &full_refresh,
            &shot,
            &fullscreen,
        );
        ui.wire_actions(&actions);
        ui.wire_color_scheme();
        ui
    }

    /// Follow the desktop light/dark preference live — CSS chrome + canvas palette.
    fn wire_color_scheme(&self) {
        let style = adw::StyleManager::default();
        let ui = self.clone();
        style.connect_dark_notify(move |sm| {
            theme::set_dark(sm.is_dark());
            let scale = ui.0.state.scale.get();
            ui.0.provider.load_from_string(&theme::stylesheet(scale));
            ui.relayout(Scope::Limits);
            ui.relayout(Scope::Full);
        });
    }

    fn wire(
        &self,
        limits_refresh: &gtk::Button,
        limits_shot: &gtk::Button,
        open_full: &gtk::Button,
        full_refresh: &gtk::Button,
        shot: &gtk::Button,
        fullscreen: &gtk::Button,
    ) {
        for refresh in [limits_refresh, full_refresh] {
            let ui = self.clone();
            refresh.connect_clicked(move |_| {
                let _ = ui.0.from_ui.send(FromUi::RefreshNow);
                ui.0.limits.subtitle.set_subtitle("refreshing…");
                ui.0.full.subtitle.set_subtitle("refreshing…");
            });
        }

        let ui = self.clone();
        limits_shot.connect_clicked(move |_| ui.export_limits_screenshot());

        let ui = self.clone();
        open_full.connect_clicked(move |_| ui.0.full.window.present());

        let ui = self.clone();
        shot.connect_clicked(move |_| ui.enter_screenshot_mode());

        let win = self.0.full.window.clone();
        fullscreen.connect_clicked(move |_| win.set_fullscreened(!win.is_fullscreen()));

        for surface in [&self.0.limits, &self.0.full] {
            let ui = self.clone();
            let scope = surface.scope;
            surface.canvas.connect_resize(move |_, _, _| ui.relayout(scope));
        }

        // Click a panel while in screenshot mode (full canvas only) to toggle it.
        let click = gtk::GestureClick::new();
        let ui = self.clone();
        click.connect_released(move |_, _, x, y| ui.on_canvas_click(x, y));
        self.0.full.canvas.add_controller(click);
    }

    fn wire_actions(&self, actions: &ActionBar) {
        let ui = self.clone();
        actions.all.connect_clicked(move |_| ui.select_all());
        let ui = self.clone();
        actions.everything.connect_clicked(move |_| {
            ui.export(None);
            ui.exit_screenshot_mode();
        });
        let ui = self.clone();
        actions.selected.connect_clicked(move |_| ui.export_selected());
        let ui = self.clone();
        actions.cancel.connect_clicked(move |_| ui.exit_screenshot_mode());
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
        scale_label.add_css_class("dim-label");
        box_.append(&scale_label);

        let choices: Vec<String> = config::SCALE_STEPS.iter().map(|s| format!("{}%", (s * 100.0) as i64)).collect();
        let choice_refs: Vec<&str> = choices.iter().map(String::as_str).collect();
        let dropdown = gtk::DropDown::from_strings(&choice_refs);
        dropdown.set_selected(config::scale_index(self.0.state.scale.get()));
        let ui = self.clone();
        dropdown.connect_selected_notify(move |dd| {
            if let Some(&scale) = config::SCALE_STEPS.get(dd.selected() as usize) {
                ui.set_scale(scale);
            }
        });
        box_.append(&dropdown);
        box_.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let export = gtk::Button::with_label("Export full dashboard…");
        export.add_css_class("flat");
        let ui = self.clone();
        let export_popover = popover.clone();
        export.connect_clicked(move |_| {
            export_popover.popdown();
            ui.export(None);
        });
        box_.append(&export);

        let grok_usage = gtk::Button::with_label("Open Grok usage");
        grok_usage.add_css_class("flat");
        let grok_popover = popover.clone();
        grok_usage.connect_clicked(move |_| {
            grok_popover.popdown();
            open_url("https://grok.com/?_s=usage");
        });
        box_.append(&grok_usage);

        let console = gtk::Button::with_label("Open opencode console");
        console.add_css_class("flat");
        let console_popover = popover.clone();
        console.connect_clicked(move |_| {
            console_popover.popdown();
            open_url("https://opencode.ai/auth");
        });
        box_.append(&console);

        popover.set_child(Some(&box_));
        popover
    }

    /// Show the compact limits window only — fixed size, bottom-right of the
    /// active monitor. The full dashboard stays hidden until asked for.
    pub fn present(&self) {
        self.0.full.window.set_visible(false);
        self.present_limits();
    }

    /// Tray click toggles the compact limits window.
    pub fn toggle(&self) {
        let window = self.0.limits.window.clone();
        if window.is_visible() {
            window.set_visible(false);
        } else {
            self.present_limits();
        }
    }

    fn present_limits(&self) {
        let window = &self.0.limits.window;
        window.set_default_size(config::LIMITS_WIDTH, config::LIMITS_HEIGHT);
        window.set_size_request(config::LIMITS_WIDTH, config::LIMITS_HEIGHT);
        window.present();
        // Wayland clients can't set absolute positions — ask KWin after map.
        glib::timeout_add_local_once(std::time::Duration::from_millis(80), || {
            place_limits_bottom_right();
        });
    }

    /// Hide (instead of quit) when either window is closed — the app stays live
    /// in the tray. Only wired when a tray host exists.
    pub fn set_close_hides_to_tray(&self) {
        for (surface, is_limits) in [(&self.0.limits, true), (&self.0.full, false)] {
            let ui = self.clone();
            surface.window.connect_close_request(move |window| {
                if !is_limits {
                    ui.save_dashboard_size(window);
                }
                window.set_visible(false);
                glib::Propagation::Stop
            });
        }
    }

    /// Remember the full dashboard size (mini window size is fixed).
    fn save_dashboard_size(&self, window: &adw::ApplicationWindow) {
        if window.is_maximized() || window.is_fullscreen() {
            return;
        }
        let (w, h) = (window.width(), window.height());
        if w < 200 || h < 200 {
            return;
        }
        let mut cfg = self.0.config.borrow_mut();
        cfg.dashboard_width = Some(w);
        cfg.dashboard_height = Some(h);
        config::save(&cfg);
    }

    pub fn update(&self, dashboard: &Dashboard) {
        let status = status_line(dashboard);
        self.0.limits.subtitle.set_subtitle("current limits");
        self.0.full.subtitle.set_subtitle(&status);
        *self.0.state.dashboard.borrow_mut() = Some(dashboard.clone());
        self.relayout(Scope::Limits);
        self.relayout(Scope::Full);
    }

    fn relayout(&self, scope: Scope) {
        let surface = self.surface(scope);
        let scale = self.0.state.scale.get();
        let width = (surface.canvas.width() as f64 / scale).max(200.0);
        if let Some(dash) = self.0.state.dashboard.borrow().as_ref() {
            let plan = render::plan(dash, width, scope);
            surface.canvas.set_content_height((plan.height * scale) as i32);
            *surface.plan.borrow_mut() = Some((width, plan));
        }
        surface.canvas.queue_draw();
    }

    fn surface(&self, scope: Scope) -> &Surface {
        match scope {
            Scope::Limits => &self.0.limits,
            Scope::Full => &self.0.full,
        }
    }

    fn set_scale(&self, scale: f64) {
        let scale = scale.clamp(1.0, 2.0);
        if (scale - self.0.state.scale.get()).abs() < 0.001 {
            return;
        }
        self.0.state.scale.set(scale);
        {
            let mut cfg = self.0.config.borrow_mut();
            cfg.ui_scale = scale;
            config::save(&cfg);
        }
        self.0.provider.load_from_string(&theme::stylesheet(scale));
        self.relayout(Scope::Limits);
        self.relayout(Scope::Full);
    }

    // ---- screenshot mode (full window) -------------------------------------

    fn enter_screenshot_mode(&self) {
        self.0.full.window.present();
        self.0.state.selecting.set(true);
        self.0.state.selected.borrow_mut().clear();
        self.update_selected_label();
        self.0.action_bar.set_reveal_child(true);
        self.0.full.canvas.queue_draw();
    }

    fn exit_screenshot_mode(&self) {
        self.0.state.selecting.set(false);
        self.0.state.selected.borrow_mut().clear();
        self.0.action_bar.set_reveal_child(false);
        self.0.full.canvas.queue_draw();
    }

    fn on_canvas_click(&self, x: f64, y: f64) {
        if !self.0.state.selecting.get() {
            return;
        }
        let scale = self.0.state.scale.get();
        let hit = self.0.full.plan.borrow().as_ref().and_then(|(_, plan)| plan.panel_at(x / scale, y / scale));
        if let Some(id) = hit {
            let mut sel = self.0.state.selected.borrow_mut();
            if !sel.insert(id.clone()) {
                sel.remove(&id);
            }
            drop(sel);
            self.update_selected_label();
            self.0.full.canvas.queue_draw();
        }
    }

    fn select_all(&self) {
        if let Some((_, plan)) = self.0.full.plan.borrow().as_ref() {
            *self.0.state.selected.borrow_mut() = plan.selectable_ids().into_iter().collect();
        }
        self.update_selected_label();
        self.0.full.canvas.queue_draw();
    }

    fn update_selected_label(&self) {
        let n = self.0.state.selected.borrow().len();
        self.0.selected_btn.set_label(&format!("Export selected ({n})"));
        self.0.selected_btn.set_sensitive(n > 0);
    }

    fn export_selected(&self) {
        let selected = self.0.state.selected.borrow().clone();
        self.export(Some(selected));
        self.exit_screenshot_mode();
    }

    fn export(&self, selected: Option<HashSet<String>>) {
        let dash = self.0.state.dashboard.borrow();
        let Some(dash) = dash.as_ref() else {
            self.notify_full("Nothing to export yet — still loading");
            return;
        };
        let path = render::default_output();
        match render::export(dash, 1500.0, 2.0, selected.as_ref(), Scope::Full, &path) {
            Ok(()) => self.finish_screenshot(&path, true),
            Err(error) => self.notify_full(&format!("Export failed: {error}")),
        }
    }

    /// One-shot screenshot of the compact limits view — same layout width and
    /// UI scale as the live canvas (no wider re-layout, no export chrome).
    fn export_limits_screenshot(&self) {
        let dash = self.0.state.dashboard.borrow();
        let Some(dash) = dash.as_ref() else {
            self.notify_limits("Nothing to export yet — still loading");
            return;
        };
        let path = render::default_output_named("limits");
        let ui_scale = self.0.state.scale.get();
        // Prefer the plan width the live canvas last laid out with; fall back to
        // canvas allocation / ui_scale (same formula as install_draw).
        let plan_width = self
            .0
            .limits
            .plan
            .borrow()
            .as_ref()
            .map(|(w, _)| *w)
            .unwrap_or_else(|| {
                let canvas_w = self.0.limits.canvas.width().max(1) as f64;
                (canvas_w / ui_scale).max(200.0)
            });
        match render::export_live_view(dash, plan_width, ui_scale, Scope::Limits, &path, 2.0) {
            Ok(()) => self.finish_screenshot(&path, false),
            Err(error) => self.notify_limits(&format!("Export failed: {error}")),
        }
    }

    /// Save toast + clipboard. Clipboard is best-effort but multi-path so it
    /// actually works on KDE Wayland (GDK alone often silently fails).
    fn finish_screenshot(&self, path: &std::path::Path, full_window: bool) {
        let clipped = copy_png_to_clipboard(path);
        let msg = if clipped {
            format!("Copied to clipboard · saved {}", path.display())
        } else {
            format!("Saved {} · clipboard copy failed", path.display())
        };
        let toast = adw::Toast::new(&msg);
        toast.set_button_label(Some("Open"));
        let opened = path.to_path_buf();
        toast.connect_button_clicked(move |_| open_url(&opened.to_string_lossy()));
        if full_window {
            self.0.toast.add_toast(toast);
        } else {
            self.0.limits_toast.add_toast(toast);
        }
    }

    pub fn export_share_card(&self) {
        self.export(None);
    }

    fn notify_full(&self, message: &str) {
        self.0.toast.add_toast(adw::Toast::new(message));
    }

    fn notify_limits(&self, message: &str) {
        self.0.limits_toast.add_toast(adw::Toast::new(message));
    }
}

/// Pin the compact window to the bottom-right of its current monitor via KWin.
/// Wayland forbids clients from setting absolute geometry themselves.
fn place_limits_bottom_right() {
    let w = config::LIMITS_WIDTH;
    let h = config::LIMITS_HEIGHT;
    let margin = config::LIMITS_SCREEN_MARGIN;
    let script = format!(
        r#"
const TARGET_W = {w};
const TARGET_H = {h};
const MARGIN = {margin};
const clients = workspace.windowList();
for (var i = 0; i < clients.length; i++) {{
  var c = clients[i];
  var cls = c.resourceClass ? c.resourceClass.toString().toLowerCase() : "";
  var name = c.resourceName ? c.resourceName.toString().toLowerCase() : "";
  var cap = c.caption ? c.caption.toString() : "";
  if (cls.indexOf("tokenmaxxing") < 0 && name.indexOf("tokenmaxxing") < 0) continue;
  if (cap.indexOf("dashboard") >= 0) continue;
  var area = null;
  try {{ area = workspace.clientArea(KWin.PlacementArea, c); }}
  catch (e1) {{
    try {{ area = workspace.clientArea(KWin.WorkArea, c.output, 0); }}
    catch (e2) {{
      try {{ area = workspace.clientArea(KWin.MaximizeArea, c.screen, c.desktop); }}
      catch (e3) {{ continue; }}
    }}
  }}
  var x = Math.round(area.x + area.width - TARGET_W - MARGIN);
  var y = Math.round(area.y + area.height - TARGET_H - MARGIN);
  c.frameGeometry = {{ x: x, y: y, width: TARGET_W, height: TARGET_H }};
}}
"#
    );
    let path = std::env::temp_dir().join("tokenmaxxing-place.js");
    if std::fs::write(&path, script).is_err() {
        return;
    }
    let path_str = path.to_string_lossy().into_owned();
    // Fire-and-forget — placement is best-effort on non-KWin desktops.
    let _ = std::process::Command::new("qdbus6")
        .args([
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.unloadScript",
            &path_str,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::process::Command::new("qdbus6")
        .args([
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.loadScript",
            &path_str,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

struct ActionBar {
    revealer: gtk::Revealer,
    all: gtk::Button,
    everything: gtk::Button,
    selected: gtk::Button,
    cancel: gtk::Button,
}

fn build_action_bar() -> ActionBar {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("action-bar");
    bar.set_halign(gtk::Align::Center);
    bar.set_valign(gtk::Align::End);

    let hint = gtk::Label::new(Some("Screenshot — click panels to include"));
    hint.add_css_class("dim-label");
    bar.append(&hint);

    let all = gtk::Button::with_label("Select all");
    all.add_css_class("flat");
    let everything = gtk::Button::with_label("Export everything");
    everything.add_css_class("flat");
    let selected = gtk::Button::with_label("Export selected (0)");
    selected.add_css_class("suggested-action");
    selected.set_sensitive(false);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("flat");
    bar.append(&all);
    bar.append(&everything);
    bar.append(&selected);
    bar.append(&cancel);

    let revealer = gtk::Revealer::new();
    revealer.set_transition_type(gtk::RevealerTransitionType::SlideUp);
    revealer.set_valign(gtk::Align::End);
    revealer.set_halign(gtk::Align::Center);
    revealer.set_margin_bottom(18);
    revealer.set_child(Some(&bar));
    revealer.set_reveal_child(false);
    ActionBar { revealer, all, everything, selected, cancel }
}

fn new_canvas() -> gtk::DrawingArea {
    let canvas = gtk::DrawingArea::new();
    canvas.set_hexpand(true);
    canvas.set_content_height(400);
    canvas
}

fn scroll_for(canvas: &gtk::DrawingArea) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_vexpand(true);
    scroll.set_child(Some(canvas));
    scroll
}

fn flat_icon(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(tooltip));
    button
}

fn install_draw(canvas: &gtk::DrawingArea, state: &Rc<RenderState>, plan_cell: &Rc<RefCell<Option<(f64, Plan)>>>, selectable: bool, scope: Scope) {
    let state = state.clone();
    let plan_cell = plan_cell.clone();
    canvas.set_draw_func(move |_area, cr, w, _h| {
        let scale = state.scale.get();
        let content_w = (w as f64 / scale).max(200.0);
        let plan = {
            let dash = state.dashboard.borrow();
            match dash.as_ref() {
                Some(d) => render::plan(d, content_w, scope),
                None => {
                    loading(cr, w as f64);
                    return;
                }
            }
        };
        cr.scale(scale, scale);
        let selected = state.selected.borrow();
        let opts = PaintOpts { selecting: selectable && state.selecting.get(), selected: &selected };
        render::paint(cr, &plan, content_w, &opts);
        drop(selected);
        *plan_cell.borrow_mut() = Some((content_w, plan));
    });
}

fn loading(cr: &gtk::cairo::Context, w: f64) {
    cr.set_source_rgb(theme::bg().0, theme::bg().1, theme::bg().2);
    cr.paint().ok();
    cr.select_font_face("sans-serif", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
    cr.set_font_size(15.0);
    cr.set_source_rgb(theme::muted().0, theme::muted().1, theme::muted().2);
    let msg = "Reading quotas…";
    let tw = cr.text_extents(msg).map(|e| e.width()).unwrap_or(0.0);
    cr.move_to((w - tw) / 2.0, 80.0);
    let _ = cr.show_text(msg);
}

fn status_line(dash: &Dashboard) -> String {
    let now = dash.generated_at.format("%H:%M:%S");
    format!("updated {now}  ·  tokenmaxxing {}", env!("CARGO_PKG_VERSION"))
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Put a PNG on the system clipboard. Tries GDK first, then `wl-copy` (Wayland),
/// then `xclip` (X11). Returns whether at least one path succeeded.
fn copy_png_to_clipboard(path: &std::path::Path) -> bool {
    let Ok(data) = std::fs::read(path) else {
        return false;
    };
    let mut ok = false;

    // GDK — ContentProvider with image/png is more reliable than set_texture alone.
    if let Some(display) = gtk::gdk::Display::default() {
        let bytes = glib::Bytes::from_owned(data.clone());
        let provider = gtk::gdk::ContentProvider::for_bytes("image/png", &bytes);
        if display.clipboard().set_content(Some(&provider)).is_ok() {
            ok = true;
        } else if let Ok(texture) = gtk::gdk::Texture::from_bytes(&bytes) {
            display.clipboard().set_texture(&texture);
            ok = true;
        }
    }

    // wl-copy — the reliable path on KDE/wlroots Wayland.
    if pipe_to_cmd(&data, "wl-copy", &["--type", "image/png"]) {
        ok = true;
    } else if pipe_to_cmd(&data, "xclip", &["-selection", "clipboard", "-t", "image/png"]) {
        ok = true;
    }

    ok
}

fn pipe_to_cmd(data: &[u8], program: &str, args: &[&str]) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return false;
    };
    if stdin.write_all(data).is_err() {
        let _ = child.kill();
        return false;
    }
    drop(stdin);
    child.wait().map(|s| s.success()).unwrap_or(false)
}
