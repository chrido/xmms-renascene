pub mod app;
pub mod app_state;
pub mod audio_model;
pub mod config;
#[cfg(feature = "gtk-ui")]
pub mod e2e;
#[cfg(any(feature = "egui-ui", feature = "mobile-ui"))]
#[path = "ui/egui/mod.rs"]
pub mod egui_frontend;
pub mod equalizer;
pub mod mpris;
pub mod playback;
pub mod player;
pub mod playlist;
pub mod render;
pub mod session;
pub mod skin;
#[cfg(feature = "gtk-ui")]
pub mod skineditor;
pub mod socket_control;
#[cfg(feature = "gtk-ui")]
pub mod ui;

#[cfg(all(target_os = "android", feature = "mobile-ui"))]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    if let Err(err) = egui_frontend::android_file_picker::initialize(&app) {
        app_log_error!(frontend, "failed to initialize Android file picker", err);
    }
    let options = app::preview::PreviewOptions {
        frontend: app::preview::FrontendKind::Egui,
        reset: false,
        ..app::preview::PreviewOptions::default()
    };
    if let Err(err) = egui_frontend::app::run_egui_frontend_android(options, app) {
        app_log_error!(frontend, "failed to start Android egui frontend", err);
    }
}
