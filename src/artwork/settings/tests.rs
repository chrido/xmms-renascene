use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Barrier};

use crate::artwork::{ArtworkKey, ARTWORK_RENDERER_VERSION};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "xmms-artwork-settings-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn resolved(selector: Option<&str>, palette: ArtworkPalette) -> ResolvedArtworkSettings {
    ResolvedArtworkSettings {
        selector: selector.map(str::to_owned),
        palette,
        policy: ArtworkPolicy::Generated,
        diagnostic: None,
    }
}

#[test]
fn foreground_updates_are_immutable_deduplicated_and_memory_only() {
    let store = ArtworkSettingsStore::default();
    let empty = store.snapshot();
    // Deliberately nonexistent selector: this operation must not load it.
    let first = store
        .publish_foreground(
            Some("/nonexistent/skin".into()),
            ArtworkPolicy::Generated,
            DEFAULT_VIS_COLORS,
        )
        .unwrap();
    assert!(first.changed);
    assert_eq!(
        (
            first.snapshot.revision,
            first.snapshot.skin_revision,
            first.snapshot.policy_revision
        ),
        (1, 1, 1)
    );
    assert!(empty.palette.is_none()); // previous immutable snapshot stays intact
    let same = store
        .publish_foreground(
            Some("/nonexistent/skin".into()),
            ArtworkPolicy::Generated,
            DEFAULT_VIS_COLORS,
        )
        .unwrap();
    assert!(!same.changed);
    assert!(Arc::ptr_eq(&first.snapshot, &same.snapshot));
    assert!(store.begin_cold_resolution().is_none());
}

#[test]
fn same_selector_palette_edits_and_policy_changes_advance_only_relevant_revisions() {
    let store = ArtworkSettingsStore::default();
    let first = store
        .publish_foreground(
            Some("same-skin".into()),
            ArtworkPolicy::Generated,
            DEFAULT_VIS_COLORS,
        )
        .unwrap()
        .snapshot;
    let mut palette = DEFAULT_VIS_COLORS;
    palette[23] = [12, 34, 56];
    let edited = store
        .publish_foreground(first.selector.clone(), first.policy, palette)
        .unwrap()
        .snapshot;
    assert_eq!(edited.skin_revision, first.skin_revision + 1);
    assert_eq!(edited.policy_revision, first.policy_revision);
    assert_eq!(edited.revision, first.revision + 1);
    assert_eq!(first.palette.unwrap(), DEFAULT_VIS_COLORS);
    let off = store
        .publish_foreground(edited.selector.clone(), ArtworkPolicy::Off, palette)
        .unwrap()
        .snapshot;
    assert_eq!(off.skin_revision, edited.skin_revision);
    assert_eq!(off.policy_revision, edited.policy_revision + 1);
}

#[test]
fn equivalent_palettes_with_different_selectors_reuse_render_key_but_invalidate_requests() {
    let store = ArtworkSettingsStore::default();
    let first = store
        .publish_foreground(
            Some("a".into()),
            ArtworkPolicy::Generated,
            DEFAULT_VIS_COLORS,
        )
        .unwrap()
        .snapshot;
    let second = store
        .publish_foreground(
            Some("b".into()),
            ArtworkPolicy::Generated,
            DEFAULT_VIS_COLORS,
        )
        .unwrap()
        .snapshot;
    assert!(second.revision > first.revision);
    let key = |snapshot: &ArtworkSettingsSnapshot| ArtworkKey {
        normalized_track_identity: "/track.wav".into(),
        palette: snapshot.palette.unwrap(),
        size: 320,
        renderer_version: ARTWORK_RENDERER_VERSION,
    };
    assert_eq!(key(&first), key(&second));
}

#[test]
fn cold_commit_is_one_shot_and_foreign_store_tokens_are_rejected() {
    let store = ArtworkSettingsStore::default();
    let foreign = ArtworkSettingsStore::default();
    let token = store.begin_cold_resolution().unwrap();
    assert!(foreign
        .commit_cold(&token, resolved(None, DEFAULT_VIS_COLORS))
        .unwrap()
        .is_none());
    let commit = store
        .commit_cold(&token, resolved(Some("saved"), DEFAULT_VIS_COLORS))
        .unwrap()
        .unwrap();
    assert_eq!(commit.snapshot.selector.as_deref(), Some("saved"));
    assert!(store
        .commit_cold(&token, resolved(Some("old"), [[0; 3]; 24]))
        .unwrap()
        .is_none());
    assert!(store.begin_cold_resolution().is_none());
}

#[test]
fn cold_resolution_runs_on_worker_and_preserves_saved_policy_skin_and_file_bytes() {
    let dir = TestDir::new();
    let skin = dir.0.join("skin");
    std::fs::create_dir(&skin).unwrap();
    std::fs::write(skin.join("viscolor.txt"), b"1,2,3\n").unwrap();
    let path = dir.0.join("config");
    let config = Config {
        skin: Some(skin.to_str().unwrap().into()),
        artwork_policy: ArtworkPolicy::Generated,
        ..Config::default()
    };
    config.save_to_file(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let store = ArtworkSettingsStore::default();
    let token = store.begin_cold_resolution().unwrap();
    let worker_path = path.clone();
    let (thread, result) = std::thread::spawn(move || {
        (
            std::thread::current().id(),
            resolve_artwork_settings(&worker_path),
        )
    })
    .join()
    .unwrap();
    assert_ne!(thread, std::thread::current().id());
    let committed = store.commit_cold(&token, result).unwrap().unwrap();
    assert_eq!(committed.snapshot.policy, ArtworkPolicy::Generated);
    assert_eq!(committed.snapshot.selector, config.skin);
    assert_eq!(committed.snapshot.palette.unwrap()[0], [1, 2, 3]);
    assert!(committed.diagnostic.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(store.begin_cold_resolution().is_none());
}

#[test]
fn delayed_worker_cannot_overwrite_newer_foreground_settings_or_report_stale_diagnostic() {
    let store = Arc::new(ArtworkSettingsStore::default());
    let token = store.begin_cold_resolution().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let (tx, rx) = mpsc::channel();
    let worker_store = Arc::clone(&store);
    let worker_barrier = Arc::clone(&barrier);
    let worker = std::thread::spawn(move || {
        let mut disk = resolved(Some("old-skin"), DEFAULT_VIS_COLORS);
        disk.diagnostic = Some(SettingsDiagnostic {
            source: SettingsDiagnosticSource::Palette,
            kind: io::ErrorKind::NotFound,
        });
        tx.send(()).unwrap();
        worker_barrier.wait();
        worker_store.commit_cold(&token, disk).unwrap()
    });
    rx.recv().unwrap();
    let current = store
        .publish_foreground(Some("new-skin".into()), ArtworkPolicy::Off, [[42; 3]; 24])
        .unwrap()
        .snapshot;
    barrier.wait();
    assert!(worker.join().unwrap().is_none());
    assert!(Arc::ptr_eq(&current, &store.snapshot()));
}

#[test]
fn palette_failure_keeps_selector_and_policy_and_emits_one_path_free_record() {
    let dir = TestDir::new();
    let path = dir.0.join("config");
    let selector = dir.0.join("missing-skin").to_str().unwrap().to_owned();
    Config {
        skin: Some(selector.clone()),
        artwork_policy: ArtworkPolicy::Generated,
        ..Config::default()
    }
    .save_to_file(&path)
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let loaded = resolve_artwork_settings(&path);
    assert_eq!(loaded.selector.as_deref(), Some(selector.as_str()));
    assert_eq!(loaded.policy, ArtworkPolicy::Generated);
    assert_eq!(loaded.palette, DEFAULT_VIS_COLORS);
    let store = ArtworkSettingsStore::default();
    let token = store.begin_cold_resolution().unwrap();
    let commit = store.commit_cold(&token, loaded.clone()).unwrap().unwrap();
    assert_eq!(
        commit.diagnostic,
        Some(SettingsDiagnostic {
            source: SettingsDiagnosticSource::Palette,
            kind: io::ErrorKind::NotFound
        })
    );
    assert!(!format!("{:?}", commit.diagnostic).contains(&selector));
    assert!(store.commit_cold(&token, loaded).unwrap().is_none());
    assert!(store.begin_cold_resolution().is_none());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn missing_config_defaults_to_auto_without_warning_and_invalid_configs_fail_closed() {
    let dir = TestDir::new();
    let path = dir.0.join("config");
    let missing = resolve_artwork_settings(&path);
    assert_eq!(missing.policy, ArtworkPolicy::Auto);
    assert_eq!(missing.palette, DEFAULT_VIS_COLORS);
    assert!(missing.diagnostic.is_none());
    for bytes in [
        vec![0xff],
        vec![b'x'; MAX_ARTWORK_CONFIG_BYTES as usize + 1],
    ] {
        std::fs::write(&path, &bytes).unwrap();
        let bad = resolve_artwork_settings(&path);
        assert_eq!(bad.policy, ArtworkPolicy::Off);
        assert_eq!(
            bad.diagnostic,
            Some(SettingsDiagnostic {
                source: SettingsDiagnosticSource::Configuration,
                kind: io::ErrorKind::InvalidData
            })
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    let directory = resolve_artwork_settings(&dir.0);
    assert_eq!(
        directory.diagnostic.unwrap().source,
        SettingsDiagnosticSource::Configuration
    );
}

#[test]
fn corrupt_palette_uses_fallback_without_rewriting_policy_or_selector() {
    let dir = TestDir::new();
    let skin = dir.0.join("skin");
    std::fs::create_dir(&skin).unwrap();
    std::fs::write(skin.join("viscolor.txt"), [0xff]).unwrap();
    let path = dir.0.join("config");
    let config = Config {
        skin: Some(skin.to_str().unwrap().into()),
        artwork_policy: ArtworkPolicy::Generated,
        ..Config::default()
    };
    config.save_to_file(&path).unwrap();
    let result = resolve_artwork_settings(&path);
    assert_eq!(result.selector, config.skin);
    assert_eq!(result.policy, config.artwork_policy);
    assert_eq!(result.palette, DEFAULT_VIS_COLORS);
    assert_eq!(
        result.diagnostic,
        Some(SettingsDiagnostic {
            source: SettingsDiagnosticSource::Palette,
            kind: io::ErrorKind::InvalidData
        })
    );
    assert_eq!(Config::load_from_file(&path).unwrap(), config);
}

#[cfg(unix)]
#[test]
fn nonregular_config_and_palette_sources_are_rejected_before_opening() {
    let dir = TestDir::new();
    let socket = dir.0.join("not-a-skin.wsz");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&socket)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        resolve_artwork_settings(&socket).diagnostic.unwrap().source,
        SettingsDiagnosticSource::Configuration
    );
}

#[test]
fn exact_config_byte_limit_is_accepted() {
    let dir = TestDir::new();
    let path = dir.0.join("config");
    let mut text = b"[xmms]\nartwork_policy=generated\n#".to_vec();
    text.resize(MAX_ARTWORK_CONFIG_BYTES as usize, b'#');
    std::fs::write(&path, text).unwrap();
    let result = resolve_artwork_settings(&path);
    assert_eq!(result.policy, ArtworkPolicy::Generated);
    assert!(result.diagnostic.is_none());
}

#[test]
fn revision_exhaustion_does_not_partially_mutate_settings() {
    for exhausted in ["all", "skin", "policy"] {
        let store = ArtworkSettingsStore::default();
        let mut initial = ArtworkSettingsSnapshot {
            palette: Some(DEFAULT_VIS_COLORS),
            ..ArtworkSettingsSnapshot::default()
        };
        match exhausted {
            "all" => initial.revision = u64::MAX,
            "skin" => initial.skin_revision = u64::MAX,
            _ => initial.policy_revision = u64::MAX,
        }
        *store.current.lock().unwrap() = Arc::new(initial);
        let before = store.snapshot();
        assert!(matches!(
            store.publish_foreground(
                Some("changed".into()),
                ArtworkPolicy::Generated,
                [[1; 3]; 24]
            ),
            Err(ArtworkError::GenerationExhausted)
        ));
        assert!(Arc::ptr_eq(&before, &store.snapshot()));
    }
}

#[test]
fn process_store_accessor_is_stable_without_mutating_global_state() {
    assert!(std::ptr::eq(
        process_artwork_settings(),
        process_artwork_settings()
    ));
}
