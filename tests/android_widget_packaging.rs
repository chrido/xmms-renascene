#[test]
fn skin_change_bridge_invalidates_cache_before_refreshing_widgets() {
    let rust = include_str!("../src/ui/egui/android_file_picker.rs");
    let refresh = rust
        .split("pub fn refresh_player_widgets()")
        .nth(1)
        .expect("Android widget refresh bridge")
        .split("fn android_context")
        .next()
        .expect("refresh bridge body");
    let invalidate = refresh
        .find("WIDGET_SKIN")
        .expect("widget skin cache invalidation");
    let java_call = refresh
        .find("\"refreshPlayerWidgets\"")
        .expect("activity widget refresh call");

    assert!(invalidate < java_call);
    assert!(refresh.contains("\"()V\""));
    assert!(refresh.contains("context.activity.clone()"));
    assert!(!refresh.contains("call_static_method"));
    assert!(!refresh.contains("org/xmms/renascene/XmmsPlayerWidget"));
    assert!(refresh.contains("exception_describe"));
    assert!(refresh.contains("exception_clear"));
}

#[test]
fn activity_widget_refresh_bridge_posts_to_main_looper() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsActivity.java");
    assert!(java.contains("new Handler(Looper.getMainLooper())"));

    let refresh = java
        .split("public void refreshPlayerWidgets()")
        .nth(1)
        .expect("activity widget refresh bridge")
        .split("public void openDocuments")
        .next()
        .expect("activity widget refresh body");
    assert!(refresh.contains("Context applicationContext = getApplicationContext()"));
    assert!(refresh.contains("MAIN_HANDLER.post(() -> {"));
    assert!(refresh.contains("XmmsPlayerWidget.refreshAll(applicationContext)"));
    assert!(refresh.contains("XmmsPlayerInfoWidget.refreshAll(applicationContext)"));
}

#[test]
fn android_output_volume_uses_stream_music_without_backend_scaling() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsActivity.java");
    let rust = include_str!("../src/ui/egui/android_file_picker.rs");
    let app = include_str!("../src/ui/egui/app.rs");
    let gtk = include_str!("../src/ui.rs");
    let controller = include_str!("../src/app/controller.rs");
    let store = include_str!("../src/app/store.rs");

    assert!(java.contains("setVolumeControlStream(AudioManager.STREAM_MUSIC)"));
    assert!(java.contains("public boolean setMediaVolumePercent(int volumePercent)"));
    assert!(java.contains("if (audioManager == null)"));
    assert!(java.contains("Math.max(0, Math.min(100, volumePercent))"));
    assert!(java.contains("getStreamMaxVolume(AudioManager.STREAM_MUSIC)"));
    assert!(java.contains("Math.round(maxVolume * (clampedPercent / 100.0))"));
    assert!(java.contains("setStreamVolume(AudioManager.STREAM_MUSIC, streamVolume, 0)"));
    assert!(java.contains("public int getMediaVolumePercent()"));
    assert!(java.contains("if (maxVolume <= 0)"));

    assert!(rust.contains("pub fn set_media_volume_percent(volume: i32) -> Result<(), String>"));
    assert!(rust.contains("\"setMediaVolumePercent\""));
    assert!(rust.contains("\"(I)Z\""));
    assert!(rust.contains("pub fn media_volume_percent() -> Result<i32, String>"));
    assert!(rust.contains("\"getMediaVolumePercent\""));
    assert!(app.contains("match super::android_file_picker::media_volume_percent()"));

    let output_effect = app
        .split("if let AppEffect::SetOutputVolume(volume) = &effect")
        .nth(1)
        .expect("Android output-volume effect")
        .split("let clear_visualization")
        .next()
        .expect("Android output-volume effect body");
    assert!(output_effect.contains("set_media_volume_percent(*volume)"));
    assert!(!output_effect.contains("backend.set_volume"));

    assert!(controller.contains("AppEffect::SetOutputVolume(self.state.player.volume())"));
    assert_eq!(
        gtk.matches("AppEffect::SetOutputVolume(volume) | AppEffect::SetBackendVolume(volume)")
            .count(),
        2
    );
    assert!(store.contains("AppEffect::SetBackendVolume(volume)"));
    assert!(store.contains("AppEffect::SetBackendVolume(restore_volume)"));
}

#[test]
fn android_external_media_volume_is_observed_coalesced_and_not_echoed() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsActivity.java");
    let bridge = include_str!("../src/ui/egui/android_file_picker.rs");
    let app = include_str!("../src/ui/egui/app.rs");
    let store = include_str!("../src/app/store.rs");

    assert!(java.contains("new ContentObserver(MAIN_HANDLER)"));
    assert!(java.contains("Settings.System.CONTENT_URI, true, mediaVolumeObserver"));
    assert!(java.contains("registerMediaVolumeObserver();"));
    assert!(java.contains("unregisterMediaVolumeObserver();"));
    assert!(java.contains("if (mediaVolumeObserverRegistered)"));
    assert!(java.contains("if (!mediaVolumeObserverRegistered)"));
    assert!(java.contains("Looper.myLooper() != Looper.getMainLooper()"));
    assert!(java.contains("private native void nativeOnMediaVolumeChanged(int volumePercent)"));
    assert!(java.contains("volumePercent == lastReportedMediaVolumePercent"));
    assert!(java.contains("pendingAppMediaVolumePercent = clampedPercent"));
    assert!(java.contains("requestedPercent == volumePercent"));

    assert!(bridge.contains("static EXTERNAL_MEDIA_VOLUME_PERCENT: OnceLock<Mutex<Option<i32>>>"));
    assert!(bridge.contains("Java_org_xmms_renascene_XmmsActivity_nativeOnMediaVolumeChanged"));
    assert!(bridge.contains("Some(volume_percent.clamp(0, 100))"));
    assert!(bridge.contains("pub fn take_latest_external_media_volume_percent()"));
    assert!(bridge.contains(".take()"));
    assert!(bridge.contains("request_registered_repaint();"));
    let initialization = bridge
        .split("pub fn initialize(")
        .nth(1)
        .expect("Android initialization")
        .split("pub fn persist_app_state")
        .next()
        .expect("Android initialization body");
    assert!(initialization.contains("EXTERNAL_MEDIA_VOLUME_PERCENT"));
    assert!(initialization.contains("Mutex::new(None)"));

    let external_poll = app
        .split("fn poll_external_android_media_volume")
        .nth(1)
        .expect("external media-volume poll")
        .split("fn poll_android_media_controls")
        .next()
        .expect("external media-volume poll body");
    assert!(external_poll.contains("take_latest_external_media_volume_percent"));
    assert!(external_poll.contains("sync_external_output_volume(volume)"));
    assert!(external_poll.contains("sync_frontend_state_from_store()"));
    assert!(external_poll.contains("persist_android_state()"));
    assert!(external_poll.contains("ctx.request_repaint()"));
    assert!(!external_poll.contains("AudioCommand::SetVolume"));
    assert!(!external_poll.contains("set_media_volume_percent"));

    let store_sync = store
        .split("pub fn sync_external_output_volume")
        .nth(1)
        .expect("external output-volume store method")
        .split("pub fn complete_stop_fade")
        .next()
        .expect("external output-volume store body");
    assert!(store_sync.contains("state.player.set_volume(volume)"));
    assert!(store_sync.contains("state.config.volume = volume"));
    assert!(store_sync
        .contains("StateChangeSet::PLAYER | StateChangeSet::CONFIG | StateChangeSet::RENDER_MAIN"));
    assert!(!store_sync.contains("SetOutputVolume"));
    assert!(!store_sync.contains("SetBackendVolume"));
}

#[test]
fn android_activity_handles_bevy_configuration_change_set() {
    let cargo = include_str!("../Cargo.toml");
    let packaging = include_str!("../scripts/repo.py");
    let changes = "layoutDirection|locale|orientation|keyboardHidden|screenSize|smallestScreenSize|density|keyboard|navigation|screenLayout|uiMode";

    assert!(cargo.contains(&format!("config_changes = \"{changes}\"")));
    assert!(packaging.contains(&format!("android:configChanges=\"{changes}\"")));
}

#[test]
fn android_winit_patch_uses_the_reproducible_git_fork() {
    let cargo = include_str!("../Cargo.toml");
    let lock = include_str!("../Cargo.lock");
    let fork = "https://github.com/chrido/winit";
    let branch = "fix-window-configchanged-android";

    assert!(cargo
        .contains("winit = { version = \"0.30.13\", optional = true, default-features = false"));
    assert!(cargo.contains("\"android-native-activity\""));
    assert!(cargo.contains(&format!(
        "winit = {{ git = \"{fork}\", branch = \"{branch}\" }}"
    )));
    assert!(!cargo.contains("vendor/winit"));

    let winit_packages: Vec<_> = lock
        .split("[[package]]")
        .filter(|package| package.contains("name = \"winit\""))
        .collect();
    assert_eq!(winit_packages.len(), 1);
    assert!(winit_packages[0].contains("version = \"0.30.13\""));
    assert!(winit_packages[0].contains(&format!("source = \"git+{fork}?branch={branch}#")));
}

#[test]
fn activity_exposes_atomic_window_geometry_and_insets_for_rotation_layout() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsActivity.java");
    let rust = include_str!("../src/ui/egui/android_file_picker.rs");

    assert!(java.contains("private volatile SafeInsetSnapshot safeInsetSnapshot"));
    assert!(java.contains("public long[] windowLayoutSnapshot()"));
    assert!(java.contains("getCurrentWindowMetrics().getBounds()"));
    assert!(java.contains("public void onConfigurationChanged(Configuration newConfig)"));
    assert!(java.contains("configGeneration++;"));
    assert!(java.contains("insets.configGeneration == configGeneration"));
    assert!(java.contains("view.getWidth() != width || view.getHeight() != height"));
    assert!(java.contains("measuredInsets = metrics.getWindowInsets()"));
    assert!(java.contains("fresh ? 1 : 0"));
    assert!(java.contains("nativeRequestRepaint();"));
    assert!(java.contains("LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS"));
    assert!(rust.contains("pub fn window_layout_snapshot_pixels()"));
    assert!(rust.contains("nativeRequestRepaint"));
}

#[test]
fn player_info_widget_is_packaged_and_opens_player() {
    let provider = include_str!("../android/java/org/xmms/renascene/XmmsPlayerInfoWidget.java");
    assert!(provider.contains("INFO_WIDTH = 164"));
    assert!(provider.contains("INFO_HEIGHT = 37"));
    assert!(provider.contains("FRAME_WIDTH = INFO_WIDTH + 4"));
    assert!(provider.contains("FRAME_HEIGHT = INFO_HEIGHT + 4"));
    assert!(provider.contains("OPEN_PLAYER_REQUEST_CODE = 1000"));
    assert!(provider.contains("new Intent(context, XmmsActivity.class)"));
    assert!(provider.contains("PendingIntent.getActivity("));
    assert!(provider.contains("OPEN_PLAYER_REQUEST_CODE,"));
    assert!(provider.contains("PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE"));
    assert!(provider.contains("views.setOnClickPendingIntent(open"));
    assert!(provider.contains("widget_player_info_content"));
    assert!(provider.contains("widgetIds.length == 0"));
    assert!(provider.contains("nativeRenderPlayerInfoWidget("));
    assert!(provider.contains("XmmsWidgetSupport.proportionalPadding("));
    assert!(provider.contains("onAppWidgetOptionsChanged("));
    assert!(provider.contains("manager.getAppWidgetOptions(widgetId)"));
    assert!(provider.contains("manager.updateAppWidget(widgetId, views)"));
    assert!(!provider.contains("manager.updateAppWidget(widgetIds, views)"));

    let layout = include_str!("../android/res/layout/widget_player_info.xml");
    assert!(layout.contains("android:id=\"@android:id/background\""));
    assert!(layout.contains("@+id/widget_player_info_image"));
    assert!(layout.contains("@+id/widget_player_info_open"));

    let info = include_str!("../android/res/xml/player_info_widget_info.xml");
    assert!(info.contains("@string/info_widget_description"));
    assert!(info.contains("android:initialLayout=\"@layout/widget_player_info\""));
    assert!(info.contains("android:previewImage=\"@drawable/widget_player_info_preview\""));
    assert!(info.contains("android:previewLayout=\"@layout/widget_player_info_preview\""));
    assert!(info.contains("android:minWidth=\"168dp\""));
    assert!(info.contains("android:minResizeWidth=\"168dp\""));
    assert!(info.contains("android:minHeight=\"41dp\""));
    assert!(info.contains("android:minResizeHeight=\"41dp\""));
    assert!(info.contains("android:resizeMode=\"horizontal|vertical\""));

    let packaging = include_str!("../scripts/repo.py");
    assert!(packaging.contains("android:name=\".XmmsPlayerWidget\""));
    assert!(packaging.contains("android:icon=\"@drawable/widget_icon\""));
    assert!(packaging.contains("android:label=\"@string/widget_label\""));
    assert!(packaging.contains("android:resource=\"@xml/player_widget_info\""));
    assert!(packaging.contains("android:name=\".XmmsPlayerInfoWidget\""));
    assert!(packaging.contains("android:label=\"@string/info_widget_label\""));
    assert!(packaging.contains("android:resource=\"@xml/player_info_widget_info\""));
}

#[test]
fn player_info_widget_opts_out_of_launcher_rounding_without_local_clipping() {
    let layout = include_str!("../android/res/layout/widget_player_info.xml");

    assert!(layout.contains("android:id=\"@android:id/background\""));
    assert!(layout.contains("@+id/widget_player_info_content"));
    assert_eq!(layout.matches("android:clipToOutline=\"true\"").count(), 1);
    assert_eq!(layout.matches("android:clipToOutline=\"false\"").count(), 1);
    assert_eq!(
        layout.matches("android:outlineProvider=\"none\"").count(),
        2
    );
    assert!(layout.contains("@+id/widget_player_info_open"));
    assert!(layout.contains("android:background=\"@android:color/black\""));
    assert!(layout.contains("android:padding=\"2dp\""));
    assert!(layout.contains("android:background=\"@android:color/transparent\""));
    assert!(!layout.contains("android:clickable=\"true\""));
    assert!(!layout.contains("android:focusable=\"true\""));

    let preview = include_str!("../android/res/layout/widget_player_info_preview.xml");
    assert!(preview.contains("android:id=\"@android:id/background\""));
    assert!(preview.contains("android:clipToOutline=\"true\""));
    assert!(preview.contains("android:outlineProvider=\"none\""));
    assert!(preview.contains("android:background=\"@android:color/black\""));
    assert!(preview.contains("android:scaleType=\"fitXY\""));
}

#[test]
fn widget_picker_metadata_has_visible_legacy_and_modern_previews() {
    let player_info = include_str!("../android/res/xml/player_widget_info.xml");
    assert!(player_info.contains("android:previewImage=\"@drawable/widget_player_preview\""));
    assert!(player_info.contains("android:previewLayout=\"@layout/widget_player_preview\""));

    let player_preview = include_str!("../android/res/layout/widget_player_preview.xml");
    assert!(player_preview.contains("android:src=\"@drawable/widget_player_preview\""));
    assert!(player_preview.contains("android:contentDescription=\"@string/widget_description\""));

    let info_preview = include_str!("../android/res/layout/widget_player_info_preview.xml");
    assert!(info_preview.contains("android:src=\"@drawable/widget_player_info_preview\""));
    assert!(info_preview.contains("android:contentDescription=\"@string/info_widget_description\""));

    let player_png = include_bytes!("../android/res/drawable-nodpi/widget_player_preview.png");
    let info_png = include_bytes!("../android/res/drawable-nodpi/widget_player_info_preview.png");
    assert_eq!(&player_png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&info_png[..8], b"\x89PNG\r\n\x1a\n");
    let info_image = image::load_from_memory(info_png).unwrap().to_rgba8();
    assert_eq!(info_image.dimensions(), (672, 164));
    assert!(info_image.pixels().all(|pixel| pixel.0[3] == 255));
    for x in 0..info_image.width() {
        for y in 0..8 {
            assert_eq!(info_image.get_pixel(x, y).0, [0, 0, 0, 255]);
            assert_eq!(
                info_image.get_pixel(x, info_image.height() - 1 - y).0,
                [0, 0, 0, 255]
            );
        }
    }
    for y in 0..info_image.height() {
        for x in 0..8 {
            assert_eq!(info_image.get_pixel(x, y).0, [0, 0, 0, 255]);
            assert_eq!(
                info_image.get_pixel(info_image.width() - 1 - x, y).0,
                [0, 0, 0, 255]
            );
        }
    }

    let packaging = include_str!("../scripts/repo.py");
    assert!(packaging.contains("drawable_dir / \"widget_icon.png\""));
}

#[test]
fn player_info_widget_uses_shared_skin_and_exact_main_crop() {
    let rust = include_str!("../src/ui/egui/android_file_picker.rs");
    let native = rust
        .split("Java_org_xmms_renascene_XmmsPlayerInfoWidget_nativeRenderPlayerInfoWidget")
        .nth(1)
        .expect("info widget native renderer");
    assert!(native.contains("with_widget_skin(&files_dir, &cache_dir"));
    assert!(native.contains("render_player_info_color_image"));
    assert!(native.contains("title_offset_px"));
    assert!(native.contains("player_info_render_state("));

    let render = include_str!("../src/ui/egui/skin_texture.rs");
    for coordinate in [
        "PLAYER_INFO_X: usize = 104",
        "PLAYER_INFO_Y: usize = 20",
        "PLAYER_INFO_WIDTH: usize = 164",
        "PLAYER_INFO_HEIGHT: usize = 37",
    ] {
        assert!(render.contains(coordinate), "missing {coordinate}");
    }
    assert!(render.contains("for y in 0..PLAYER_INFO_HEIGHT"));
    assert!(render.contains("&info.pixels[info_start..info_start + PLAYER_INFO_WIDTH]"));
}

#[test]
fn player_info_widget_maps_title_audio_fields_and_channel_mode() {
    let render = include_str!("../src/ui/egui/skin_texture.rs");
    let mapping = render
        .split("pub fn player_info_render_state(")
        .nth(1)
        .expect("player info state mapping")
        .split("pub fn argb_to_egui_rgba")
        .next()
        .expect("player info state mapping body");
    assert!(mapping.contains("\"XMMS Renascene\".to_string()"));
    assert!(mapping.contains("bitrate > 0"));
    assert!(mapping.contains("frequency > 0"));
    assert!(mapping.contains("channels: channels.max(0)"));
    assert!(mapping.contains("title_offset_px: title_offset_px.max(0)"));

    let renderer = include_str!("../src/render/main.rs");
    assert!(renderer.contains("2 => (0, 12)"));
    assert!(renderer.contains("1 => (12, 0)"));
    assert!(renderer.contains("_ => (12, 12)"));
}

#[test]
fn both_widgets_share_proportional_per_instance_sizing() {
    let support = include_str!("../android/java/org/xmms/renascene/XmmsWidgetSupport.java");
    assert!(support.contains("OPTION_APPWIDGET_MAX_WIDTH"));
    assert!(support.contains("OPTION_APPWIDGET_MIN_WIDTH"));
    assert!(support.contains("OPTION_APPWIDGET_MIN_HEIGHT"));
    assert!(support.contains("OPTION_APPWIDGET_MAX_HEIGHT"));
    assert!(
        support.contains("contentHeight = Math.round((float) width * nativeHeight / nativeWidth)")
    );
    assert!(
        support.contains("contentWidth = Math.round((float) height * nativeWidth / nativeHeight)")
    );

    for provider in [
        include_str!("../android/java/org/xmms/renascene/XmmsPlayerWidget.java"),
        include_str!("../android/java/org/xmms/renascene/XmmsPlayerInfoWidget.java"),
    ] {
        assert!(provider.contains("onAppWidgetOptionsChanged("));
        assert!(provider.contains("manager.getAppWidgetOptions(widgetId)"));
        assert!(provider.contains("XmmsWidgetSupport.proportionalPadding("));
        assert!(provider.contains("manager.updateAppWidget("));
        assert!(!provider.contains("manager.updateAppWidget(widgetIds,"));
    }
}

#[test]
fn playback_callbacks_refresh_info_for_display_or_transport_changes() {
    let service = include_str!("../android/java/org/xmms/renascene/XmmsPlaybackService.java");
    let apply = service
        .split("public void applyNativePlaybackState(")
        .nth(1)
        .expect("playback state callback")
        .split("public void applyNativePlaybackPosition")
        .next()
        .expect("playback callback body");
    assert!(apply.contains("boolean infoChanged"));
    assert!(apply.contains("boolean playbackChanged"));
    assert!(apply.contains("if (state != 0 && (infoChanged || playbackChanged))"));
    assert!(apply.contains("XmmsPlayerInfoWidget.updateAll("));

    let position = service
        .split("public void applyNativePlaybackPosition")
        .nth(1)
        .expect("position callback")
        .split("@Override")
        .next()
        .expect("position callback body");
    assert!(!position.contains("XmmsPlayerInfoWidget"));

    let rust = include_str!("../src/ui/egui/android_file_picker.rs");
    assert!(rust.contains("PlaybackEvent::StreamInfo(_)"));
    assert!(rust.contains("stream_info.bitrate.unwrap_or_default()"));
    assert!(rust.contains("stream_info.frequency.unwrap_or_default()"));
    assert!(rust.contains("stream_info.channels.unwrap_or_default()"));
}

#[test]
fn player_info_widget_marquee_reuses_native_bitmap_title_behavior() {
    let provider = include_str!("../android/java/org/xmms/renascene/XmmsPlayerInfoWidget.java");
    assert!(provider.contains("nativeUpdateTitleMarquee("));
    assert!(provider.contains("MARQUEE_TICK_MS = 250"));
    assert!(provider.contains("state.playbackState"));
    assert!(provider.contains("marqueeChanged(marquee)"));
    assert!(provider.contains("MARQUEE_HANDLER.postDelayed(this, MARQUEE_TICK_MS)"));
    assert!(provider.contains("stopMarquee()"));
    assert!(!provider.contains("TextView"));

    let native = include_str!("../src/ui/egui/android_file_picker.rs");
    let marquee = native
        .split("Java_org_xmms_renascene_XmmsPlayerInfoWidget_nativeUpdateTitleMarquee")
        .nth(1)
        .expect("native widget marquee")
        .split("fn jstring_path")
        .next()
        .expect("native widget marquee body");
    assert!(native.contains("WIDGET_TITLE_MARQUEE"));
    assert!(marquee.contains("TitleMarquee::default()"));
    assert!(marquee.contains("crate::render::MAIN_TITLE_TEXT_WIDTH"));
    assert!(marquee.contains("PlayerState::Playing"));
    assert!(marquee.contains("PlayerState::Paused"));
    assert!(marquee.contains("PlayerState::Stopped"));
    assert!(marquee.contains("marquee.offset_px()"));
    assert!(marquee.contains("marquee.is_scrolling(player_state, true)"));

    let renderer = include_str!("../src/render/main.rs");
    assert!(renderer.contains("render_text_offset("));
    assert!(renderer.contains("state.title_offset_px"));
}

#[test]
fn packaged_widget_refresh_preserves_instance_state_without_starting_playback() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsPlayerWidget.java");
    let refresh = java
        .split("static void refreshAll(Context context)")
        .nth(1)
        .expect("packaged Java widget refresh method")
        .split("private static void showPressedControl")
        .next()
        .expect("refresh method body");

    assert!(refresh.contains("getAppWidgetIds(provider)"));
    assert!(refresh.contains("widgetIds.length == 0"));
    assert!(refresh.contains("loadState(applicationContext)"));
    assert!(refresh.contains("pressedControl"));
    assert!(refresh.contains("updateWidgets("));
    assert!(!refresh.contains("startService"));
    assert!(!refresh.contains("startForegroundService"));

    let update_widgets = java
        .split("private static void updateWidgets(")
        .nth(1)
        .expect("per-instance widget update helper")
        .split("private static void updateWidget(")
        .next()
        .expect("updateWidgets body");
    assert!(update_widgets.contains("manager.getAppWidgetOptions(widgetId)"));
}

#[test]
fn skin_config_is_persisted_before_widget_refresh() {
    let app = include_str!("../src/ui/egui/app.rs");
    let apply = app
        .split("pub(crate) fn apply_preferences_config")
        .nth(1)
        .expect("preferences config application")
        .split("fn persist_android_state")
        .next()
        .expect("apply_preferences_config body");
    let persist = apply
        .find("self.persist_android_state();")
        .expect("Android config persistence");
    let refresh = apply
        .find("refresh_player_widgets()")
        .expect("widget refresh after skin config change");

    assert!(persist < refresh);
    assert!(apply.contains("skin_changed"));
}

#[test]
fn touched_widget_control_reaches_native_pressed_rendering() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsPlayerWidget.java");
    let receive = java
        .split("public void onReceive(Context context, Intent intent)")
        .nth(1)
        .expect("widget broadcast receiver")
        .split("public void onDisabled")
        .next()
        .expect("onReceive body");
    let extract = receive.find("intent.getIntExtra(EXTRA_CONTROL").unwrap();
    let pressed = receive
        .find("showPressedControl(context, control)")
        .unwrap();
    let service = receive
        .find("putExtra(XmmsPlaybackService.EXTRA_WIDGET_CONTROL, control)")
        .unwrap();
    assert!(extract < pressed);
    assert!(pressed < service);

    let remote_views = java
        .split("private static RemoteViews remoteViews(")
        .nth(1)
        .expect("widget RemoteViews renderer")
        .split("private static WidgetPadding widgetPadding")
        .next()
        .expect("remoteViews body");
    assert!(remote_views.contains("int activePressedControl"));
    assert!(remote_views.contains(
        "nativeRenderPlayerWidget(\n                context.getFilesDir().getAbsolutePath(),\n                context.getCacheDir().getAbsolutePath(),\n                activePressedControl)"
    ));
}

#[test]
fn pressed_widget_restore_is_timed_and_stale_safe() {
    let java = include_str!("../android/java/org/xmms/renascene/XmmsPlayerWidget.java");
    let duration = java
        .split("PRESSED_DURATION_MS = ")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .expect("numeric pressed duration");
    assert!((100..=200).contains(&duration));

    let pressed = java
        .split("private static void showPressedControl(Context context, int control)")
        .nth(1)
        .expect("pressed-control renderer")
        .split("private static void updateWidgets")
        .next()
        .expect("showPressedControl body");
    assert!(pressed.contains("long generation = ++pressedGeneration"));
    assert!(pressed.contains("PRESSED_HANDLER.removeCallbacks(restorePressedRunnable)"));
    assert!(pressed.contains("if (generation != pressedGeneration)"));
    assert!(pressed.contains("pressedControl = NO_PRESSED_CONTROL"));
    assert!(pressed
        .contains("PRESSED_HANDLER.postDelayed(restorePressedRunnable, PRESSED_DURATION_MS)"));

    let disabled = java
        .split("public void onDisabled(Context context)")
        .nth(1)
        .expect("widget disable cleanup")
        .split("static void updateAll")
        .next()
        .expect("onDisabled body");
    assert!(disabled.contains("pressedGeneration++"));
    assert!(disabled.contains("PRESSED_HANDLER.removeCallbacks(restorePressedRunnable)"));
}

#[test]
fn native_widget_renderer_maps_controls_to_pressed_sprites() {
    let rust = include_str!("../src/ui/egui/android_file_picker.rs");
    let render = rust
        .split("Java_org_xmms_renascene_XmmsPlayerWidget_nativeRenderPlayerWidget")
        .nth(1)
        .expect("native widget renderer");
    for mapping in [
        "1 => Some(crate::skin::layout::MainPushButton::Pause)",
        "2 => Some(crate::skin::layout::MainPushButton::Play)",
        "3 => Some(crate::skin::layout::MainPushButton::Next)",
        "4 => Some(crate::skin::layout::MainPushButton::Previous)",
        "6 => Some(crate::skin::layout::MainPushButton::Stop)",
    ] {
        assert!(
            render.contains(mapping),
            "missing native mapping: {mapping}"
        );
    }
    assert!(render.contains("render_transport_buttons_color_image(skin, pressed)"));
}
