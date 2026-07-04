#[cfg(feature = "egui-ui")]
use xmms_renascene::app::preview::{FrontendKind, PreviewOptions};

#[cfg(feature = "egui-ui")]
fn main() {
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: xmms-egui [preview options]");
        return;
    }
    let options = PreviewOptions {
        frontend: FrontendKind::Egui,
        ..PreviewOptions::default()
    };
    if let Err(err) = xmms_renascene::egui_frontend::app::run_egui_frontend(options) {
        eprintln!("xmms-egui: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "egui-ui"))]
fn main() {
    eprintln!("xmms-egui: this binary was built without the egui-ui frontend");
    std::process::exit(2);
}
