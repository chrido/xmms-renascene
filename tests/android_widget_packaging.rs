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
fn player_info_widget_is_packaged_and_opens_player() {
    let provider = include_str!("../android/java/org/xmms/renascene/XmmsPlayerInfoWidget.java");
    assert!(provider.contains("INFO_WIDTH = 157"));
    assert!(provider.contains("INFO_HEIGHT = 26"));
    assert!(provider.contains("OPEN_PLAYER_REQUEST_CODE = 1000"));
    assert!(provider.contains("new Intent(context, XmmsActivity.class)"));
    assert!(provider.contains("PendingIntent.getActivity("));
    assert!(provider.contains("OPEN_PLAYER_REQUEST_CODE,"));
    assert!(provider.contains("PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE"));
    assert!(provider.contains("views.setOnClickPendingIntent(container"));
    assert!(provider.contains("widgetIds.length == 0"));
    assert!(provider.contains("nativeRenderPlayerInfoWidget("));
    assert!(provider.contains("XmmsWidgetSupport.proportionalPadding("));
    assert!(provider.contains("onAppWidgetOptionsChanged("));
    assert!(provider.contains("manager.getAppWidgetOptions(widgetId)"));
    assert!(provider.contains("manager.updateAppWidget(widgetId, views)"));
    assert!(!provider.contains("manager.updateAppWidget(widgetIds, views)"));

    let layout = include_str!("../android/res/layout/widget_player_info.xml");
    assert!(layout.contains("@+id/widget_player_info_container"));
    assert!(layout.contains("@+id/widget_player_info_image"));
    assert!(!layout.contains("ImageButton"));

    let info = include_str!("../android/res/xml/player_info_widget_info.xml");
    assert!(info.contains("@string/info_widget_description"));
    assert!(info.contains("@layout/widget_player_info"));
    assert!(info.contains("android:minWidth=\"157dp\""));
    assert!(info.contains("android:minHeight=\"26dp\""));
    assert!(info.contains("android:resizeMode=\"horizontal|vertical\""));

    let packaging = include_str!("../scripts/repo.py");
    assert!(packaging.contains("android:name=\".XmmsPlayerWidget\""));
    assert!(packaging.contains("android:resource=\"@xml/player_widget_info\""));
    assert!(packaging.contains("android:name=\".XmmsPlayerInfoWidget\""));
    assert!(packaging.contains("android:resource=\"@xml/player_info_widget_info\""));
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
    assert!(native.contains("player_info_render_state(&title, bitrate, frequency, channels)"));

    let render = include_str!("../src/ui/egui/skin_texture.rs");
    for coordinate in [
        "PLAYER_INFO_X: usize = 111",
        "PLAYER_INFO_Y: usize = 27",
        "PLAYER_INFO_WIDTH: usize = 157",
        "PLAYER_INFO_HEIGHT: usize = 26",
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
fn playback_callbacks_refresh_info_only_for_display_changes() {
    let service = include_str!("../android/java/org/xmms/renascene/XmmsPlaybackService.java");
    let apply = service
        .split("public void applyNativePlaybackState(")
        .nth(1)
        .expect("playback state callback")
        .split("public void applyNativePlaybackPosition")
        .next()
        .expect("playback callback body");
    assert!(apply.contains("boolean infoChanged"));
    assert!(apply.contains("if (infoChanged)"));
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
