use super::*;

fn source(track: &str, revision: u64) -> ArtworkSource {
    ArtworkSource {
        track_revision: revision,
        normalized_track_identity: track.into(),
        skin_revision: 1,
        policy_revision: 1,
    }
}

fn key(track: &str) -> ArtworkKey {
    ArtworkKey {
        normalized_track_identity: track.into(),
        palette: [[0; 3]; 24],
        size: NOTIFICATION_ARTWORK_SIZE,
        renderer_version: ARTWORK_RENDERER_VERSION,
    }
}

fn started(queue: &mut LatestArtworkQueue, track: &str) -> ArtworkRequest {
    match queue.request(source(track, 1), key(track)).unwrap() {
        Schedule::Start(request) => request,
        other => panic!("expected running request, got {other:?}"),
    }
}

#[test]
fn normalizes_local_paths_and_percent_encoded_unicode_file_uris() {
    let expected = "/music/space and 音.wav";
    assert_eq!(
        normalize_track_identity("/music/./tmp/../space and 音.wav", None),
        expected
    );
    assert_eq!(
        normalize_track_identity("file:///music/space%20and%20%E9%9F%B3.wav", None),
        expected
    );
    assert_eq!(
        normalize_track_identity("file://localhost/music/space%20and%20%E9%9F%B3.wav", None),
        expected
    );
    assert_eq!(
        normalize_track_identity("/music/percent%20literal.wav", None),
        "/music/percent%20literal.wav"
    );
}

#[test]
fn preserves_nonlocal_and_malformed_uris_as_opaque_identities() {
    for uri in [
        "content://provider/a%20b",
        "https://example/a/../b",
        "file://server/music/a.wav",
        "file:///music/%GG.wav",
        "file:///music/%FF.wav",
        "file:///music/%00.wav",
        "file:///music/a?query=1",
        "file:///music/a#fragment",
    ] {
        assert_eq!(normalize_track_identity(uri, None), uri);
    }
}

#[test]
fn lexical_normalization_preserves_relative_parent_semantics() {
    assert_eq!(normalize_track_identity("../../a/../b", None), "../../b");
    assert_eq!(normalize_track_identity("/../../a//b/", None), "/a/b");
    assert_eq!(normalize_track_identity("./", None), ".");
}

#[test]
fn android_alias_folding_requires_the_actual_app_files_directory() {
    let old = "/data/data/org.xmms.renascene/files/imports/a.wav";
    let canonical = "/data/user/0/org.xmms.renascene/files/imports/a.wav";
    for root in [
        "/data/user/0/org.xmms.renascene/files",
        "/data/data/org.xmms.renascene/files",
    ] {
        assert_eq!(
            normalize_track_identity(old, Some(Path::new(root))),
            canonical
        );
        assert_eq!(
            normalize_track_identity(canonical, Some(Path::new(root))),
            canonical
        );
        for foreign in [
            "/data/data/org.xmms.renascene.extra/files/a.wav",
            "/data/user/10/org.xmms.renascene/files/a.wav",
            "/data/data/other/files/a.wav",
        ] {
            assert_eq!(
                normalize_track_identity(foreign, Some(Path::new(root))),
                foreign
            );
        }
    }
    for root in [
        None,
        Some(Path::new("/tmp/files")),
        Some(Path::new("/data/user/0/org.xmms.renascene/cache")),
    ] {
        assert_eq!(normalize_track_identity(old, root), old);
    }
}

#[test]
fn seed_is_stable_versioned_length_prefixed_and_sensitive_to_full_identity() {
    // Computed independently from the documented FNV-1a serialization.
    assert_eq!(stable_track_seed("/music/a.wav"), 0x218c74f4dcf588c7);
    assert_ne!(
        stable_track_seed("/music/a.wav"),
        stable_track_seed("/other/a.wav")
    );
    assert_ne!(stable_track_seed("a\0b"), stable_track_seed("ab"));
    let a = normalize_track_identity("file:///music/a.wav", None);
    assert_eq!(stable_track_seed(&a), stable_track_seed("/music/a.wav"));
}

#[test]
fn render_key_keeps_full_palette_track_size_and_version() {
    let a = key("/music/a.wav");
    assert_eq!(a, key("/music/a.wav")); // repeated playlist entry
    assert_ne!(a, key("/other/a.wav")); // same title, different file
    let mut b = a.clone();
    b.palette[23][2] = 1;
    assert_ne!(a, b);
    assert_eq!(a.spec().stable_track_seed, b.spec().stable_track_seed);
    b = a.clone();
    b.size = 512;
    assert_ne!(a, b);
    b = a.clone();
    b.renderer_version += 1;
    assert_ne!(a, b);
}

#[test]
fn spec_rejects_unsupported_or_overflowing_dimensions_and_versions() {
    let mut spec = key("a").spec();
    assert_eq!(spec.pixel_count(), Ok(320 * 320));
    spec.width = 512;
    spec.height = 512;
    assert_eq!(spec.pixel_count(), Ok(512 * 512));
    for (w, h) in [(0, 0), (1, 1), (320, 512), (u32::MAX, u32::MAX)] {
        spec.width = w;
        spec.height = h;
        assert_eq!(spec.pixel_count(), Err(ArtworkError::UnsupportedDimensions));
    }
    spec.width = 320;
    spec.height = 320;
    spec.renderer_version += 1;
    assert_eq!(spec.pixel_count(), Err(ArtworkError::UnsupportedRenderer));
}

#[test]
fn unchanged_request_is_deduplicated_before_and_after_completion() {
    let mut queue = LatestArtworkQueue::new(7);
    let a = started(&mut queue, "a");
    assert_eq!(a.identity.service_epoch, 7);
    assert_eq!(
        queue.request(source("a", 1), key("a")).unwrap(),
        Schedule::Unchanged
    );
    assert_eq!(
        queue.finish(&a, &source("a", 1), true),
        Completion {
            publish: true,
            next: None
        }
    );
    assert_eq!(
        queue.request(source("a", 1), key("a")).unwrap(),
        Schedule::Unchanged
    );
}

#[test]
fn latest_pending_replaces_backlog_and_only_latest_completion_publishes() {
    let mut queue = LatestArtworkQueue::new(1);
    let first = started(&mut queue, "a");
    for n in 0..100 {
        let track = format!("pending-{n}");
        assert!(matches!(
            queue.request(source(&track, 1), key(&track)).unwrap(),
            Schedule::Pending(_)
        ));
        assert_eq!(queue.running(), Some(&first));
        assert_eq!(
            queue.pending().unwrap().key.normalized_track_identity,
            track
        );
    }
    let last = queue.pending().unwrap().clone();
    let done = queue.finish(&first, &last.identity.source, true);
    assert!(!done.publish);
    assert_eq!(done.next, Some(last.clone()));
    assert!(queue.pending().is_none());
    assert!(queue.finish(&last, &last.identity.source, true).publish);
    assert!(queue.running().is_none());
}

#[test]
fn foreign_or_out_of_order_completions_cannot_release_the_real_worker() {
    let mut queue = LatestArtworkQueue::new(1);
    let first = started(&mut queue, "a");
    queue.request(source("b", 1), key("b")).unwrap();
    let pending = queue.pending().unwrap().clone();
    assert_eq!(
        queue.finish(&pending, &pending.identity.source, true),
        Completion {
            publish: false,
            next: None
        }
    );
    assert_eq!(queue.running(), Some(&first));
    let mut old_epoch = first.clone();
    old_epoch.identity.service_epoch = 0;
    assert!(
        !queue
            .finish(&old_epoch, &first.identity.source, true)
            .publish
    );
    assert_eq!(queue.running(), Some(&first));
}

#[test]
fn playlist_reorder_changes_request_revision_but_not_render_key() {
    let mut queue = LatestArtworkQueue::new(1);
    let first = started(&mut queue, "a");
    let next = match queue.request(source("a", 2), key("a")).unwrap() {
        Schedule::Pending(next) => next,
        other => panic!("{other:?}"),
    };
    assert_eq!(first.key, next.key);
    assert_ne!(first.identity, next.identity);
    assert!(!queue.finish(&first, &next.identity.source, true).publish);
    assert!(queue.finish(&next, &next.identity.source, true).publish);
}

#[test]
fn settings_revision_without_new_render_key_still_rejects_old_publication() {
    for skin_change in [true, false] {
        let mut queue = LatestArtworkQueue::new(1);
        let a = started(&mut queue, "a");
        let mut current = source("a", 1);
        if skin_change {
            current.skin_revision += 1;
        } else {
            current.policy_revision += 1;
        }
        assert!(!queue.finish(&a, &current, true).publish);
        assert!(matches!(
            queue.request(current.clone(), key("a")).unwrap(),
            Schedule::Start(_)
        ));
    }
}

#[test]
fn cancel_retains_running_accounting_and_allows_new_pending_request() {
    let mut queue = LatestArtworkQueue::new(1);
    let a = started(&mut queue, "a");
    queue.request(source("b", 1), key("b")).unwrap();
    queue.cancel();
    assert_eq!(queue.running(), Some(&a));
    assert!(queue.pending().is_none());
    assert!(matches!(
        queue.request(source("c", 1), key("c")).unwrap(),
        Schedule::Pending(_)
    ));
    let done = queue.finish(&a, &source("c", 1), true);
    assert!(!done.publish);
    assert_eq!(done.next.unwrap().key.normalized_track_identity, "c");
}

#[test]
fn stop_and_service_replacement_cannot_publish_old_results() {
    let mut old = LatestArtworkQueue::new(1);
    let a = started(&mut old, "a");
    old.stop();
    assert_eq!(
        old.request(source("a", 1), key("a")),
        Err(ArtworkError::Stopped)
    );
    assert_eq!(
        old.finish(&a, &a.identity.source, true),
        Completion {
            publish: false,
            next: None
        }
    );
    let mut replacement = LatestArtworkQueue::new(2);
    let b = started(&mut replacement, "a");
    assert!(!replacement.finish(&a, &a.identity.source, true).publish);
    assert_eq!(replacement.running(), Some(&b));
    assert!(replacement.finish(&b, &b.identity.source, true).publish);
}

#[test]
fn failed_job_can_be_explicitly_retried_without_automatic_retry_loop() {
    let mut queue = LatestArtworkQueue::new(1);
    let a = started(&mut queue, "a");
    let done = queue.finish(&a, &a.identity.source, false);
    assert!(!done.publish);
    assert!(done.next.is_none());
    let retry = started(&mut queue, "a");
    assert!(retry.identity.request_generation > a.identity.request_generation);
    assert!(queue.finish(&retry, &retry.identity.source, true).publish);
}

#[test]
fn key_track_mismatch_is_rejected_without_mutating_scheduler() {
    let mut queue = LatestArtworkQueue::new(1);
    assert_eq!(
        queue.request(source("a", 1), key("b")),
        Err(ArtworkError::IdentityMismatch)
    );
    assert!(queue.running().is_none());
}

#[test]
fn delayed_foreground_projection_requires_current_activity_and_media_revision() {
    let current = ProjectionStamp {
        activity_generation: 2,
        media_revision: 10,
    };
    assert!(current.is_current(current));
    assert!(!ProjectionStamp {
        activity_generation: 1,
        media_revision: 10
    }
    .is_current(current));
    assert!(!ProjectionStamp {
        activity_generation: 2,
        media_revision: 9
    }
    .is_current(current));
    assert!(!ProjectionStamp {
        activity_generation: 2,
        media_revision: 11
    }
    .is_current(current));
}
