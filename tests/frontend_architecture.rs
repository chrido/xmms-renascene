use std::fs;
use std::path::{Path, PathBuf};

fn collect_rust_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path.to_path_buf());
        }
        return;
    }

    for entry in fs::read_dir(path).expect("frontend source directory") {
        collect_rust_files(&entry.expect("frontend source entry").path(), files);
    }
}

fn production_source(source: &str) -> &str {
    source
        .split_once("#[cfg(test)]\nmod tests")
        .map_or(source, |(production, _)| production)
}

fn calls_state_mut(line: &str) -> bool {
    line.contains(".state_mut") || line.contains("AppStore::state_mut")
}

#[test]
fn production_frontends_cannot_use_app_store_state_mut() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("src/ui.rs")];
    collect_rust_files(&root.join("src/ui"), &mut files);
    let mut offenders = Vec::new();

    for path in files {
        let source = fs::read_to_string(&path).expect("frontend source");
        for (line_index, line) in production_source(&source).lines().enumerate() {
            if calls_state_mut(line) {
                offenders.push(format!(
                    "{}:{}",
                    path.strip_prefix(root).unwrap_or(&path).display(),
                    line_index + 1
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "production frontend code must dispatch through AppStore, not call state_mut: {offenders:?}"
    );
}

#[test]
fn mutable_controller_access_stays_inside_app_store() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let controller = include_str!("../src/app/controller.rs");
    let store = include_str!("../src/app/store.rs");
    let mut files = Vec::new();
    collect_rust_files(&root.join("src/app"), &mut files);
    let mut offenders = Vec::new();

    for path in files {
        if path.ends_with("store.rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("application source");
        for (line_index, line) in production_source(&source).lines().enumerate() {
            if calls_state_mut(line) {
                offenders.push(format!(
                    "{}:{}",
                    path.strip_prefix(root).unwrap_or(&path).display(),
                    line_index + 1
                ));
            }
        }
    }

    assert!(controller.contains("pub(super) fn state_mut"));
    assert_eq!(controller.matches("fn state_mut").count(), 1);
    assert!(store.contains("self.controller.state_mut()"));
    assert!(
        offenders.is_empty(),
        "mutable controller access must remain inside AppStore: {offenders:?}"
    );
}

#[test]
fn production_frontends_route_shared_mutations_through_runtime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("src/ui.rs")];
    collect_rust_files(&root.join("src/ui"), &mut files);
    let mut offenders = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("frontend source");
        let mut production = production_source(&source).to_string();
        // The egui test fixture can edit a store; it is not compiled in production.
        if path.ends_with("src/ui/egui/app.rs") {
            let fixture = "#[cfg(test)]\n    pub(crate) fn controller_mut(&mut self) -> &mut AppStore {\n        self.core.store_mut()\n    }";
            assert!(production.contains(fixture));
            production = production.replace(fixture, "");
        }
        for (line, text) in production.lines().enumerate() {
            if text.contains(".store_mut()")
                || text.contains("AppStore::new(")
                || text.contains("PlaybackRuntime::new(")
                || text.contains(".store().dispatch(")
                || text.contains(".store().handle_playback_event(")
                || text.contains(".store().tick_playback_position(")
            {
                offenders.push(format!(
                    "{}:{}",
                    path.strip_prefix(root).unwrap().display(),
                    line + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "frontend shared mutations must pass through FrontendRuntime: {offenders:?}"
    );
    let gtk = production_source(include_str!("../src/ui.rs"));
    assert!(gtk.contains("core: FrontendRuntime"));
    assert!(!gtk.contains("playback_backend: Option"));
    assert!(!gtk.contains("duration_indexer: DurationIndexer"));
    let egui = production_source(include_str!("../src/ui/egui/app.rs"));
    assert!(!egui.contains("duration_indexer: DurationIndexer"));
    assert!(!egui.contains("DurationIndexer::new("));
    assert!(egui.contains("FrontendRuntime::new_with_duration_wakeup("));
    let runtime = production_source(include_str!("../src/app/runtime.rs"));
    let fields = runtime
        .split_once("pub struct FrontendRuntime {")
        .expect("runtime declaration")
        .1
        .split_once('}')
        .expect("runtime fields")
        .0;
    assert_eq!(
        fields.matches("duration_indexer: DurationIndexer").count(),
        1
    );
}

#[test]
fn production_frontends_have_no_precomputed_dispatch_or_effect_bridges() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("src/ui.rs"), root.join("src/app/runtime.rs")];
    collect_rust_files(&root.join("src/ui"), &mut files);
    let forbidden = [
        "RuntimeEvent::DispatchResult",
        "RuntimeEvent::Effect(",
        "fn process_dispatch_result(",
        ".dispatch_command(",
        "fn dispatch_command(",
        "fn dispatch_playlist_eof(",
        "fn apply_store_effect(",
    ];
    let mut offenders = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("frontend/runtime source");
        for (line_index, line) in production_source(&source).lines().enumerate() {
            if forbidden.iter().any(|bridge| line.contains(bridge)) {
                offenders.push(format!(
                    "{}:{}",
                    path.strip_prefix(root).unwrap_or(&path).display(),
                    line_index + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "precomputed result/effect bridge: {offenders:?}"
    );
    let runtime = production_source(include_str!("../src/app/runtime.rs"));
    assert!(runtime.contains("PlaybackStartPrepared(PendingPlaybackStart)"));
    assert!(runtime.contains("PlaybackStartCancelled(PendingPlaybackStart)"));
}

#[test]
fn production_frontends_do_not_access_playback_runtime_or_backend_internals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("src/ui.rs")];
    collect_rust_files(&root.join("src/ui"), &mut files);
    let mut offenders = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("frontend source");
        for (line, text) in production_source(&source).lines().enumerate() {
            if [
                ".playback_mut(",
                ".playback(",
                "PlaybackRuntime",
                ".backend.as_ref()",
                ".backend.as_mut()",
            ]
            .iter()
            .any(|forbidden| text.contains(forbidden))
            {
                offenders.push(format!(
                    "{}:{}",
                    path.strip_prefix(root).unwrap().display(),
                    line + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "frontend backend internals must remain runtime-owned: {offenders:?}"
    );
    let runtime = include_str!("../src/app/runtime.rs");
    assert!(!runtime.contains("fn playback_mut("));
    assert!(!runtime.contains("fn playback(&self) -> &PlaybackRuntime"));
}

#[test]
fn socket_application_commands_do_not_present_auxiliary_dialogs() {
    let gtk = production_source(include_str!("../src/ui.rs"));
    let app_handler = gtk
        .split_once("fn apply_socket_app_command_gtk(")
        .expect("GTK socket app handler")
        .1
        .split_once("fn apply_socket_ui_command_gtk(")
        .expect("GTK socket UI handler follows app handler")
        .0;

    assert!(!app_handler.contains("set_window_visible_gtk"));
    assert!(!app_handler.contains(".present()"));
    assert!(app_handler.contains("sync_panel_windows"));
}

#[test]
fn playlist_queue_is_domain_owned() {
    let gtk_frontend = production_source(include_str!("../src/ui.rs"));
    let playlist = include_str!("../src/playlist.rs");

    assert!(!gtk_frontend.contains("playlist_queue:"));
    assert!(playlist.contains("queue: Vec<PlaylistEntryId>"));
    assert!(playlist.contains("pub fn queued_indices"));
}
