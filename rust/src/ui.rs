use std::rc::Rc;

use gtk::prelude::*;

use crate::render::{render_main_player_reset, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH};
use crate::skin::DefaultSkin;

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
    let skin = Rc::new(skin);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Resuscitated Rust Preview")
        .resizable(false)
        .decorated(false)
        .default_width(MAIN_WINDOW_WIDTH * DEFAULT_SCALE)
        .default_height(MAIN_WINDOW_HEIGHT * DEFAULT_SCALE)
        .build();

    let drawing_area = gtk::DrawingArea::builder()
        .content_width(MAIN_WINDOW_WIDTH * DEFAULT_SCALE)
        .content_height(MAIN_WINDOW_HEIGHT * DEFAULT_SCALE)
        .focusable(true)
        .build();

    drawing_area.set_draw_func(move |_area, cr, width, height| {
        cr.scale(
            width as f64 / MAIN_WINDOW_WIDTH as f64,
            height as f64 / MAIN_WINDOW_HEIGHT as f64,
        );
        if let Err(err) = render_main_player_reset(cr, &skin) {
            eprintln!("xmms-rs: failed to render reset-state preview: {err}");
        }
    });

    window.set_child(Some(&drawing_area));
    window.present();
    Ok(())
}
