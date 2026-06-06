use xmms_resuscitated::skin::DefaultSkin;
use xmms_resuscitated::ui;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "--gtk") {
        ui::run_default_skin_preview();
        return;
    }
    if args.iter().any(|arg| arg == "--gtk-smoke") {
        ui::run_default_skin_preview_smoke();
        return;
    }

    match DefaultSkin::load_bundled() {
        Ok(skin) => {
            println!(
                "xmms-rs: loaded {} bundled default skin pixmaps",
                skin.loaded_pixmap_count(),
            );
        }
        Err(err) => {
            eprintln!("xmms-rs: failed to load default skin: {err}");
            std::process::exit(1);
        }
    }
}
