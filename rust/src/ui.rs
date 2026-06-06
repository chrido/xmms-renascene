use std::rc::Rc;

use gtk::prelude::*;

use crate::render::surface_from_xpm;
use crate::skin::{DefaultSkin, SkinPixmapKind};

const MAINWIN_WIDTH: i32 = 275;
const MAINWIN_HEIGHT: i32 = 116;
const DEFAULT_SCALE: i32 = 2;

pub fn run_default_skin_preview() {
    run_preview_application(PreviewMode::Interactive);
}

pub fn run_default_skin_preview_smoke() {
    run_preview_application(PreviewMode::Smoke);
}

enum PreviewMode {
    Interactive,
    Smoke,
}

fn run_preview_application(mode: PreviewMode) {
    let app = gtk::Application::builder()
        .application_id("org.xmms.Resuscitated.RustPreview")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        if let Err(err) = build_preview_window(app) {
            eprintln!("xmms-rs: failed to create GTK preview: {err}");
            app.quit();
            return;
        }

        if matches!(mode, PreviewMode::Smoke) {
            let app = app.clone();
            gtk::glib::idle_add_local_once(move || app.quit());
        }
    });

    app.run_with_args(&["xmms-rs"]);
}

fn build_preview_window(app: &gtk::Application) -> Result<(), String> {
    let skin = DefaultSkin::load_bundled().map_err(|err| err.to_string())?;
    let main = skin
        .get(SkinPixmapKind::Main)
        .ok_or_else(|| "default main skin pixmap is missing".to_string())?;
    let surface = Rc::new(surface_from_xpm(main).map_err(|err| err.to_string())?);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Resuscitated Rust Preview")
        .resizable(false)
        .decorated(false)
        .default_width(MAINWIN_WIDTH * DEFAULT_SCALE)
        .default_height(MAINWIN_HEIGHT * DEFAULT_SCALE)
        .build();

    let drawing_area = gtk::DrawingArea::builder()
        .content_width(MAINWIN_WIDTH * DEFAULT_SCALE)
        .content_height(MAINWIN_HEIGHT * DEFAULT_SCALE)
        .focusable(true)
        .build();

    drawing_area.set_draw_func(move |_area, cr, width, height| {
        cr.scale(
            width as f64 / MAINWIN_WIDTH as f64,
            height as f64 / MAINWIN_HEIGHT as f64,
        );
        if let Err(err) = cr.set_source_surface(&*surface, 0.0, 0.0) {
            eprintln!("xmms-rs: failed to set cairo source surface: {err}");
            return;
        }
        if let Err(err) = cr.paint() {
            eprintln!("xmms-rs: failed to paint cairo surface: {err}");
        }
    });

    window.set_child(Some(&drawing_area));
    window.present();
    Ok(())
}
