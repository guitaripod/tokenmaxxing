mod charts;
mod config;
mod creds;
mod format;
mod gauge;
mod icon;
mod model;
mod pricing;
mod providers;
mod render;
mod theme;
mod tray;
mod ui;
mod worker;

use adw::prelude::*;
use gtk::glib;
use std::sync::mpsc;

const APP_ID: &str = "dev.guitaripod.tokenmaxxing";

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    for (flag, scope) in [("--export", render::Scope::Full), ("--export-limits", render::Scope::Limits)] {
        if let Some(index) = args.iter().position(|a| a == flag) {
            let path = args
                .get(index + 1)
                .filter(|value| !value.starts_with('-'))
                .map(std::path::PathBuf::from);
            return run_export(path, scope);
        }
    }
    if let Some(index) = args.iter().position(|a| a == "--icon") {
        let path = args
            .get(index + 1)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp/tokenmaxxing-icon.png"));
        let size = args.get(index + 2).and_then(|s| s.parse().ok()).unwrap_or(1024);
        return match icon::render(size, &path) {
            Ok(()) => {
                println!("{}", path.display());
                glib::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("icon render failed: {error}");
                glib::ExitCode::FAILURE
            }
        };
    }

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    });
    app.connect_activate(build_ui);
    app.run()
}

/// Headless one-shot: build the dashboard (or just the compact limits view) and
/// write it to a PNG, no GUI.
fn run_export(path: Option<std::path::PathBuf>, scope: render::Scope) -> glib::ExitCode {
    let dashboard = worker::snapshot_once();
    let output = path.unwrap_or_else(render::default_output);
    let width = if scope == render::Scope::Limits { 520.0 } else { 1500.0 };
    match render::export(&dashboard, width, 2.0, None, scope, &output) {
        Ok(()) => {
            println!("{}", output.display());
            glib::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("export failed: {error}");
            glib::ExitCode::FAILURE
        }
    }
}

fn build_ui(app: &adw::Application) {
    let (to_ui_tx, to_ui_rx) = async_channel::unbounded::<worker::ToUi>();
    let (from_ui_tx, from_ui_rx) = mpsc::channel::<worker::FromUi>();
    worker::spawn(to_ui_tx, from_ui_rx);

    let app_ui = ui::AppUi::new(app, from_ui_tx.clone());
    app_ui.present();

    let (tray_tx, tray_rx) = async_channel::unbounded::<tray::TrayEvent>();
    if tray::start(tray_tx, from_ui_tx) {
        std::mem::forget(app.hold());
        app_ui.set_close_hides_to_tray();
    }

    glib::spawn_future_local(glib::clone!(
        #[strong]
        app_ui,
        #[strong]
        app,
        async move {
            while let Ok(event) = tray_rx.recv().await {
                match event {
                    tray::TrayEvent::Toggle => app_ui.toggle(),
                    tray::TrayEvent::Export => app_ui.export_share_card(),
                    tray::TrayEvent::Quit => app.quit(),
                }
            }
        }
    ));

    glib::spawn_future_local(glib::clone!(
        #[strong]
        app_ui,
        async move {
            while let Ok(message) = to_ui_rx.recv().await {
                match message {
                    worker::ToUi::Dashboard(dashboard) => app_ui.update(&dashboard),
                }
            }
        }
    ));
}
