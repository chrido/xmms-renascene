use std::path::PathBuf;

use xmms_resuscitated::skin::DefaultSkin;
use xmms_resuscitated::ui;

fn main() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let skin_dir = repo_root.join("data").join("defskin");
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "--gtk") {
        ui::run_default_skin_preview(skin_dir);
        return;
    }
    if args.iter().any(|arg| arg == "--gtk-smoke") {
        ui::run_default_skin_preview_smoke(skin_dir);
        return;
    }

    match DefaultSkin::load_from_dir(&skin_dir) {
        Ok(skin) => {
            println!(
                "xmms-rs: loaded {} default skin pixmaps from {}",
                skin.loaded_pixmap_count(),
                skin_dir.display()
            );
        }
        Err(err) => {
            eprintln!("xmms-rs: failed to load default skin: {err}");
            std::process::exit(1);
        }
    }
}
