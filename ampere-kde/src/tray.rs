use crate::gauge;
use crate::worker::FromUi;
use gtk::cairo::{Context, Format, ImageSurface};
use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{Category, Icon, Status, ToolTip, Tray};
use std::sync::mpsc::Sender;

pub enum TrayEvent {
    Toggle,
    Export,
    Quit,
}

struct AmpereTray {
    ui_tx: async_channel::Sender<TrayEvent>,
    to_worker: Sender<FromUi>,
    icon: Vec<Icon>,
}

impl Tray for AmpereTray {
    fn id(&self) -> String {
        "dev.guitaripod.Ampere".into()
    }

    fn title(&self) -> String {
        "Ampere".into()
    }

    fn icon_name(&self) -> String {
        "ampere".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon.clone()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
            title: "Ampere".into(),
            description: "LLM token quotas".into(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.ui_tx.send_blocking(TrayEvent::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Show / hide Ampere".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.ui_tx.send_blocking(TrayEvent::Toggle);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Refresh now".into(),
                icon_name: "view-refresh-symbolic".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.to_worker.send(FromUi::RefreshNow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Export share card…".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.ui_tx.send_blocking(TrayEvent::Export);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open opencode console".into(),
                activate: Box::new(|_| open_url("https://opencode.ai/auth")),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit Ampere".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.ui_tx.send_blocking(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the StatusNotifierItem on its own D-Bus thread. Returns false when the
/// desktop has no working tray host, so the caller can keep the window quittable.
pub fn start(ui_tx: async_channel::Sender<TrayEvent>, to_worker: Sender<FromUi>) -> bool {
    let tray = AmpereTray {
        ui_tx,
        to_worker,
        icon: vec![render_icon(32), render_icon(22)],
    };
    match tray.spawn() {
        Ok(handle) => {
            std::mem::forget(handle);
            true
        }
        Err(error) => {
            eprintln!("ampere: system tray unavailable ({error}); window stays open");
            false
        }
    }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Render the Ampere mark and convert Cairo's premultiplied native-endian
/// buffer into the ARGB32 network-byte-order buffer SNI expects.
fn render_icon(size: i32) -> Icon {
    let mut surface = ImageSurface::create(Format::ARgb32, size, size).expect("icon surface");
    if let Ok(cr) = Context::new(&surface) {
        gauge::draw_logo(&cr, f64::from(size));
    }
    surface.flush();

    let stride = surface.stride() as usize;
    let width = size as usize;
    let height = size as usize;
    let pixels = surface.data().expect("icon pixels");

    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = &pixels[y * stride..y * stride + width * 4];
        for x in 0..width {
            let p = &row[x * 4..x * 4 + 4];
            let (b, g, r, a) = (p[0], p[1], p[2], p[3]);
            let straight = |c: u8| {
                if a == 0 {
                    0
                } else {
                    ((u32::from(c) * 255) / u32::from(a)).min(255) as u8
                }
            };
            data.extend_from_slice(&[a, straight(r), straight(g), straight(b)]);
        }
    }
    Icon {
        width: size,
        height: size,
        data,
    }
}
