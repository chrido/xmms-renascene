#[test]
fn cold_widget_control_initializes_native_context_before_creating_audio_backend() {
    let service = include_str!("../android/java/org/xmms/renascene/XmmsPlaybackService.java");
    assert!(service.contains("private native void nativeOnMediaControl(int control, long value)"));

    let native = include_str!("../src/ui/egui/android_file_picker.rs");
    let callback = native
        .split("Java_org_xmms_renascene_XmmsPlaybackService_nativeOnMediaControl")
        .nth(1)
        .expect("playback service JNI callback")
        .split("Java_org_xmms_renascene_XmmsPlaybackService_nativeInitializeMediaLibrary")
        .next()
        .expect("playback service JNI callback body");
    let process_context = callback
        .find("ensure_process_android_context")
        .expect("process Android context initialization");
    let dispatch = callback
        .find("handle_android_media_control")
        .expect("native media-control dispatch");
    assert!(process_context < dispatch);
    assert!(native.contains("PLAYBACK_BACKEND"));
    assert!(native.contains("getApplicationContext"));
    assert!(native.contains("initialize_android_context_process_wide"));
    assert!(!native.contains("ndk_context::release_android_context()"));
}

#[test]
fn cold_widget_resume_restores_the_persisted_track_position() {
    let native = include_str!("../src/ui/egui/android_file_picker.rs");
    assert!(native.contains("resume_position_ms: i64"));
    assert!(native.contains("state.config.playback_position_ms"));
    assert!(native.contains("backend.play_uri_at(&uri, position_ms)"));
}

#[test]
fn cold_info_widget_refresh_reformats_raw_state_from_persisted_config() {
    let provider = include_str!("../android/java/org/xmms/renascene/XmmsPlayerInfoWidget.java");
    let load = provider
        .split("private static WidgetState loadState(Context context)")
        .nth(1)
        .expect("persisted info-widget state loader")
        .split("private static synchronized void setMarqueeActive")
        .next()
        .expect("persisted info-widget state loader body");
    assert!(load.contains("preferences.getString(KEY_FILENAME, \"\")"));
    assert!(load.contains("preferences.getString(KEY_METADATA_TITLE, \"\")"));
    assert!(load.contains("return new WidgetState("));
    assert!(
        provider.contains("this.title = formatTitle(context, this.filename, this.metadataTitle)")
    );

    let app = include_str!("../src/ui/egui/app.rs");
    let apply = app
        .split("pub(crate) fn apply_preferences_config")
        .nth(1)
        .expect("preferences application")
        .split("fn persist_android_state")
        .next()
        .expect("preferences application body");
    let persist = apply
        .find("self.persist_android_state();")
        .expect("persisted Android config");
    let refresh = apply
        .find("refresh_player_widgets()")
        .expect("widget refresh");
    assert!(persist < refresh);
    assert!(apply.contains("previous.convert_underscore != config.convert_underscore"));
    assert!(apply.contains("previous.convert_twenty != config.convert_twenty"));
    assert!(apply.contains("previous.title_format != config.title_format"));
    assert!(apply.contains("self.android_media_playlist_snapshot = None"));
}
