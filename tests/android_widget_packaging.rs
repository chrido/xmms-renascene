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
    assert!(refresh
        .contains("MAIN_HANDLER.post(() -> XmmsPlayerWidget.refreshAll(applicationContext))"));
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
