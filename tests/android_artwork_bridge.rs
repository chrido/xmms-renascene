//! ABI/packaging guardrails alongside real emulator worker tests. Source checks
//! alone do not prove JNI behavior or main-thread/lifecycle safety.

#[test]
fn worker_native_declarations_have_matching_static_jni_exports() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsArtworkWorker.java");
    let rust = include_str!("../src/ui/egui/android/jni.rs");
    for name in [
        "nativeSnapshot",
        "nativeSelect",
        "nativeCancel",
        "nativeResolveSettings",
        "nativeRender",
        "nativeRenderPng",
    ] {
        assert!(java.contains(&format!("{name}(")));
        assert!(rust.contains(&format!(
            "fn Java_org_xmms_renascene_XmmsArtworkWorker_{name}("
        )));
    }
    assert!(java.contains("private static native int nativeResolveSettings(String filesDir, long epoch, long generation)"));
    assert!(java.contains("private static native int[] nativeRender(String filesDir, String snapshot, long epoch, long generation, long[] timings, int outputSize)"));
    assert!(rust.contains("argb_pixels_to_jint_array(&mut env, &pixels)"));
    assert!(rust.contains("jint::try_from(pixels.len())"));
}

#[test]
fn snapshot_is_memory_only_and_blocking_jni_endpoints_reject_main_looper() {
    let rust = include_str!("../src/ui/egui/android/artwork.rs");
    let snapshot = rust
        .split("pub(crate) fn snapshot(")
        .nth(1)
        .unwrap()
        .split("pub(crate) fn resolve_browse_on_worker")
        .next()
        .unwrap();
    assert!(!snapshot.contains("resolve_artwork_settings("));
    assert!(!snapshot.contains("fs::"));
    assert!(snapshot.contains("artwork_media_identity()"));
    let jni = include_str!("../src/ui/egui/android/jni.rs");
    for name in ["nativeResolveSettings", "nativeRender", "nativeRenderPng"] {
        let body = jni
            .split(&format!(
                "fn Java_org_xmms_renascene_XmmsArtworkWorker_{name}("
            ))
            .nth(1)
            .unwrap()
            .split("#[unsafe(no_mangle)]")
            .next()
            .unwrap();
        assert!(body.contains("if !artwork_worker_thread(&mut env)"));
    }
    for name in ["nativePrepare", "nativeRender"] {
        let body = jni
            .split(&format!(
                "fn Java_org_xmms_renascene_XmmsBrowseArtwork_{name}("
            ))
            .nth(1)
            .unwrap()
            .split("#[unsafe(no_mangle)]")
            .next()
            .unwrap();
        assert!(body.contains("if !artwork_worker_thread(&mut env)"));
    }
    assert!(jni.contains("\"myLooper\""));
    assert!(jni.contains("\"getMainLooper\""));
    assert!(!rust.contains("use jni::objects::GlobalRef"));
    assert!(!rust.contains("new_global_ref("));
}

#[test]
fn worker_limits_pending_jobs_results_cache_and_cross_epoch_execution() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsArtworkWorker.java");
    assert!(java.contains("new ArrayBlockingQueue<Runnable>(1)"));
    assert_eq!(java.matches("executor.execute(").count(), 1);
    assert!(java.contains("pending = job"));
    assert!(java.contains("postedCompletion = new CompletionCallback"));
    assert!(java.contains("clearPostedLocked()"));
    assert!(java.contains("cache.size() > 2"));
    assert!(java.contains("new Semaphore(1, true)"));
    assert!(java.contains("if (acquired) PROCESS_LEASE.release()"));
    assert!(java.contains("failures < 3"));
    assert!(java.contains("executor.shutdownNow()"));
    assert!(java.contains("listener = null"));
    assert!(!java.contains("Bitmap.recycle"));
}

#[test]
fn service_closes_worker_on_stop_and_destroy_and_debug_actions_are_gated() {
    let service = include_str!("../android/java/org/xmms/renascene/XmmsPlaybackService.java");
    let destroy = service
        .split("public void onDestroy()")
        .nth(1)
        .unwrap()
        .split("public BrowserRoot")
        .next()
        .unwrap();
    assert!(destroy.contains("closeArtworkWorker()"));
    let stop = service
        .split("private void stopPlaybackService()")
        .nth(1)
        .unwrap();
    assert!(stop.contains("closeArtworkWorker()"));
    assert!(service.contains("artworkDebuggable && \"xmms-debug-artwork-worker\".equals(action)"));
    assert!(service.contains("if (!artworkDebuggable) return"));
    assert!(include_str!("../scripts/repo.py").contains(
        "<meta-data android:name=\"androidx.car.app.launchable\" android:value=\"true\" />"
    ));
    let probe = include_str!("../android/java/org/xmms/renascene/XmmsAutoProbeActivity.java");
    assert!(probe.contains("ApplicationInfo.FLAG_DEBUGGABLE) == 0"));
    let packaging = include_str!("../scripts/repo.py");
    assert!(packaging.contains("java_source_dir.glob(\"*.java\")"));
    assert!(packaging.contains("if not release"));
}

#[test]
fn both_media_publication_paths_carry_validated_projection_tokens() {
    let rust = include_str!("../src/ui/egui/android/media_session.rs");
    assert_eq!(
        rust.matches("(ILjava/lang/String;IIIJJJIZZLjava/lang/String;)V")
            .count(),
        2
    );
    assert!(rust.contains("publish_projection(notification.clone())"));
    assert!(rust.contains("publish_projection(notification)"));
    let activity = include_str!("../android/java/org/xmms/renascene/XmmsActivity.java");
    assert!(activity.contains("if (!nativeValidateMediaProjection(projectionToken)) return"));
    assert!(activity.contains("EXTRA_PROJECTION_TOKEN, projectionToken"));
    let service = include_str!("../android/java/org/xmms/renascene/XmmsPlaybackService.java");
    assert!(service.contains("if (!nativeValidateProjection(projectionToken, true))"));
    assert!(service.contains("playbackHandler.post(refreshProjection)"));
}

#[test]
fn artwork_only_publication_has_no_audio_focus_or_foreground_restart() {
    let service = include_str!("../android/java/org/xmms/renascene/XmmsPlaybackService.java");
    let art_only = service
        .split("private void publishArtworkOnly()")
        .nth(1)
        .unwrap()
        .split("private void publishNotification")
        .next()
        .unwrap();
    assert!(art_only.contains("!foregroundActive"));
    assert!(art_only.contains("publishMetadata()"));
    assert!(!art_only.contains("requestPlaybackAudioFocus"));
    assert!(!art_only.contains("startForeground("));
    let metadata = service
        .split("private void publishMetadata()")
        .nth(1)
        .unwrap()
        .split("private void updatePlaybackState")
        .next()
        .unwrap();
    assert!(metadata.contains("if (fields.equals(lastMetadata)) return"));
    assert_eq!(metadata.matches("putBitmap(").count(), 1);
    assert!(metadata.contains("METADATA_KEY_ALBUM_ART"));
    assert!(service.contains(".setLargeIcon(readyArtwork)"));
    assert!(service.contains("else notificationManager.notify(NOTIFICATION_ID, notification)"));
    let activity = include_str!("../android/java/org/xmms/renascene/XmmsActivity.java");
    let invalidate = activity
        .split("public void refreshPlaybackArtwork()")
        .nth(1)
        .unwrap()
        .split("private void registerMediaVolumeObserver")
        .next()
        .unwrap();
    assert!(invalidate.contains("application.sendBroadcast"));
    assert!(!invalidate.contains("startService"));
}

#[test]
fn artwork_probe_waits_for_current_generation_and_metadata_publication() {
    let worker = include_str!("../android/java/org/xmms/renascene/XmmsArtworkWorker.java");
    let request = worker
        .split("private void requestInternal(")
        .nth(1)
        .unwrap()
        .split("private boolean cancelled(")
        .next()
        .unwrap();
    let pending = request.find("lastStatus = \"pending\"").unwrap();
    assert!(request.find("completedNs = 0").unwrap() < pending);
    assert!(pending < request.find("Job job = new Job(").unwrap());
    let probe = include_str!("../android/java/org/xmms/renascene/XmmsAutoProbeActivity.java");
    assert!(probe.contains("artworkPublicationReady(value)"));
    assert!(probe.contains("stats.optLong(\"completedNs\") >= stats.optLong(\"requestNs\", 1)"));
    assert!(probe.contains("stats.optBoolean(\"hasMetadataArt\")"));
    assert!(probe.contains("waitingForPublication"));
    let service = include_str!("../android/java/org/xmms/renascene/XmmsPlaybackService.java");
    assert!(service.contains(".put(\"playbackState\", playbackState)"));
}

#[test]
fn timing_instrumentation_uses_durations_and_opt_in_bounded_logs() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsArtworkWorker.java");
    assert!(java.contains("long[] timings"));
    assert!(java.contains("traceEnabled && traceRecords.getAndIncrement() < 256"));
    assert!(java.contains("SystemClock.elapsedRealtimeNanos()"));
    assert!(java.contains("Math.min(2000, coldDelayMillis)"));
    let jni = include_str!("../src/ui/egui/android/jni.rs");
    assert!(jni.contains("timings: JLongArray"));
    assert!(jni.contains("started.elapsed().as_nanos()"));
    assert!(jni.contains("set_long_array_region(&timings"));
    let probe = include_str!("../android/java/org/xmms/renascene/XmmsAutoProbeActivity.java");
    assert!(probe.contains("requestToClientReadyNs"));
    assert!(probe.contains("callbackToInspectNs"));
    assert!(probe.contains("exportStarted"));
}
