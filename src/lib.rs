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
    let activity_generation = match egui_frontend::android::initialize(&app) {
        Ok(activity_generation) => activity_generation,
        Err(err) => {
            app_log_error!(frontend, "failed to initialize Android runtime", err);
            return;
        }
    };
    let options = app::preview::PreviewOptions {
        frontend: app::preview::FrontendKind::Egui,
        reset: false,
        ..app::preview::PreviewOptions::default()
    };
    let result = egui_frontend::app::run_egui_frontend_android(options, app, activity_generation);
    egui_frontend::android::runtime_exited(activity_generation);
    if let Err(err) = result {
        app_log_error!(frontend, "failed to start Android egui frontend", err);
    }
}
