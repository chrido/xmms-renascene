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
fn playlist_queue_is_domain_owned() {
    let gtk_frontend = production_source(include_str!("../src/ui.rs"));
    let playlist = include_str!("../src/playlist.rs");

    assert!(!gtk_frontend.contains("playlist_queue:"));
    assert!(playlist.contains("queue: Vec<PlaylistEntryId>"));
    assert!(playlist.contains("pub fn queued_indices"));
}
