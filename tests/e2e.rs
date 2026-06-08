use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use xmms_renascene::e2e::{
    MainTarget, MenuItem, PanelTarget, PlayerSettings, Shortcut, UiE2e, Window,
};
use xmms_renascene::mpris::{MprisCommand, MprisEvent};
use xmms_renascene::player::{OutputDevice, OutputDeviceSelection, PlayerState};
use xmms_renascene::playlist::{Playlist, PlaylistSortKey};
use xmms_renascene::podcast::{
    add_feed_to_playlist, cache_file_is_fresh, cache_is_fresh, cache_path_for_url,
    classify_url_response, cleanup_cache_dir, discover_cached_duration_ms,
    download_url_with_retries, download_with_retries, fetch_url_into_playlist, handle_url_response,
    mark_cache_failed_and_skip_current, parse_feed, prepare_playback_uri, refresh_interval_seconds,
    retry_delay_seconds, stale_cache_files, status_should_retry, write_cache_file,
    PodcastCacheEntry, PodcastDownloadAttempt, PodcastHttpResponse, PodcastRefreshScheduler,
    PodcastResponseAction, PodcastUrlKind,
};
use xmms_renascene::render::{
    EQUALIZER_WINDOW_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH, PLAYLIST_DEFAULT_HEIGHT,
};
use xmms_renascene::session::{
    application_launch_flags, apply_session_command, load_saved_state, parse_session_command,
    restore_state_dict, save_fallback_state, save_session_state, save_state_dict, SessionState,
};
use xmms_renascene::skin::widget::{
    VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode, VisScopeMode, VisVuMode,
};
use xmms_renascene::skin::{skin_browser_search_dirs, SkinPixmapKind};
use xmms_renascene::spotify::{
    auth_code_request_body, authorization_url, code_challenge_for_verifier,
    config_path as spotify_config_path, exchange_code_for_token_with_url, parse_devices_response,
    parse_playback_state_response, parse_playlist_tracks_response, parse_playlists_response,
    play_track_body, playlist_tracks_endpoint, playlists_endpoint, preferred_device_id,
    refresh_access_token_with_url, SpotifyAuthConfig, SpotifyAuthState, SpotifyPlaybackRequest,
    SpotifyPlaylist, SpotifyTrack, CLIENT_ID, REDIRECT_URI,
};
use xmms_renascene::ui::{
    preferences_page_parity_controls, preferences_window_default_size,
    preferences_zoom_spans_full_width, visualization_preference_sensitivity, PanelKind,
    PlaylistContextAction, PlaylistMenuKind, PlaylistSortAction, PreferencesPage,
    SpotifyChooserPage,
};

#[test]
fn titlebar_buttons_keep_player_open_minimize_shade_and_close() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click(MainTarget::MENU)
        .assert_window_visible(Window::Player)
        .assert_player_not_minimized()
        .assert_player_unshaded()
        .assert_menu_visible()
        .click_menu_item(MenuItem::Preferences)
        .assert_menu_hidden()
        .assert_window_visible(Window::Preferences);

    app.click(MainTarget::MINIMIZE)
        .assert_window_visible(Window::Player)
        .assert_player_minimized();

    app.click(MainTarget::SHADE)
        .assert_window_visible(Window::Player)
        .assert_player_shaded();

    app.click(MainTarget::SHADE)
        .assert_window_visible(Window::Player)
        .assert_player_unshaded();

    app.click(MainTarget::CLOSE)
        .assert_window_hidden(Window::Player)
        .assert_window_hidden(Window::Playlist)
        .assert_window_hidden(Window::Equalizer);
}

#[test]
fn cli_startup_flags_are_accepted_by_gtk_smoke_mode() {
    let root = unique_temp_dir("xmms-rs-cli-smoke-skin");
    fs::create_dir_all(&root).unwrap();
    let skin = root.join("base-2.9.1.wsz");
    write_one_pixel_wsz(&skin, "#010203");

    let status = Command::new(env!("CARGO_BIN_EXE_xmms-rs"))
        .args([
            "--gtk-smoke",
            "--playlist",
            "--equalizer",
            "--shade-main",
            "--shade-playlist",
            "--shade-equalizer",
            "--undock-playlist",
            "--undock-equalizer",
            "--playlist-menu-list",
            "--reset",
            "--skin",
            skin.to_str().unwrap(),
            "--playlist-size=325x280",
        ])
        .status()
        .unwrap();

    assert!(status.success());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_primary_binary_loads_bundled_skin_without_gtk_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_xmms-rs"))
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xmms-rs: loaded 14 skin pixmaps"));
}

#[test]
fn cli_primary_binary_loads_requested_skin_without_gtk_mode() {
    let root = unique_temp_dir("xmms-rs-cli-primary-skin");
    let skin = root.join("base-2.9.1");
    write_one_pixel_skin(&skin, "#010203");

    let output = Command::new(env!("CARGO_BIN_EXE_xmms-rs"))
        .args(["--skin", skin.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xmms-rs: loaded 1 skin pixmaps"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn session_e2e_flags_secondary_activation_and_state_dict_match_c_contract() {
    let mut state = xmms_renascene::app_state::AppState::default();
    let flags = application_launch_flags(Some("1"));
    assert!(flags.handles_command_line);
    assert!(flags.non_unique);

    let command = parse_session_command(
        &[
            "xmms-rs",
            "--playlist-menu-add",
            "--undock-playlist",
            "--equalizer",
            "--skin",
            "/tmp/session-skin.wsz",
            "https://example.test/session.mp3",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>(),
    )
    .unwrap();
    let result = apply_session_command(&mut state, &command).unwrap();

    assert!(state.config.playlist_visible);
    assert!(state.config.playlist_detached);
    assert!(state.config.equalizer_visible);
    assert_eq!(state.config.skin.as_deref(), Some("/tmp/session-skin.wsz"));
    assert_eq!(
        state.playlist.entries()[0].filename,
        "https://example.test/session.mp3"
    );
    assert!(result.should_start_playback);

    let saved = save_session_state(&state);
    assert_eq!(
        save_state_dict(&saved),
        std::collections::BTreeMap::from([
            ("equalizer-detached", false),
            ("equalizer-visible", true),
            ("playlist-detached", true),
            ("playlist-visible", true),
        ])
    );

    let mut restored = xmms_renascene::app_state::AppState::default();
    restore_state_dict(
        &mut restored,
        &std::collections::BTreeMap::from([
            ("playlist-visible".to_string(), true),
            ("playlist-detached".to_string(), true),
            ("equalizer-visible".to_string(), true),
            ("equalizer-detached".to_string(), true),
        ]),
        false,
    );
    assert_eq!(
        save_session_state(&restored),
        SessionState {
            playlist_visible: true,
            playlist_detached: true,
            equalizer_visible: true,
            equalizer_detached: true,
        }
    );
}

#[test]
fn session_e2e_fallback_save_and_reset_load_preserve_config_and_playlist() {
    let root = unique_temp_dir("xmms-rs-session-save");
    let config_path = root.join("config");
    let playlist_path = root.join("playlist.m3u");
    let mut state = xmms_renascene::app_state::AppState::default();
    state.config.playlist_visible = true;
    state.config.equalizer_visible = true;
    state.playlist.add_uri("https://example.test/fallback.mp3");

    save_fallback_state(&mut state, &config_path, &playlist_path).unwrap();
    let loaded = load_saved_state(&config_path, &playlist_path, false).unwrap();
    assert!(loaded.config.playlist_visible);
    assert!(loaded.config.equalizer_visible);
    assert_eq!(
        loaded.playlist.entries()[0].filename,
        "https://example.test/fallback.mp3"
    );

    let reset = load_saved_state(&config_path, &playlist_path, true).unwrap();
    assert!(!reset.config.playlist_visible);
    assert!(reset.playlist.is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn session_e2e_runtime_snapshot_restores_playlist_position_and_playback_options() {
    let root = unique_temp_dir("xmms-rs-runtime-session-save");
    let config_path = root.join("config");
    let playlist_path = root.join("playlist.m3u");
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_equalizer_visible(true),
    );

    app.drop_on_playlist([
        "file:///music/session-one.ogg",
        "file:///music/session-two.ogg",
    ])
    .click(MainTarget::NEXT)
    .click(MainTarget::NEXT)
    .click(MainTarget::SHUFFLE)
    .click(MainTarget::REPEAT)
    .click(MainTarget::SHADE)
    .click_panel(PanelTarget::EqualizerShade)
    .click_panel(PanelTarget::PlaylistShade)
    .accept_jump_time("1:23")
    .save_runtime_snapshot(&config_path, &playlist_path);

    let loaded = load_saved_state(&config_path, &playlist_path, false).unwrap();
    assert!(loaded.config.playlist_visible);
    assert!(loaded.config.equalizer_visible);
    assert!(loaded.config.main_shaded);
    assert!(loaded.config.equalizer_shaded);
    assert!(loaded.config.playlist_shaded);
    assert!(loaded.playlist.shuffle());
    assert!(loaded.playlist.repeat());
    assert_eq!(loaded.playlist.position(), Some(1));
    assert_eq!(loaded.config.playback_position_ms, 83_000);
    assert_eq!(
        loaded.playlist.entries()[1].filename,
        "file:///music/session-two.ogg"
    );

    let mut restored = UiE2e::start_from_app_state(loaded.clone());
    restored
        .assert_playlist_position(Some(1))
        .assert_shuffle(true)
        .assert_repeat(true)
        .assert_playback_position_ms(83_000);

    UiE2e::start_from_app_state(loaded)
        .assert_player_shaded()
        .assert_equalizer_shaded()
        .assert_playlist_shaded();

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn session_e2e_runtime_snapshot_restores_window_and_equalizer_options() {
    let root = unique_temp_dir("xmms-rs-runtime-options-save");
    let config_path = root.join("config");
    let playlist_path = root.join("playlist.m3u");
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_equalizer_visible(true),
    );

    app.open_preferences_page(PreferencesPage::Options)
        .set_preference_playlist_docked(false)
        .set_preference_equalizer_docked(false)
        .click_panel(PanelTarget::EqualizerOn)
        .click_panel(PanelTarget::EqualizerAuto)
        .drag_equalizer_preamp(25)
        .drag_equalizer_band(0, 10)
        .save_runtime_snapshot(&config_path, &playlist_path);

    let loaded = load_saved_state(&config_path, &playlist_path, false).unwrap();
    assert!(loaded.config.playlist_visible);
    assert!(loaded.config.playlist_detached);
    assert!(loaded.config.equalizer_visible);
    assert!(loaded.config.equalizer_detached);
    assert!(!loaded.config.equalizer_active);
    assert!(loaded.config.equalizer_auto);
    assert_eq!(loaded.config.equalizer_preamp_pos, 24);
    assert_eq!(loaded.config.equalizer_band_pos[0], 10);

    UiE2e::start_from_app_state(loaded)
        .assert_panel_detached(PanelKind::Playlist, true)
        .assert_panel_detached(PanelKind::Equalizer, true)
        .assert_equalizer_active(false)
        .assert_equalizer_automatic(true)
        .assert_equalizer_preamp_position(24)
        .assert_equalizer_band_position(0, 10);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn main_menu_items_trigger_their_preview_actions() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click(MainTarget::MENU)
        .assert_menu_visible()
        .click_menu_item(MenuItem::OpenFiles)
        .assert_menu_hidden()
        .assert_file_dialog_visible();

    app.click(MainTarget::MENU)
        .assert_menu_visible()
        .click_menu_item(MenuItem::OpenLocation)
        .assert_menu_hidden()
        .assert_window_visible(Window::OpenLocation)
        .accept_open_location("https://example.test/song.ogg")
        .assert_window_hidden(Window::OpenLocation)
        .assert_last_open_location("https://example.test/song.ogg")
        .assert_playlist_entry(0, "https://example.test/song.ogg")
        .assert_player_state(PlayerState::Playing);

    app.click(MainTarget::MENU)
        .assert_menu_visible()
        .click_menu_item(MenuItem::Preferences)
        .assert_menu_hidden()
        .assert_window_visible(Window::Preferences);

    app.click(MainTarget::MENU)
        .assert_menu_visible()
        .click_menu_item(MenuItem::SkinBrowser)
        .assert_menu_hidden()
        .assert_window_visible(Window::SkinBrowser);

    app.click(MainTarget::MENU)
        .assert_menu_visible()
        .click_menu_item(MenuItem::Quit)
        .assert_window_hidden(Window::Player);
}

#[test]
fn main_prompts_accept_location_and_jump_time_values() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click(MainTarget::MENU)
        .click_menu_item(MenuItem::OpenLocation)
        .assert_window_visible(Window::OpenLocation)
        .accept_open_location("file:///tmp/example.mp3")
        .assert_window_hidden(Window::OpenLocation)
        .assert_last_open_location("file:///tmp/example.mp3")
        .assert_playlist_entry(0, "file:///tmp/example.mp3")
        .assert_player_state(PlayerState::Playing);

    app.show_jump_time_prompt()
        .assert_window_visible(Window::JumpTime)
        .accept_jump_time("1:23")
        .assert_window_hidden(Window::JumpTime)
        .assert_last_jump_time_ms(83_000)
        .assert_mpris_position_us(83_000_000);

    app.show_jump_time_prompt()
        .accept_jump_time("42")
        .assert_last_jump_time_ms(42_000)
        .assert_mpris_position_us(42_000_000);
}

#[test]
fn prompt_keyboard_shortcuts_open_location_and_jump_time() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.press_shortcut(Shortcut::OpenLocation)
        .assert_window_visible(Window::OpenLocation);

    app.press_shortcut(Shortcut::JumpTime)
        .assert_window_visible(Window::JumpTime);
}

#[test]
fn main_keyboard_shortcuts_trigger_preview_actions() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_spotify_entry("spotify:track:shortcut", "Shortcut", 10_000)
        .press_shortcut(Shortcut::Play)
        .assert_player_state(PlayerState::Playing)
        .press_shortcut(Shortcut::Pause)
        .assert_player_state(PlayerState::Paused)
        .press_shortcut(Shortcut::Stop)
        .assert_player_state(PlayerState::Stopped)
        .click(MainTarget::position(219))
        .assert_position(219)
        .press_shortcut(Shortcut::Previous)
        .assert_position(0)
        .click(MainTarget::position(219))
        .assert_position(219)
        .press_shortcut(Shortcut::Next)
        .assert_position(0);

    app.press_shortcut(Shortcut::OpenFiles)
        .assert_file_dialog_visible()
        .press_shortcut(Shortcut::ReloadSkin)
        .assert_skin_reload_count(1)
        .assert_shuffle(false)
        .press_shortcut(Shortcut::ToggleShuffle)
        .assert_shuffle(true)
        .assert_repeat(false)
        .press_shortcut(Shortcut::ToggleRepeat)
        .assert_repeat(true)
        .assert_no_advance(false)
        .press_shortcut(Shortcut::ToggleNoAdvance)
        .assert_no_advance(true)
        .press_shortcut(Shortcut::TimerRemaining)
        .assert_preference_timer_remaining(true)
        .press_shortcut(Shortcut::TimerElapsed)
        .assert_preference_timer_remaining(false)
        .press_shortcut(Shortcut::ToggleSticky)
        .assert_sticky(true)
        .press_shortcut(Shortcut::ToggleDoubleSize)
        .assert_double_size(false)
        .press_shortcut(Shortcut::Preferences)
        .assert_window_visible(Window::Preferences)
        .press_shortcut(Shortcut::SkinBrowser)
        .assert_window_visible(Window::SkinBrowser);
}

#[test]
fn main_feature_shortcuts_file_info_and_play_first_are_wired() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.accept_open_location("file:///tmp/first.mp3")
        .accept_open_location("file:///tmp/second.mp3")
        .press_shortcut(Shortcut::Next)
        .assert_playlist_position(Some(1))
        .press_shortcut(Shortcut::FileInfo)
        .assert_last_playlist_file_info("second")
        .press_shortcut(Shortcut::PlayFirst)
        .assert_playlist_position(Some(0))
        .assert_player_state(PlayerState::Playing);
}

#[test]
fn panel_keyboard_shortcuts_toggle_and_shade_windows() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_detached(true)
            .with_equalizer_detached(true),
    );

    app.press_shortcut(Shortcut::TogglePlaylist)
        .assert_window_visible(Window::Playlist)
        .assert_playlist_unshaded()
        .press_shortcut(Shortcut::ShadePlaylist)
        .assert_playlist_shaded()
        .press_shortcut(Shortcut::ToggleEqualizer)
        .assert_window_visible(Window::Equalizer)
        .assert_equalizer_unshaded()
        .press_shortcut(Shortcut::ShadeEqualizer)
        .assert_equalizer_shaded()
        .assert_player_unshaded()
        .press_shortcut(Shortcut::ShadeMain)
        .assert_player_shaded();
}

#[test]
fn drag_and_drop_on_main_replaces_playlist_and_starts_playback() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist(["file:///tmp/old.ogg"])
        .assert_playlist_len(1)
        .drop_on_main(["file:///tmp/first.ogg", "file:///tmp/second.ogg"])
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/first.ogg")
        .assert_playlist_entry(1, "file:///tmp/second.ogg")
        .assert_player_state(PlayerState::Playing);
}

#[test]
fn drag_and_drop_on_playlist_appends_to_existing_entries() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.drop_on_playlist(["file:///tmp/first.ogg"])
        .drop_on_playlist(["https://example.test/stream"])
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/first.ogg")
        .assert_playlist_entry(1, "https://example.test/stream")
        .assert_player_state(PlayerState::Stopped);
}

#[test]
fn playlist_navigation_controls_update_position_and_eof_behavior() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist(["file:///tmp/one.ogg", "file:///tmp/two.ogg"])
        .click(MainTarget::NEXT)
        .assert_playlist_position(Some(0))
        .assert_current_playlist_entry("file:///tmp/one.ogg")
        .assert_last_playback_request(Some("file:///tmp/one.ogg"))
        .assert_player_state(PlayerState::Playing)
        .click(MainTarget::NEXT)
        .assert_playlist_position(Some(1))
        .assert_current_playlist_entry("file:///tmp/two.ogg")
        .assert_last_playback_request(Some("file:///tmp/two.ogg"))
        .click(MainTarget::NEXT)
        .assert_playlist_position(Some(1))
        .assert_last_playback_request(Some("file:///tmp/two.ogg"))
        .click(MainTarget::PREVIOUS)
        .assert_playlist_position(Some(0))
        .assert_current_playlist_entry("file:///tmp/one.ogg")
        .assert_last_playback_request(Some("file:///tmp/one.ogg"))
        .click(MainTarget::REPEAT)
        .click(MainTarget::PREVIOUS)
        .assert_playlist_position(Some(1))
        .assert_current_playlist_entry("file:///tmp/two.ogg")
        .assert_last_playback_request(Some("file:///tmp/two.ogg"))
        .press_shortcut(Shortcut::ToggleNoAdvance)
        .playlist_eof_reached()
        .assert_playlist_position(Some(1))
        .assert_player_state(PlayerState::Stopped);
}

#[test]
fn shaded_transport_controls_trigger_playback_actions() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist(["file:///tmp/one.ogg", "file:///tmp/two.ogg"])
        .click(MainTarget::SHADE)
        .assert_player_shaded()
        .click(MainTarget::PLAY)
        .assert_player_state(PlayerState::Playing)
        .assert_playlist_position(Some(0))
        .click(MainTarget::PAUSE)
        .assert_player_state(PlayerState::Paused)
        .click(MainTarget::PAUSE)
        .assert_player_state(PlayerState::Playing)
        .click(MainTarget::NEXT)
        .assert_playlist_position(Some(1))
        .assert_current_playlist_entry("file:///tmp/two.ogg")
        .click(MainTarget::PREVIOUS)
        .assert_playlist_position(Some(0))
        .assert_current_playlist_entry("file:///tmp/one.ogg")
        .click(MainTarget::STOP)
        .assert_player_state(PlayerState::Stopped);
}

#[test]
fn shaded_player_displays_time_and_position_slider() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_spotify_entry("spotify:track:one", "Song", 130_000)
        .press_shortcut(Shortcut::PlayFirst)
        .click(MainTarget::SHADE)
        .assert_player_shaded()
        .assert_shaded_main_position_visible(true)
        .update_timer_tick(65_000)
        .assert_shaded_main_time_text(" 01", "05")
        .assert_shaded_main_position(7)
        .click_at(242, 7)
        .assert_playback_position_ms(130_000)
        .assert_position(219)
        .click(MainTarget::STOP)
        .assert_shaded_main_time_text("   ", "  ")
        .assert_shaded_main_position_visible(false);
}

#[test]
fn accepted_file_dialog_replaces_playlist_and_starts_playback() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist(["file:///tmp/old.ogg"])
        .press_shortcut(Shortcut::OpenFiles)
        .assert_file_dialog_visible()
        .accept_file_dialog(["file:///tmp/new-a.ogg", "file:///tmp/new-b.ogg"])
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/new-a.ogg")
        .assert_playlist_entry(1, "file:///tmp/new-b.ogg")
        .assert_player_state(PlayerState::Playing);
}

#[test]
fn accepted_directory_dialog_replaces_playlist_and_starts_playback() {
    let mut app = UiE2e::start_player(PlayerSettings::default());
    let music_dir = unique_temp_dir("xmms-rs-e2e-open-dir");
    fs::create_dir_all(music_dir.join("albums")).unwrap();
    fs::write(music_dir.join("albums").join("New_Song.flac"), b"audio").unwrap();
    fs::write(music_dir.join("cover.png"), b"image").unwrap();

    app.press_shortcut(Shortcut::OpenDirectory)
        .assert_directory_dialog_visible()
        .accept_directory_dialog(&file_uri(&music_dir))
        .assert_playlist_len(1)
        .assert_playlist_entry(
            0,
            &file_uri(&music_dir.join("albums").join("New_Song.flac")),
        )
        .assert_player_state(PlayerState::Playing);

    fs::remove_dir_all(music_dir).unwrap();
}

#[test]
fn spotify_and_podcast_entries_are_available_to_e2e_playlist_state() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_spotify_entry("spotify:track:123", "Spotify Song", 123_000)
        .add_podcast_entry(
            "https://example.test/episode.mp3",
            "Podcast Episode",
            "https://example.test/feed.xml",
            "episode-1",
        )
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "spotify:track:123")
        .assert_playlist_title(0, "Spotify Song")
        .assert_playlist_entry(1, "https://example.test/episode.mp3")
        .assert_playlist_title(1, "Podcast Episode");
}

#[test]
fn spotify_auth_config_and_url_match_c_contract() {
    let dir = unique_temp_dir("spotify-auth");
    let path = spotify_config_path(&dir);

    let missing = SpotifyAuthConfig::load_from_file(&path).unwrap();
    assert!(!missing.is_authenticated());

    SpotifyAuthConfig {
        refresh_token: Some("stored-refresh-token".to_string()),
    }
    .save_to_file(&path)
    .unwrap();

    let loaded = SpotifyAuthConfig::load_from_file(&path).unwrap();
    assert_eq!(
        loaded.refresh_token.as_deref(),
        Some("stored-refresh-token")
    );
    assert!(loaded.is_authenticated());

    let auth_url = authorization_url("pkce-challenge");
    assert!(auth_url.contains(&format!("client_id={CLIENT_ID}")));
    assert!(auth_url.contains(&format!("redirect_uri={REDIRECT_URI}")));
    assert!(auth_url.contains("scope=user-read-playback-state%20user-modify-playback-state"));
    assert!(auth_url.contains("code_challenge_method=S256"));
    assert!(auth_url.contains("code_challenge=pkce-challenge"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn spotify_pkce_helpers_and_auth_code_body_match_c_contract() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

    assert_eq!(
        code_challenge_for_verifier(verifier),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let body = auth_code_request_body("code value", verifier);
    assert!(body.contains("grant_type=authorization_code"));
    assert!(body.contains("code=code%20value"));
    assert!(body.contains(&format!("client_id={CLIENT_ID}")));
    assert!(body.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8391%2Fcallback"));
    assert!(body.contains("code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
}

#[test]
fn spotify_token_refresh_state_matches_c_contract() {
    let mut state = SpotifyAuthState::from_config(SpotifyAuthConfig {
        refresh_token: Some("refresh-token".to_string()),
    });

    assert!(state.is_authenticated());
    let expected_body =
        format!("grant_type=refresh_token&refresh_token=refresh-token&client_id={CLIENT_ID}");
    assert_eq!(
        state.refresh_request_body().as_deref(),
        Some(expected_body.as_str())
    );

    assert!(state.apply_token_response(
        r#"{"access_token":"access-token","expires_in":120,"refresh_token":"new-refresh"}"#,
        10,
    ));
    assert_eq!(state.access_token.as_deref(), Some("access-token"));
    assert_eq!(state.refresh_token.as_deref(), Some("new-refresh"));
    assert_eq!(state.token_expiry_unix, 70);
    assert!(state.access_token_valid(69));
    assert!(!state.access_token_valid(70));
}

#[test]
fn spotify_live_token_exchange_and_refresh_use_http_token_endpoint() {
    let response = r#"{"access_token":"access","expires_in":120,"refresh_token":"refresh-new"}"#;
    let (token_url, server) = local_http_server(vec![
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            response.len(),
            response
        ),
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            response.len(),
            response
        ),
    ]);
    let mut state = SpotifyAuthState::default();

    assert!(
        exchange_code_for_token_with_url(&mut state, &token_url, "code", "verifier", 100).unwrap()
    );
    assert_eq!(state.access_token.as_deref(), Some("access"));
    assert_eq!(state.refresh_token.as_deref(), Some("refresh-new"));
    assert_eq!(state.token_expiry_unix, 160);

    assert!(refresh_access_token_with_url(&mut state, &token_url, 200).unwrap());
    assert_eq!(state.token_expiry_unix, 260);
    server.join().unwrap();
}

#[test]
fn spotify_api_parsers_and_requests_match_c_contract() {
    assert_eq!(playlists_endpoint(50), "/me/playlists?limit=50&offset=50");
    assert_eq!(
        playlist_tracks_endpoint("playlist-id", 100),
        "/playlists/playlist-id/items?limit=100&offset=100"
    );

    let (playlists, total) = parse_playlists_response(
        r#"{"total":2,"items":[
            {"id":"old","name":"Old","uri":"spotify:playlist:old","tracks":{"total":7}},
            {"id":"new","name":"New","uri":"spotify:playlist:new","items":{"total":8}}
        ]}"#,
    )
    .unwrap();
    assert_eq!(total, 2);
    assert_eq!(playlists[0].name, "Old");
    assert_eq!(playlists[1].total_tracks, 8);

    let (tracks, total) = parse_playlist_tracks_response(
        r#"{"total":3,"items":[
            {"track":{"id":"one","name":"One","uri":"spotify:track:one","duration_ms":1000,"artists":[{"name":"Artist"}],"album":{"name":"Album"}}},
            {"item":{"id":"two","name":"Two","uri":"spotify:track:two","duration_ms":2000}},
            {"track":{"id":null}}
        ]}"#,
    )
    .unwrap();
    assert_eq!(total, 3);
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].artist.as_deref(), Some("Artist"));
    assert_eq!(tracks[0].album.as_deref(), Some("Album"));

    let devices = parse_devices_response(
        r#"{"devices":[
            {"id":"inactive","name":"Laptop","type":"Computer","is_active":false},
            {"id":"active","name":"Phone","type":"Smartphone","is_active":true}
        ]}"#,
    )
    .unwrap();
    assert_eq!(preferred_device_id(&devices), Some("active"));

    let playback = parse_playback_state_response(
        r#"{"is_playing":true,"progress_ms":42,"item":{"name":"Song","duration_ms":123,"artists":[{"name":"Artist"}]}}"#,
    )
    .unwrap();
    assert!(playback.is_playing);
    assert_eq!(playback.progress_ms, 42);
    assert_eq!(playback.track_name.as_deref(), Some("Song"));

    assert_eq!(
        play_track_body(Some("spotify:track:one"), None, 0),
        r#"{"uris":["spotify:track:one"]}"#
    );
    assert_eq!(
        SpotifyPlaybackRequest::TransferDevice {
            device_id: "active".to_string(),
        }
        .body()
        .as_deref(),
        Some(r#"{"device_ids":["active"],"play":false}"#)
    );
    assert_eq!(SpotifyPlaybackRequest::Next.method(), "POST");
    assert_eq!(SpotifyPlaybackRequest::Pause.endpoint(), "/me/player/pause");
}

#[test]
fn spotify_uri_playback_updates_player_state_like_c_player() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_spotify_entry("spotify:track:one", "Spotify One", 123_000)
        .press_shortcut(Shortcut::Play)
        .assert_player_state(PlayerState::Playing)
        .assert_player_spotify_mode(true)
        .assert_player_spotify_uri(Some("spotify:track:one"))
        .assert_player_spotify_duration_ms(123_000)
        .assert_player_spotify_position_ms(0)
        .update_timer_tick(100)
        .assert_player_spotify_position_ms(100)
        .assert_spotify_playback_poll_requests(0);

    for _ in 0..19 {
        app.update_timer_tick(100);
    }

    app.assert_spotify_playback_poll_requests(1)
        .press_shortcut(Shortcut::Pause)
        .assert_player_state(PlayerState::Paused)
        .assert_player_spotify_mode(true)
        .press_shortcut(Shortcut::Stop)
        .assert_player_state(PlayerState::Stopped)
        .assert_player_spotify_mode(false)
        .assert_player_spotify_uri(None);

    app.add_playlist_uri("file:///music/local.ogg")
        .execute_mpris_command(MprisCommand::Next)
        .assert_player_state(PlayerState::Playing)
        .assert_player_spotify_mode(false);
}

#[test]
fn spotify_playlist_window_auth_selection_import_and_empty_states_are_wired() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click_menu_item(MenuItem::Spotify)
        .assert_spotify_auth_prompt_visible(true)
        .assert_spotify_window_visible(false)
        .assert_spotify_status("Authentication required")
        .set_spotify_authenticated(true)
        .click_menu_item(MenuItem::Spotify)
        .assert_spotify_window_visible(true)
        .assert_spotify_page(SpotifyChooserPage::Playlists)
        .assert_spotify_status("Loading playlists...")
        .receive_spotify_playlists(Vec::new())
        .assert_spotify_status("0 playlists")
        .set_spotify_error("Failed to load playlists")
        .assert_spotify_status("Failed to load playlists")
        .receive_spotify_playlists(vec![SpotifyPlaylist {
            id: "playlist-id".to_string(),
            name: "Favorites".to_string(),
            total_tracks: 2,
            uri: "spotify:playlist:favorites".to_string(),
        }])
        .assert_spotify_playlists(&["Favorites"])
        .select_spotify_playlist(0)
        .assert_spotify_last_track_request(Some("playlist-id"))
        .assert_spotify_status("Loading tracks...")
        .receive_spotify_tracks(vec![
            SpotifyTrack {
                id: "one".to_string(),
                name: "One".to_string(),
                artist: Some("Artist".to_string()),
                album: Some("Album".to_string()),
                uri: "spotify:track:one".to_string(),
                duration_ms: 1_000,
            },
            SpotifyTrack {
                id: "two".to_string(),
                name: "Two".to_string(),
                artist: None,
                album: None,
                uri: "spotify:track:two".to_string(),
                duration_ms: 2_000,
            },
        ])
        .assert_spotify_page(SpotifyChooserPage::Tracks)
        .assert_spotify_status("2 tracks")
        .assert_spotify_tracks(&["1. Artist - One", "2. Unknown - Two"])
        .spotify_back_to_playlists()
        .assert_spotify_page(SpotifyChooserPage::Playlists)
        .receive_spotify_tracks(vec![SpotifyTrack {
            id: "three".to_string(),
            name: "Three".to_string(),
            artist: Some("Artist".to_string()),
            album: None,
            uri: "spotify:track:three".to_string(),
            duration_ms: 3_000,
        }])
        .load_spotify_tracks_into_playlist()
        .assert_spotify_window_visible(false)
        .assert_playlist_len(1)
        .assert_playlist_entry(0, "spotify:track:three")
        .assert_playlist_title(0, "Artist - Three")
        .click_menu_item(MenuItem::Spotify)
        .assert_spotify_window_visible(true)
        .close_spotify_window()
        .assert_spotify_window_visible(false);
}

#[test]
fn playlist_sort_e2e_orders_entries_and_preserves_current_item() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist([
        "file:///music/Beta/b_song.ogg",
        "file:///music/Alpha/c_song.ogg",
        "file:///music/Gamma/a_song.ogg",
    ])
    .click(MainTarget::NEXT)
    .assert_playlist_position(Some(0))
    .sort_playlist_by(PlaylistSortKey::Filename)
    .assert_playlist_entry(0, "file:///music/Gamma/a_song.ogg")
    .assert_playlist_entry(1, "file:///music/Beta/b_song.ogg")
    .assert_playlist_entry(2, "file:///music/Alpha/c_song.ogg")
    .assert_playlist_position(Some(1))
    .sort_playlist_by(PlaylistSortKey::Path)
    .assert_playlist_entry(0, "file:///music/Alpha/c_song.ogg")
    .assert_playlist_entry(1, "file:///music/Beta/b_song.ogg")
    .assert_playlist_entry(2, "file:///music/Gamma/a_song.ogg")
    .assert_playlist_position(Some(1));
}

#[test]
fn playlist_row_selection_footer_and_drag_reorder_are_wired() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.add_spotify_entry("spotify:track:one", "One", 60_000)
        .add_playlist_uri("file:///music/unknown.ogg")
        .add_spotify_entry("spotify:track:two", "Two", 90_000)
        .assert_playlist_footer_info("0:00/2:30+")
        .click_playlist_row(1)
        .assert_playlist_entry_selected(0, false)
        .assert_playlist_entry_selected(1, true)
        .assert_playlist_entry_selected(2, false)
        .assert_playlist_footer_info("?/2:30+")
        .drag_playlist_row(1, 0)
        .assert_playlist_entry(0, "file:///music/unknown.ogg")
        .assert_playlist_entry(1, "spotify:track:one")
        .assert_playlist_entry_selected(0, true)
        .assert_playlist_footer_info("?/2:30+")
        .drag_playlist_row(2, 1)
        .assert_playlist_entry(0, "file:///music/unknown.ogg")
        .assert_playlist_entry(1, "spotify:track:two")
        .assert_playlist_entry(2, "spotify:track:one");
}

#[test]
fn clicked_playlist_rows_update_single_selection() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.drop_on_playlist([
        "file:///music/4-zulu.ogg",
        "file:///music/3-charlie.ogg",
        "file:///music/2-bravo.ogg",
        "file:///music/1-alpha.ogg",
    ])
    .click_playlist_row(0)
    .assert_playlist_entry_selected(0, true)
    .click_playlist_row(2)
    .assert_playlist_entry_selected(0, false)
    .assert_playlist_entry_selected(2, true)
    .click_playlist_row(3)
    .assert_playlist_entry_selected(2, false)
    .assert_playlist_entry_selected(3, true);
}

#[test]
fn vim_playlist_keys_move_selection_and_play_selected_entry() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_vim_playlist_navigation(true),
    );

    app.drop_on_playlist([
        "file:///music/one.ogg",
        "file:///music/two.ogg",
        "file:///music/three.ogg",
    ])
    .press_shortcut(Shortcut::PlaylistDown)
    .assert_playlist_entry_selected(1, true)
    .press_shortcut(Shortcut::PlaylistDown)
    .assert_playlist_entry_selected(1, false)
    .assert_playlist_entry_selected(2, true)
    .press_shortcut(Shortcut::PlaylistUp)
    .assert_playlist_entry_selected(1, true)
    .press_shortcut(Shortcut::PlaylistUp)
    .assert_playlist_entry_selected(0, true)
    .press_shortcut(Shortcut::PlaylistPlay)
    .assert_player_state(PlayerState::Playing)
    .assert_playlist_position(Some(0))
    .assert_current_playlist_entry("file:///music/one.ogg")
    .press_shortcut(Shortcut::PlaylistDown)
    .press_shortcut(Shortcut::PlaylistDown)
    .press_shortcut(Shortcut::PlaylistDown)
    .assert_playlist_entry_selected(2, true)
    .press_shortcut(Shortcut::PlaylistPlay)
    .assert_playlist_position(Some(2))
    .assert_current_playlist_entry("file:///music/three.ogg");

    let mut disabled = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    disabled
        .drop_on_playlist([
            "file:///music/disabled-one.ogg",
            "file:///music/disabled-two.ogg",
        ])
        .press_shortcut(Shortcut::PlaylistDown)
        .assert_playlist_entry_selected(1, false)
        .press_shortcut(Shortcut::PlaylistPlay)
        .assert_player_state(PlayerState::Stopped)
        .assert_playlist_position(None);
}

#[test]
fn ctrl_clicking_playlist_rows_toggles_multiple_selection() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.drop_on_playlist([
        "file:///music/4-zulu.ogg",
        "file:///music/3-charlie.ogg",
        "file:///music/2-bravo.ogg",
        "file:///music/1-alpha.ogg",
    ])
    .click_playlist_row(0)
    .assert_playlist_entry_selected(0, true)
    .ctrl_click_playlist_row(2)
    .assert_playlist_entry_selected(0, true)
    .assert_playlist_entry_selected(1, false)
    .assert_playlist_entry_selected(2, true)
    .assert_playlist_entry_selected(3, false)
    .ctrl_click_playlist_row(0)
    .assert_playlist_entry_selected(0, false)
    .assert_playlist_entry_selected(2, true)
    .click_playlist_row(3)
    .assert_playlist_entry_selected(2, false)
    .assert_playlist_entry_selected(3, true);
}

#[test]
fn double_clicking_playlist_row_starts_that_entry() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.drop_on_playlist([
        "file:///music/first.ogg",
        "file:///music/second.ogg",
        "file:///music/third.ogg",
    ])
    .assert_player_state(PlayerState::Stopped)
    .double_click_playlist_row(1)
    .assert_playlist_entry_selected(1, true)
    .assert_playlist_position(Some(1))
    .assert_current_playlist_entry("file:///music/second.ogg")
    .assert_last_playback_request(Some("file:///music/second.ogg"))
    .assert_player_state(PlayerState::Playing);
}

#[test]
fn playlist_sort_e2e_supports_title_and_date_keys() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_spotify_entry("spotify:track:z", "Zulu", 1_000)
        .add_spotify_entry("spotify:track:a", "alpha", 1_000)
        .add_spotify_entry("spotify:track:e", "Echo", 1_000)
        .sort_playlist_by(PlaylistSortKey::Title)
        .assert_playlist_entry(0, "spotify:track:a")
        .assert_playlist_title(0, "alpha")
        .assert_playlist_entry(1, "spotify:track:e")
        .assert_playlist_title(1, "Echo")
        .assert_playlist_entry(2, "spotify:track:z")
        .assert_playlist_title(2, "Zulu");

    let music_dir = unique_temp_dir("xmms-rs-e2e-sort-date");
    fs::create_dir_all(&music_dir).unwrap();
    let older = music_dir.join("older.ogg");
    let newer = music_dir.join("newer.ogg");
    fs::write(&older, b"old").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&newer, b"new").unwrap();

    let mut app = UiE2e::start_player(PlayerSettings::default());
    app.drop_on_playlist([file_uri(&newer), file_uri(&older)])
        .sort_playlist_by(PlaylistSortKey::Date)
        .assert_playlist_entry(0, &file_uri(&older))
        .assert_playlist_entry(1, &file_uri(&newer));

    fs::remove_dir_all(music_dir).unwrap();
}

#[test]
fn selected_playlist_sort_e2e_reorders_only_selected_rows() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist([
        "file:///music/4-zulu.ogg",
        "file:///music/3-charlie.ogg",
        "file:///music/2-bravo.ogg",
        "file:///music/1-alpha.ogg",
    ])
    .click(MainTarget::NEXT)
    .assert_playlist_position(Some(0))
    .select_playlist_entry(0)
    .select_playlist_entry(2)
    .select_playlist_entry(3)
    .sort_selected_playlist_by(PlaylistSortKey::Filename)
    .assert_playlist_entry(0, "file:///music/1-alpha.ogg")
    .assert_playlist_entry(1, "file:///music/3-charlie.ogg")
    .assert_playlist_entry(2, "file:///music/2-bravo.ogg")
    .assert_playlist_entry(3, "file:///music/4-zulu.ogg")
    .assert_playlist_position(Some(3));
}

#[test]
fn playlist_reverse_and_randomize_e2e_preserve_current_entry() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist([
        "file:///music/one.ogg",
        "file:///music/two.ogg",
        "file:///music/three.ogg",
        "file:///music/four.ogg",
    ])
    .click(MainTarget::NEXT)
    .click(MainTarget::NEXT)
    .assert_playlist_position(Some(1))
    .assert_playlist_entry(1, "file:///music/two.ogg")
    .reverse_playlist()
    .assert_playlist_entry(0, "file:///music/four.ogg")
    .assert_playlist_entry(1, "file:///music/three.ogg")
    .assert_playlist_entry(2, "file:///music/two.ogg")
    .assert_playlist_entry(3, "file:///music/one.ogg")
    .assert_playlist_position(Some(2))
    .randomize_playlist()
    .assert_playlist_len(4)
    .assert_current_playlist_entry("file:///music/two.ogg");
}

#[test]
fn playlist_misc_sort_menu_actions_cover_each_list_sort() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.drop_on_playlist([
        "file:///music/Beta/b_song.ogg",
        "file:///music/Alpha/c_song.ogg",
        "file:///music/Gamma/a_song.ogg",
    ])
    .click_panel(PanelTarget::PlaylistMisc)
    .activate_playlist_menu_item(0)
    .activate_playlist_sort_action(PlaylistSortAction::ListByFilename)
    .assert_playlist_entry(0, "file:///music/Gamma/a_song.ogg")
    .assert_playlist_entry(1, "file:///music/Beta/b_song.ogg")
    .assert_playlist_entry(2, "file:///music/Alpha/c_song.ogg")
    .activate_playlist_sort_action(PlaylistSortAction::ListByPath)
    .assert_playlist_entry(0, "file:///music/Alpha/c_song.ogg")
    .assert_playlist_entry(1, "file:///music/Beta/b_song.ogg")
    .assert_playlist_entry(2, "file:///music/Gamma/a_song.ogg");

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.add_spotify_entry("spotify:track:z", "Zulu", 1_000)
        .add_spotify_entry("spotify:track:a", "alpha", 1_000)
        .add_spotify_entry("spotify:track:e", "Echo", 1_000)
        .activate_playlist_sort_action(PlaylistSortAction::ListByTitle)
        .assert_playlist_entry(0, "spotify:track:a")
        .assert_playlist_entry(1, "spotify:track:e")
        .assert_playlist_entry(2, "spotify:track:z");

    let music_dir = unique_temp_dir("xmms-rs-misc-sort-date");
    fs::create_dir_all(&music_dir).unwrap();
    let older = music_dir.join("older.ogg");
    let newer = music_dir.join("newer.ogg");
    fs::write(&older, b"old").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&newer, b"new").unwrap();

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.drop_on_playlist([file_uri(&newer), file_uri(&older)])
        .activate_playlist_sort_action(PlaylistSortAction::ListByDate)
        .assert_playlist_entry(0, &file_uri(&older))
        .assert_playlist_entry(1, &file_uri(&newer))
        .activate_playlist_sort_action(PlaylistSortAction::ReverseList)
        .assert_playlist_entry(0, &file_uri(&newer))
        .assert_playlist_entry(1, &file_uri(&older))
        .activate_playlist_sort_action(PlaylistSortAction::RandomizeList)
        .assert_playlist_len(2);

    fs::remove_dir_all(music_dir).unwrap();
}

#[test]
fn playlist_misc_sort_menu_actions_cover_each_selected_sort() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.add_spotify_entry("spotify:track:z", "Zulu", 1_000)
        .add_spotify_entry("spotify:track:middle", "middle", 1_000)
        .add_spotify_entry("spotify:track:a", "alpha", 1_000)
        .select_playlist_entry(0)
        .select_playlist_entry(2)
        .activate_playlist_sort_action(PlaylistSortAction::SelectionByTitle)
        .assert_playlist_entry(0, "spotify:track:a")
        .assert_playlist_entry(1, "spotify:track:middle")
        .assert_playlist_entry(2, "spotify:track:z");

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.drop_on_playlist([
        "file:///music/4-zulu.ogg",
        "file:///music/3-charlie.ogg",
        "file:///music/2-bravo.ogg",
        "file:///music/1-alpha.ogg",
    ])
    .select_playlist_entry(0)
    .select_playlist_entry(2)
    .select_playlist_entry(3)
    .activate_playlist_sort_action(PlaylistSortAction::SelectionByFilename)
    .assert_playlist_entry(0, "file:///music/1-alpha.ogg")
    .assert_playlist_entry(1, "file:///music/3-charlie.ogg")
    .assert_playlist_entry(2, "file:///music/2-bravo.ogg")
    .assert_playlist_entry(3, "file:///music/4-zulu.ogg")
    .activate_playlist_sort_action(PlaylistSortAction::SelectionByPath)
    .assert_playlist_entry(0, "file:///music/1-alpha.ogg")
    .assert_playlist_entry(1, "file:///music/3-charlie.ogg")
    .assert_playlist_entry(2, "file:///music/2-bravo.ogg")
    .assert_playlist_entry(3, "file:///music/4-zulu.ogg")
    .activate_playlist_sort_action(PlaylistSortAction::SelectionByDate)
    .assert_playlist_len(4);
}

#[test]
fn playlist_misc_file_info_and_options_actions_are_wired() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.add_spotify_entry("spotify:track:one", "Info Target", 1_000)
        .add_spotify_entry("spotify:track:two", "Other Track", 1_000)
        .select_playlist_entry(0)
        .click_panel(PanelTarget::PlaylistMisc)
        .activate_playlist_menu_item(1)
        .assert_last_playlist_file_info("Info Target")
        .click_panel(PanelTarget::PlaylistMisc)
        .activate_playlist_menu_item(2)
        .assert_playlist_options_opened();
}

#[test]
fn playlist_duration_indexing_e2e_updates_missing_file_entries_only() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_playlist(["file:///music/a.ogg", "file:///music/b.ogg"])
        .add_spotify_entry("spotify:track:skip", "Spotify", 123_000)
        .add_podcast_entry(
            "https://example.test/episode.mp3",
            "Episode",
            "https://example.test/feed.xml",
            "episode-1",
        )
        .index_missing_playlist_durations()
        .assert_playlist_length_ms(0, 1_000)
        .assert_playlist_title(0, "Indexed 1")
        .assert_playlist_length_ms(1, 2_000)
        .assert_playlist_title(1, "Indexed 2")
        .assert_playlist_length_ms(2, 123_000)
        .assert_playlist_title(2, "Spotify")
        .assert_playlist_length_ms(3, -1)
        .assert_playlist_title(3, "Episode");
}

#[test]
fn playlist_duration_results_are_applied_asynchronously_from_timer() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.drop_on_playlist(["file:///music/async-a.ogg", "file:///music/async-b.ogg"])
        .assert_playlist_length_ms(0, -1)
        .assert_playlist_length_ms(1, -1)
        .queue_playlist_duration_result(1, 42_000, Some("Async B"))
        .assert_playlist_length_ms(1, -1)
        .update_timer_tick(100)
        .assert_playlist_length_ms(0, -1)
        .assert_playlist_length_ms(1, 42_000)
        .assert_playlist_title(1, "Async B");
}

#[test]
fn update_timer_advances_position_while_playing_only() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_position(0)
        .assert_main_time_digits([10, 10, 10, 10, 10])
        .press_shortcut(Shortcut::Play)
        .assert_player_state(PlayerState::Stopped)
        .update_timer_tick(1_000)
        .assert_position(0)
        .assert_main_time_digits([10, 10, 10, 10, 10])
        .add_spotify_entry("spotify:track:one", "Song", 10_000)
        .press_shortcut(Shortcut::PlayFirst)
        .assert_player_state(PlayerState::Playing)
        .update_timer_tick(5_000)
        .assert_position(109)
        .assert_main_time_digits([10, 0, 0, 0, 5])
        .press_shortcut(Shortcut::Pause)
        .update_timer_tick(1_000)
        .assert_position(109)
        .press_shortcut(Shortcut::Stop)
        .update_timer_tick(1_000)
        .assert_position(0)
        .assert_main_time_digits([10, 10, 10, 10, 10]);
}

#[test]
fn skin_browser_discovers_user_and_system_skins_sorted_like_c() {
    let root = unique_temp_dir("xmms-rs-skin-browser-discover");
    let user_skins = root.join("user").join("xmms").join("Skins");
    let system_skins = root.join("system").join("Skins");
    fs::create_dir_all(user_skins.join("Zed Skin")).unwrap();
    fs::create_dir_all(user_skins.join(".hidden")).unwrap();
    fs::create_dir_all(system_skins.join("Classic")).unwrap();
    fs::write(user_skins.join("Blue.wsz"), b"archive").unwrap();
    fs::write(user_skins.join("not-a-skin.txt"), b"ignored").unwrap();

    let mut app = UiE2e::start_player(PlayerSettings::default());
    app.open_preferences_page(PreferencesPage::Fonts)
        .click_menu_item(MenuItem::SkinBrowser)
        .assert_window_visible(Window::SkinBrowser)
        .scan_skin_browser_dirs(&[user_skins.clone(), system_skins.clone()])
        .assert_skin_browser_entries(&["Blue", "Classic", "Zed Skin"])
        .assert_selected_skin_index(0)
        .assert_selected_skin_path(None);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn skin_browser_selects_default_and_installed_skin_paths() {
    let root = unique_temp_dir("xmms-rs-skin-browser-select");
    let skins = root.join("Skins");
    write_one_pixel_skin(&skins.join("Classic"), "#010203");
    fs::create_dir_all(&skins).unwrap();
    write_one_pixel_wsz(&skins.join("Packed.wsz"), "#040506");

    let classic = skins.join("Classic");
    let packed = skins.join("Packed.wsz");
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.scan_skin_browser_dirs(&[skins.clone()])
        .assert_skin_browser_entries(&["Classic", "Packed"])
        .select_skin_browser_index(1)
        .assert_selected_skin_index(1)
        .assert_selected_skin_path(Some(classic.as_path()))
        .assert_active_skin_pixel(SkinPixmapKind::Main, 0, 0, 0xff010203)
        .assert_skin_reload_count(1)
        .select_skin_browser_index(2)
        .assert_selected_skin_index(2)
        .assert_selected_skin_path(Some(packed.as_path()))
        .assert_active_skin_pixel(SkinPixmapKind::Main, 0, 0, 0xff040506)
        .assert_skin_reload_count(2)
        .select_skin_browser_index(0)
        .assert_selected_skin_index(0)
        .assert_selected_skin_path(None)
        .assert_active_skin_pixel(SkinPixmapKind::Main, 0, 0, 0xff000000)
        .assert_skin_reload_count(3)
        .reload_skin()
        .assert_skin_reload_count(4);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn startup_config_loads_directory_and_wsz_skins() {
    let root = unique_temp_dir("xmms-rs-skin-startup");
    let dir_skin = root.join("base-2.9.1");
    let wsz_skin = root.join("base-2.9.1.wsz");
    write_one_pixel_skin(&dir_skin, "#070809");
    fs::create_dir_all(&root).unwrap();
    write_one_pixel_wsz(&wsz_skin, "#0a0b0c");

    UiE2e::start_player(PlayerSettings::default().with_skin(dir_skin.display().to_string()))
        .assert_active_skin_pixel(SkinPixmapKind::Main, 0, 0, 0xff070809);
    UiE2e::start_player(PlayerSettings::default().with_skin(wsz_skin.display().to_string()))
        .assert_active_skin_pixel(SkinPixmapKind::Main, 0, 0, 0xff0a0b0c);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn skin_browser_search_path_covers_user_legacy_system_and_env_dirs() {
    let root = unique_temp_dir("xmms-rs-skin-browser-paths");
    let user_config = root.join("config");
    let home = root.join("home");
    let system = root.join("system").join("Skins");
    let env_one = root.join("env-one");
    let env_two = root.join("env-two");

    for dir in [
        user_config.join("xmms").join("Skins"),
        home.join(".xmms").join("Skins"),
        system.clone(),
        env_one.clone(),
        env_two.clone(),
    ] {
        fs::create_dir_all(dir.join("Skin")).unwrap();
    }

    let dirs = skin_browser_search_dirs(
        &user_config,
        &home,
        &system,
        Some(&format!("{}:{}", env_one.display(), env_two.display())),
    );
    let mut app = UiE2e::start_player(PlayerSettings::default());
    app.scan_skin_browser_dirs(&dirs)
        .assert_skin_browser_entries(&["Skin", "Skin", "Skin", "Skin", "Skin"]);

    assert_eq!(dirs[0], user_config.join("xmms").join("Skins"));
    assert_eq!(dirs[1], home.join(".xmms").join("Skins"));
    assert_eq!(dirs[2], system);
    assert_eq!(dirs[3], env_one);
    assert_eq!(dirs[4], env_two);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn output_device_picker_groups_and_deduplicates_system_devices() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.open_output_device_picker()
        .assert_window_visible(Window::OutputDevicePicker)
        .set_output_devices(
            vec![
                OutputDevice::system("speaker", "Speakers", "pipewire-proplist", false),
                OutputDevice::system("speaker", "Speakers via Pulse", "pulse-proplist", false),
                OutputDevice::system("raw", "Raw ALSA", "alsa-proplist", false),
                OutputDevice::system("cast", "Living Room", "pipewire-proplist", true),
            ],
            vec![],
        )
        .assert_local_output_devices(&["Speakers"])
        .assert_network_output_devices(&["Living Room"]);
}

#[test]
fn output_device_picker_preserves_automatic_system_default() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.set_output_devices(
        vec![OutputDevice::system(
            "speaker",
            "Speakers",
            "pipewire-proplist",
            false,
        )],
        vec![],
    )
    .assert_selected_output_device(None)
    .select_output_device(OutputDeviceSelection::System("speaker"))
    .assert_selected_output_device(Some("speaker"))
    .assert_output_switch_count(1)
    .select_output_device(OutputDeviceSelection::Automatic)
    .assert_selected_output_device(None)
    .assert_output_switch_count(2);
}

#[test]
fn output_device_picker_switches_system_device_without_stopping_playback() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_spotify_entry("spotify:track:output", "Output", 10_000)
        .press_shortcut(Shortcut::Play)
        .assert_player_state(PlayerState::Playing)
        .set_output_devices(
            vec![OutputDevice::system(
                "headphones",
                "Headphones",
                "pipewire-proplist",
                false,
            )],
            vec![],
        )
        .select_output_device(OutputDeviceSelection::System("headphones"))
        .assert_selected_output_device(Some("headphones"))
        .assert_player_state(PlayerState::Playing)
        .assert_output_switch_count(1);
}

#[test]
fn output_device_picker_lists_and_selects_spotify_devices() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.set_output_devices(
        vec![],
        vec![
            OutputDevice::spotify("phone", "Phone", "Smartphone"),
            OutputDevice::spotify("desktop", "Desktop", "Computer"),
        ],
    )
    .assert_spotify_output_devices(&["Phone", "Desktop"])
    .select_output_device(OutputDeviceSelection::Spotify("desktop"))
    .assert_selected_spotify_output_device(Some("desktop"))
    .assert_selected_output_device(None)
    .assert_output_switch_count(1);
}

#[test]
fn mpris_root_and_player_properties_match_xmms_contract() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_volume(40));

    app.add_playlist_uri("file:///music/one.ogg")
        .assert_mpris_identity()
        .assert_mpris_dbus_introspection()
        .assert_mpris_playback_status("Stopped")
        .assert_mpris_volume(0.4)
        .assert_mpris_position_us(0)
        .assert_mpris_metadata(
            "/org/xmms/Track/0",
            Some("one"),
            Some("file:///music/one.ogg"),
            None,
        )
        .press_shortcut(Shortcut::Play)
        .assert_mpris_playback_status("Playing")
        .press_shortcut(Shortcut::Pause)
        .assert_mpris_playback_status("Paused");
}

#[test]
fn mpris_volume_seek_and_set_position_update_player_state() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_spotify_entry("spotify:track:mpris", "MPRIS", 10_000)
        .set_mpris_volume(0.25)
        .assert_volume(25)
        .assert_mpris_volume(0.25)
        .execute_mpris_command(MprisCommand::Play)
        .assert_player_state(PlayerState::Playing)
        .execute_mpris_command(MprisCommand::Seek {
            offset_us: 5_000_000,
        })
        .assert_position(109)
        .assert_mpris_position_us(5_000_000)
        .assert_mpris_event(MprisEvent::Seeked(5_000_000))
        .execute_mpris_command(MprisCommand::SetPosition {
            track_id: "/org/xmms/Track/0".to_string(),
            position_us: 2_000_000,
        })
        .assert_position(43)
        .assert_mpris_position_us(2_000_000)
        .assert_mpris_event(MprisEvent::Seeked(2_000_000));
}

#[test]
fn mpris_transport_methods_drive_playlist_and_playback() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_playlist_uri("file:///music/one.ogg")
        .add_playlist_uri("file:///music/two.ogg")
        .execute_mpris_command(MprisCommand::OpenUri(
            "file:///music/opened.ogg".to_string(),
        ))
        .assert_playlist_len(1)
        .assert_current_playlist_entry("file:///music/opened.ogg")
        .assert_player_state(PlayerState::Playing)
        .assert_mpris_event(MprisEvent::MetadataChanged)
        .execute_mpris_command(MprisCommand::Pause)
        .assert_player_state(PlayerState::Paused)
        .execute_mpris_command(MprisCommand::PlayPause)
        .assert_player_state(PlayerState::Playing)
        .execute_mpris_command(MprisCommand::Stop)
        .assert_player_state(PlayerState::Stopped)
        .assert_mpris_event(MprisEvent::PlaybackStatusChanged);
}

#[test]
fn mpris_raise_quit_and_next_previous_methods_emit_expected_state() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_playlist_uri("file:///music/one.ogg")
        .add_playlist_uri("file:///music/two.ogg")
        .execute_mpris_command(MprisCommand::Play)
        .execute_mpris_command(MprisCommand::Next)
        .assert_playlist_position(Some(1))
        .execute_mpris_command(MprisCommand::Previous)
        .assert_playlist_position(Some(0))
        .execute_mpris_command(MprisCommand::Raise)
        .assert_mpris_event(MprisEvent::Raised)
        .execute_mpris_command(MprisCommand::Quit)
        .assert_mpris_quit_requested(true)
        .assert_mpris_event(MprisEvent::QuitRequested);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}

fn one_pixel_xpm(color: &str) -> String {
    format!(
        r#"/* XPM */
static char * main_xpm[] = {{
"1 1 1 1",
". c {color}",
"."}};
"#
    )
}

fn write_one_pixel_skin(dir: &Path, color: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("main.xpm"), one_pixel_xpm(color)).unwrap();
}

fn write_one_pixel_wsz(path: &Path, color: &str) {
    let file = File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    archive.start_file("base-2.9.1/main.xpm", options).unwrap();
    archive.write_all(one_pixel_xpm(color).as_bytes()).unwrap();
    archive.finish().unwrap();
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

#[test]
fn transport_buttons_update_player_state_and_position() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click(MainTarget::PLAY)
        .assert_player_state(PlayerState::Stopped)
        .add_spotify_entry("spotify:track:transport", "Transport", 10_000)
        .click(MainTarget::PLAY)
        .assert_player_state(PlayerState::Playing);

    app.click(MainTarget::PAUSE)
        .assert_player_state(PlayerState::Paused);

    app.click(MainTarget::PAUSE)
        .assert_player_state(PlayerState::Playing);

    app.click(MainTarget::position(219)).assert_position(219);

    app.click(MainTarget::PREVIOUS).assert_position(0);

    app.click(MainTarget::position(219)).assert_position(219);

    app.click(MainTarget::NEXT).assert_position(0);

    app.click(MainTarget::PLAY)
        .click(MainTarget::STOP)
        .assert_player_state(PlayerState::Stopped)
        .assert_position(0);

    app.click(MainTarget::EJECT)
        .assert_window_visible(Window::Player)
        .assert_player_state(PlayerState::Stopped)
        .assert_file_dialog_visible();
}

#[test]
fn playlist_footer_transport_buttons_update_player_state_and_position() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.add_spotify_entry("spotify:track:playlist-footer-one", "Footer One", 10_000)
        .add_spotify_entry("spotify:track:playlist-footer-two", "Footer Two", 12_000)
        .click_panel(PanelTarget::PlaylistPlay)
        .assert_player_state(PlayerState::Playing)
        .assert_playlist_position(Some(0))
        .click_panel(PanelTarget::PlaylistPause)
        .assert_player_state(PlayerState::Paused)
        .click_panel(PanelTarget::PlaylistPause)
        .assert_player_state(PlayerState::Playing)
        .click_panel(PanelTarget::PlaylistNext)
        .assert_playlist_position(Some(1))
        .assert_player_state(PlayerState::Playing)
        .click_panel(PanelTarget::PlaylistPrevious)
        .assert_playlist_position(Some(0))
        .click_panel(PanelTarget::PlaylistStop)
        .assert_player_state(PlayerState::Stopped)
        .click_panel(PanelTarget::PlaylistEject)
        .assert_file_dialog_visible();
}

#[test]
fn docked_playlist_footer_transport_buttons_use_current_geometry() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.add_spotify_entry(
        "spotify:track:docked-playlist-footer",
        "Docked Footer",
        10_000,
    )
    .click_docked_panel(PanelTarget::PlaylistPlay)
    .assert_player_state(PlayerState::Playing)
    .click_docked_panel(PanelTarget::PlaylistStop)
    .assert_player_state(PlayerState::Stopped);
}

#[test]
fn playlist_footer_scroll_buttons_update_scroll_offset() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    for index in 0..30 {
        app.accept_open_location(&format!("file:///tmp/footer-scroll-{index:02}.mp3"));
    }

    app.assert_playlist_scroll_offset(0)
        .assert_playlist_scrollbar_visible(true)
        .click_panel(PanelTarget::PlaylistScrollDown)
        .assert_playlist_scroll_offset(1)
        .click_panel(PanelTarget::PlaylistScrollUp)
        .assert_playlist_scroll_offset(0);
}

#[test]
fn mono_stereo_indicator_tracks_stream_channel_count() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_main_channels(0)
        .set_stream_channels(2)
        .assert_main_channels(2)
        .set_stream_channels(1)
        .assert_main_channels(1);
}

#[test]
fn shuffle_and_repeat_buttons_toggle_playlist_modes() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_shuffle(false)
        .click(MainTarget::SHUFFLE)
        .assert_shuffle(true)
        .click(MainTarget::SHUFFLE)
        .assert_shuffle(false);

    app.assert_repeat(false)
        .click(MainTarget::REPEAT)
        .assert_repeat(true)
        .click(MainTarget::REPEAT)
        .assert_repeat(false);
}

#[test]
fn volume_balance_and_position_sliders_update_player_values() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click(MainTarget::volume(0)).assert_volume(0);
    app.click(MainTarget::volume(51)).assert_volume(100);

    app.click(MainTarget::balance(0)).assert_balance(-100);
    app.click(MainTarget::balance(12)).assert_balance(0);
    app.click(MainTarget::balance(24)).assert_balance(100);

    app.add_spotify_entry("spotify:track:slider", "Slider", 10_000)
        .press_shortcut(Shortcut::PlayFirst)
        .click(MainTarget::position(0))
        .assert_position(0);
    app.click(MainTarget::position(100)).assert_position(99);
    app.click(MainTarget::position(219)).assert_position(219);
}

#[test]
fn playlist_button_opens_and_closes_playlist_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_detached(true));

    app.assert_window_visible(Window::Player)
        .assert_window_hidden(Window::Playlist);

    app.click(MainTarget::PLAYLIST)
        .assert_window_visible(Window::Playlist);

    app.click(MainTarget::PLAYLIST)
        .assert_window_hidden(Window::Playlist);
}

#[test]
fn equalizer_button_opens_and_closes_equalizer_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_equalizer_detached(true));

    app.assert_window_visible(Window::Player)
        .assert_window_hidden(Window::Equalizer);

    app.click(MainTarget::EQUALIZER)
        .assert_window_visible(Window::Equalizer);

    app.click(MainTarget::EQUALIZER)
        .assert_window_hidden(Window::Equalizer);
}

#[test]
fn equalizer_top_right_buttons_shade_and_close_equalizer_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_equalizer_detached(true));

    app.click(MainTarget::EQUALIZER)
        .assert_window_visible(Window::Equalizer)
        .assert_equalizer_unshaded();

    app.click_panel(PanelTarget::EqualizerShade)
        .assert_window_visible(Window::Equalizer)
        .assert_equalizer_shaded();

    app.click_panel(PanelTarget::EqualizerShade)
        .assert_window_visible(Window::Equalizer)
        .assert_equalizer_unshaded();

    app.click_panel(PanelTarget::EqualizerClose)
        .assert_window_hidden(Window::Equalizer);
}

#[test]
fn shaded_equalizer_volume_and_balance_sliders_update_shared_player_state() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_equalizer_visible(true));

    app.click_panel(PanelTarget::EqualizerShade)
        .assert_equalizer_shaded()
        .drag_shaded_equalizer_volume(94)
        .assert_volume(100)
        .drag_shaded_equalizer_volume(0)
        .assert_volume(0)
        .drag_shaded_equalizer_balance(0)
        .assert_balance(-100)
        .drag_shaded_equalizer_balance(19)
        .assert_balance(0)
        .drag_shaded_equalizer_balance(39)
        .assert_balance(100);
}

#[test]
fn equalizer_buttons_sliders_and_presets_update_state() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_equalizer_visible(true));

    app.assert_equalizer_active(true)
        .click_panel(PanelTarget::EqualizerOn)
        .assert_equalizer_active(false)
        .click_panel(PanelTarget::EqualizerOn)
        .assert_equalizer_active(true);

    app.assert_equalizer_automatic(false)
        .click_panel(PanelTarget::EqualizerAuto)
        .assert_equalizer_automatic(true);

    app.drag_equalizer_preamp(25)
        .assert_equalizer_preamp_position(24)
        .drag_equalizer_band(0, 10)
        .assert_equalizer_band_position(0, 10)
        .drag_equalizer_band(9, 80)
        .assert_equalizer_band_position(9, 80);

    app.click_panel(PanelTarget::EqualizerPresets)
        .assert_equalizer_presets_pressed(false)
        .apply_equalizer_preset(3)
        .assert_equalizer_preamp_position(50)
        .assert_equalizer_band_position(0, 30)
        .assert_equalizer_band_position(4, 60)
        .assert_equalizer_band_position(9, 30);
}

#[test]
fn equalizer_all_bands_expose_c_compatible_gstreamer_db_mapping() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_equalizer_visible(true));

    app.drag_equalizer_preamp(0)
        .assert_equalizer_preamp_position(0)
        .assert_equalizer_preamp_db(20.0);

    let requested_positions = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90];
    let snapped_positions = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90];
    for (band, (requested, snapped)) in requested_positions
        .into_iter()
        .zip(snapped_positions)
        .enumerate()
    {
        app.drag_equalizer_band(band, requested)
            .assert_equalizer_band_position(band, snapped)
            .assert_equalizer_band_db(band, (50 - snapped) as f64 * 20.0 / 50.0);
    }

    app.assert_equalizer_gstreamer_band_db_values([
        20.0, 16.0, 12.0, 8.0, 4.0, 0.0, -4.0, -8.0, -12.0, -16.0,
    ]);

    app.click_panel(PanelTarget::EqualizerOn)
        .assert_equalizer_active(false)
        .assert_equalizer_gstreamer_band_db_values([0.0; 10]);
}

#[test]
fn playlist_top_right_buttons_shade_and_close_playlist_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_detached(true));

    app.click(MainTarget::PLAYLIST)
        .assert_window_visible(Window::Playlist)
        .assert_playlist_unshaded();

    app.click_panel(PanelTarget::PlaylistShade)
        .assert_window_visible(Window::Playlist)
        .assert_playlist_shaded();

    app.click_panel(PanelTarget::PlaylistShade)
        .assert_window_visible(Window::Playlist)
        .assert_playlist_unshaded();

    app.click_panel(PanelTarget::PlaylistClose)
        .assert_window_hidden(Window::Playlist);
}

#[test]
fn floating_panel_titlebars_are_draggable_without_breaking_buttons() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_equalizer_visible(true),
    );

    app.assert_panel_title_draggable(PanelKind::Equalizer)
        .assert_panel_title_button_not_draggable(PanelKind::Equalizer)
        .assert_panel_title_draggable(PanelKind::Playlist)
        .assert_panel_title_button_not_draggable(PanelKind::Playlist);
}

#[test]
fn docked_panel_size_respects_equalizer_detached_and_docked_state() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_equalizer_visible(true)
            .with_playlist_visible(true),
    );

    app.assert_panel_detached(PanelKind::Equalizer, false)
        .assert_docked_panel_size((
            MAIN_WINDOW_WIDTH,
            MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT,
        ))
        .detach_panel(PanelKind::Equalizer)
        .assert_panel_detached(PanelKind::Equalizer, true)
        .assert_docked_panel_size((
            MAIN_WINDOW_WIDTH,
            MAIN_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT,
        ))
        .dock_panel(PanelKind::Equalizer)
        .assert_panel_detached(PanelKind::Equalizer, false)
        .assert_docked_panel_size((
            MAIN_WINDOW_WIDTH,
            MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT,
        ));
}

#[test]
fn docked_panel_size_respects_playlist_detached_and_docked_state() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_equalizer_visible(true)
            .with_playlist_visible(true),
    );

    app.detach_panel(PanelKind::Playlist)
        .assert_panel_detached(PanelKind::Playlist, true)
        .assert_docked_panel_size((
            MAIN_WINDOW_WIDTH,
            MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT,
        ))
        .dock_panel(PanelKind::Playlist)
        .assert_panel_detached(PanelKind::Playlist, false)
        .assert_docked_panel_size((
            MAIN_WINDOW_WIDTH,
            MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT,
        ));
}

#[test]
fn docking_resized_floating_playlist_resets_width_but_preserves_height() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_playlist_detached(true),
    );

    app.resize_playlist(325, 280)
        .assert_playlist_size(325, 261)
        .set_preference_playlist_docked(true)
        .assert_panel_detached(PanelKind::Playlist, false)
        .assert_playlist_size(275, 261)
        .assert_docked_panel_size((275, 116 + 261));
}

#[test]
fn visualization_modes_can_be_selected_from_rust_e2e() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_visualization_mode(VisMode::Analyzer)
        .set_visualization_mode(VisMode::Scope)
        .assert_visualization_mode(VisMode::Scope)
        .set_visualization_mode(VisMode::Off)
        .assert_visualization_mode(VisMode::Off)
        .set_visualization_mode(VisMode::Milkdrop)
        .assert_visualization_mode(VisMode::Milkdrop)
        .set_visualization_mode(VisMode::Analyzer)
        .assert_visualization_mode(VisMode::Analyzer);
}

#[test]
fn visualization_analyzer_styles_can_be_selected_from_rust_e2e() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default().with_visualization_analyzer_style(VisAnalyzerStyle::Lines),
    );

    app.assert_visualization_analyzer_style(VisAnalyzerStyle::Lines)
        .set_visualization_analyzer_style(VisAnalyzerStyle::Bars)
        .assert_visualization_analyzer_style(VisAnalyzerStyle::Bars);
}

#[test]
fn visualization_analyzer_modes_can_be_selected_from_rust_e2e() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default().with_visualization_analyzer_mode(VisAnalyzerMode::Fire),
    );

    app.assert_visualization_analyzer_mode(VisAnalyzerMode::Fire)
        .set_visualization_analyzer_mode(VisAnalyzerMode::VerticalLines)
        .assert_visualization_analyzer_mode(VisAnalyzerMode::VerticalLines)
        .set_visualization_analyzer_mode(VisAnalyzerMode::Normal)
        .assert_visualization_analyzer_mode(VisAnalyzerMode::Normal);
}

#[test]
fn visualization_scope_modes_can_be_selected_from_rust_e2e() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_visualization_mode(VisMode::Scope)
            .with_visualization_scope_mode(VisScopeMode::Dot),
    );

    app.assert_visualization_mode(VisMode::Scope)
        .assert_visualization_scope_mode(VisScopeMode::Dot)
        .set_visualization_scope_mode(VisScopeMode::Solid)
        .assert_visualization_scope_mode(VisScopeMode::Solid)
        .set_visualization_scope_mode(VisScopeMode::Line)
        .assert_visualization_scope_mode(VisScopeMode::Line);
}

#[test]
fn visualization_peaks_and_falloff_can_be_selected_from_rust_e2e() {
    let mut app =
        UiE2e::start_player(PlayerSettings::default().with_visualization_peaks_enabled(false));

    app.assert_visualization_peaks_enabled(false)
        .assert_visualization_peak_cleared()
        .set_visualization_peaks_enabled(true)
        .assert_visualization_peaks_enabled(true)
        .set_visualization_falloff(VisFalloffSpeed::Fastest, VisFalloffSpeed::Slowest)
        .feed_visualization_data(3, 1.0)
        .tick_visualization(100)
        .assert_visualization_band_at_least(3, 0.8);
}

#[test]
fn visualization_windowshade_vu_mode_can_be_selected_from_rust_e2e() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default().with_visualization_vu_mode(VisVuMode::Smooth),
    );

    app.assert_visualization_vu_mode(VisVuMode::Smooth)
        .set_visualization_vu_mode(VisVuMode::Normal)
        .assert_visualization_vu_mode(VisVuMode::Normal);
}

#[test]
fn visualization_refresh_divisor_throttles_data_ticks_from_rust_e2e() {
    let mut app =
        UiE2e::start_player(PlayerSettings::default().with_visualization_refresh_divisor(2));

    app.assert_visualization_refresh_divisor(2)
        .feed_visualization_data(4, 1.0)
        .tick_visualization(100)
        .assert_visualization_band_at_most(4, 0.0)
        .tick_visualization(100)
        .assert_visualization_band_at_least(4, 0.9)
        .set_visualization_refresh_divisor(8)
        .assert_visualization_refresh_divisor(8);
}

#[test]
fn preferences_audio_page_applies_output_volume_and_balance_immediately() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.open_preferences_page(PreferencesPage::Audio)
        .assert_window_visible(Window::Preferences)
        .assert_preferences_page(PreferencesPage::Audio)
        .set_preference_output_device(Some("fakesink"))
        .assert_preference_output_device(Some("fakesink"))
        .set_preference_volume(35)
        .assert_volume(35)
        .set_preference_balance(-40)
        .assert_balance(-40)
        .assert_preferences_saved();
}

#[test]
fn preferences_pages_expose_c_parity_controls() {
    for page in [
        PreferencesPage::Audio,
        PreferencesPage::Visualization,
        PreferencesPage::Options,
        PreferencesPage::Fonts,
        PreferencesPage::Title,
    ] {
        assert!(
            !preferences_page_parity_controls(page).is_empty(),
            "expected {page:?} preferences page to expose controls"
        );
    }

    assert!(preferences_page_parity_controls(PreferencesPage::Options).contains(&"Volume:"));
    assert!(
        preferences_page_parity_controls(PreferencesPage::Visualization)
            .contains(&"Visualization mode:")
    );
    assert!(preferences_page_parity_controls(PreferencesPage::Audio).contains(&"Output device:"));
}

#[test]
fn preferences_options_layout_keeps_zoom_slider_full_width_and_window_tall_enough() {
    assert!(preferences_zoom_spans_full_width());
    assert_eq!(preferences_window_default_size(), (560, 680));
}

#[test]
fn preferences_visualization_controls_follow_selected_mode_sensitivity() {
    let analyzer = visualization_preference_sensitivity(VisMode::Analyzer, true);
    assert!(analyzer.analyzer_mode);
    assert!(analyzer.analyzer_style);
    assert!(analyzer.analyzer_peaks);
    assert!(analyzer.analyzer_falloff);
    assert!(analyzer.peaks_falloff);
    assert!(!analyzer.scope_mode);
    assert!(analyzer.windowshade_vu);
    assert!(analyzer.refresh_rate);

    let scope = visualization_preference_sensitivity(VisMode::Scope, true);
    assert!(!scope.analyzer_mode);
    assert!(scope.scope_mode);
    assert!(!scope.windowshade_vu);
    assert!(scope.refresh_rate);

    let milkdrop = visualization_preference_sensitivity(VisMode::Milkdrop, true);
    assert!(!milkdrop.analyzer_mode);
    assert!(!milkdrop.analyzer_style);
    assert!(!milkdrop.analyzer_peaks);
    assert!(!milkdrop.analyzer_falloff);
    assert!(!milkdrop.peaks_falloff);
    assert!(!milkdrop.scope_mode);
    assert!(!milkdrop.windowshade_vu);
    assert!(!milkdrop.refresh_rate);
}

#[test]
fn local_file_playback_requests_gstreamer_uri_instead_of_only_toggling_ui_state() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.drop_on_main(["file:///music/local-song.ogg"])
        .assert_player_state(PlayerState::Playing)
        .assert_player_spotify_uri(None)
        .assert_last_playback_request(Some("file:///music/local-song.ogg"));
}

#[test]
fn preferences_options_page_applies_playlist_and_docking_options_immediately() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_equalizer_visible(true)
            .with_playlist_visible(true),
    );

    app.open_preferences_page(PreferencesPage::Options)
        .assert_preferences_page(PreferencesPage::Options)
        .assert_window_hidden(Window::Playlist)
        .assert_window_hidden(Window::Equalizer)
        .set_preference_volume(37)
        .assert_volume(37)
        .set_preference_balance(-25)
        .assert_balance(-25)
        .set_preference_scale_factor(1.7)
        .assert_scale_factor(1.7)
        .set_preference_repeat(true)
        .assert_repeat(true)
        .set_preference_shuffle(true)
        .assert_shuffle(true)
        .set_preference_no_playlist_advance(true)
        .assert_no_playlist_advance(true)
        .set_preference_timer_remaining(true)
        .assert_preference_timer_remaining(true)
        .set_preference_playlist_docked(false)
        .assert_panel_detached(PanelKind::Playlist, true)
        .assert_window_visible(Window::Playlist)
        .set_preference_equalizer_docked(false)
        .assert_panel_detached(PanelKind::Equalizer, true)
        .assert_window_visible(Window::Equalizer)
        .set_preference_playlist_docked(true)
        .assert_panel_detached(PanelKind::Playlist, false)
        .assert_window_hidden(Window::Playlist)
        .assert_docked_panel_size((275, 116 + 232))
        .set_preference_equalizer_docked(true)
        .assert_panel_detached(PanelKind::Equalizer, false)
        .assert_window_hidden(Window::Equalizer)
        .assert_docked_panel_size((275, 116 + 116 + 232))
        .set_preference_convert_underscore(false)
        .assert_preference_convert_underscore(false)
        .set_preference_convert_twenty(false)
        .assert_preference_convert_twenty(false)
        .set_preference_show_numbers_in_playlist(false)
        .assert_preference_show_numbers_in_playlist(false)
        .assert_preference_vim_playlist_navigation(false)
        .set_preference_vim_playlist_navigation(true)
        .assert_preference_vim_playlist_navigation(true)
        .assert_preferences_saved();
}

#[test]
fn preferences_docking_changes_mode_without_changing_visibility() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.open_preferences_page(PreferencesPage::Options)
        .set_preference_playlist_docked(false)
        .assert_panel_detached(PanelKind::Playlist, true)
        .assert_window_hidden(Window::Playlist)
        .set_preference_playlist_docked(true)
        .assert_panel_detached(PanelKind::Playlist, false)
        .assert_window_hidden(Window::Playlist)
        .assert_docked_panel_size((275, 116))
        .set_preference_equalizer_docked(false)
        .assert_panel_detached(PanelKind::Equalizer, true)
        .assert_window_hidden(Window::Equalizer)
        .set_preference_equalizer_docked(true)
        .assert_panel_detached(PanelKind::Equalizer, false)
        .assert_window_hidden(Window::Equalizer)
        .assert_docked_panel_size((275, 116));
}

#[test]
fn player_buttons_control_visibility_for_current_docking_mode() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.open_preferences_page(PreferencesPage::Options)
        .set_preference_playlist_docked(false)
        .click(MainTarget::PLAYLIST)
        .assert_window_visible(Window::Playlist)
        .click(MainTarget::PLAYLIST)
        .assert_window_hidden(Window::Playlist)
        .set_preference_playlist_docked(true)
        .click(MainTarget::PLAYLIST)
        .assert_window_hidden(Window::Playlist)
        .assert_docked_panel_size((275, 116 + 232))
        .set_preference_equalizer_docked(false)
        .click(MainTarget::EQUALIZER)
        .assert_window_visible(Window::Equalizer)
        .click(MainTarget::EQUALIZER)
        .assert_window_hidden(Window::Equalizer)
        .set_preference_equalizer_docked(true)
        .click(MainTarget::EQUALIZER)
        .assert_window_hidden(Window::Equalizer)
        .assert_docked_panel_size((275, 116 + 116 + 232));
}

#[test]
fn preferences_font_and_title_pages_apply_text_controls_immediately() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.open_preferences_page(PreferencesPage::Fonts)
        .set_preference_playlist_font("Monospace")
        .assert_preference_playlist_font("Monospace")
        .set_preference_playlist_font("")
        .assert_preference_playlist_font("Helvetica")
        .set_preference_mainwin_font("")
        .assert_preference_mainwin_font("Skin bitmap font")
        .open_preferences_page(PreferencesPage::Title)
        .set_preference_title_format("%p/%t")
        .assert_preference_title_format("%p/%t")
        .set_preference_title_format("")
        .assert_preference_title_format("%p - %t")
        .assert_preferences_saved();
}

#[test]
fn title_format_updates_main_title_and_shaded_playlist_info() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.add_playlist_uri("file:///music/Artist%20Name%20-%20Track_Name.ogg")
        .press_shortcut(Shortcut::PlayFirst)
        .set_preference_title_format("%p/%t")
        .assert_main_title("Artist Name/Track Name")
        .assert_visible_playlist_title(0, "Artist Name/Track Name")
        .press_shortcut(Shortcut::ShadePlaylist)
        .assert_playlist_shaded()
        .assert_shaded_playlist_info("1. Artist Name/Track Name")
        .set_preference_title_format("%t")
        .assert_main_title("Track Name")
        .assert_shaded_playlist_info("1. Track Name");
}

#[test]
fn playlist_font_preference_and_visualization_feed_render_state() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.set_preference_playlist_font("Monospace")
        .assert_playlist_row_font("Monospace")
        .set_visualization_mode(VisMode::Analyzer)
        .feed_visualization_data(4, 0.9)
        .tick_visualization(100)
        .assert_visualization_band_at_least(4, 0.8);
}

#[test]
fn stop_clears_visualization_immediately() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.set_visualization_mode(VisMode::Analyzer)
        .feed_visualization_data(4, 0.9)
        .tick_visualization(100)
        .assert_visualization_band_at_least(4, 0.8)
        .press_shortcut(Shortcut::Stop)
        .assert_visualization_data_cleared()
        .assert_visualization_peak_cleared();
}

#[test]
fn title_format_respects_percent_twenty_conversion_preference() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.add_playlist_uri("file:///music/Artist%20Name%20-%20Track_Name.ogg")
        .press_shortcut(Shortcut::PlayFirst)
        .assert_main_title("Artist Name - Track Name")
        .set_preference_convert_twenty(false)
        .assert_main_title("Artist%20Name%20-%20Track Name")
        .set_preference_convert_underscore(false)
        .assert_main_title("Artist%20Name%20-%20Track_Name");
}

#[test]
fn preferences_visualization_page_applies_controls_immediately() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.open_preferences_page(PreferencesPage::Visualization)
        .set_visualization_mode(VisMode::Scope)
        .assert_visualization_mode(VisMode::Scope)
        .set_visualization_scope_mode(VisScopeMode::Dot)
        .assert_visualization_scope_mode(VisScopeMode::Dot)
        .set_visualization_mode(VisMode::Analyzer)
        .set_visualization_analyzer_mode(VisAnalyzerMode::Fire)
        .assert_visualization_analyzer_mode(VisAnalyzerMode::Fire)
        .set_visualization_analyzer_style(VisAnalyzerStyle::Lines)
        .assert_visualization_analyzer_style(VisAnalyzerStyle::Lines)
        .set_visualization_peaks_enabled(false)
        .assert_visualization_peaks_enabled(false)
        .set_visualization_falloff(VisFalloffSpeed::Slowest, VisFalloffSpeed::Fastest)
        .set_visualization_vu_mode(VisVuMode::Smooth)
        .assert_visualization_vu_mode(VisVuMode::Smooth)
        .set_visualization_refresh_divisor(4)
        .assert_visualization_refresh_divisor(4)
        .assert_preferences_saved();
}

#[test]
fn preferences_podcast_controls_apply_and_reset_defaults() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.open_preferences_page(PreferencesPage::Options)
        .set_preference_podcast_cache_ttl_days(14)
        .assert_preference_podcast_cache_ttl_days(14)
        .set_preference_podcast_refresh_interval_minutes(120)
        .assert_preference_podcast_refresh_interval_minutes(120)
        .set_preference_podcast_cache_ttl_days(0)
        .assert_preference_podcast_cache_ttl_days(60)
        .reset_preferences_to_defaults()
        .assert_volume(100)
        .assert_balance(0)
        .assert_repeat(false)
        .assert_shuffle(false)
        .assert_preference_title_format("%p - %t")
        .assert_preferences_saved();
}

#[test]
fn playlist_bottom_buttons_open_their_submenus() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.assert_no_playlist_menu()
        .click_panel(PanelTarget::PlaylistAdd)
        .assert_playlist_menu(PlaylistMenuKind::Add)
        .click_panel(PanelTarget::PlaylistRemove)
        .assert_playlist_menu(PlaylistMenuKind::Remove)
        .click_panel(PanelTarget::PlaylistSelect)
        .assert_playlist_menu(PlaylistMenuKind::Select)
        .click_panel(PanelTarget::PlaylistMisc)
        .assert_playlist_menu(PlaylistMenuKind::Misc)
        .click_panel(PanelTarget::PlaylistList)
        .assert_playlist_menu(PlaylistMenuKind::List)
        .assert_playlist_menu_hover(Some(2))
        .press_playlist_menu_item(1)
        .assert_playlist_menu_hover(Some(1))
        .hover_playlist_menu_item(0)
        .assert_playlist_menu_hover(Some(0));
}

#[test]
fn playlist_add_menu_url_opens_location_prompt_and_adds_entry() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.accept_open_location("file:///tmp/existing-url-base.mp3")
        .click_panel(PanelTarget::PlaylistAdd)
        .activate_playlist_menu_item(0)
        .assert_window_visible(Window::OpenLocation)
        .accept_open_location("https://example.test/add-url.ogg")
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/existing-url-base.mp3")
        .assert_playlist_entry(1, "https://example.test/add-url.ogg");
}

#[test]
fn playlist_add_menu_file_opens_file_dialog_and_adds_entries() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.accept_open_location("file:///tmp/existing-file-base.mp3")
        .click_panel(PanelTarget::PlaylistAdd)
        .activate_playlist_menu_item(2)
        .assert_file_dialog_visible()
        .accept_playlist_add_file_dialog([
            "file:///tmp/add-file-one.mp3",
            "file:///tmp/add-file-two.ogg",
        ])
        .assert_playlist_len(3)
        .assert_playlist_entry(0, "file:///tmp/existing-file-base.mp3")
        .assert_playlist_entry(1, "file:///tmp/add-file-one.mp3")
        .assert_playlist_entry(2, "file:///tmp/add-file-two.ogg");
}

#[test]
fn playlist_add_menu_directory_opens_directory_dialog_and_adds_entries() {
    let music_dir = unique_temp_dir("xmms-rs-add-menu-dir");
    fs::create_dir_all(&music_dir).unwrap();
    fs::write(music_dir.join("track-one.mp3"), b"audio").unwrap();
    fs::write(music_dir.join("ignored.txt"), b"text").unwrap();
    let dir_uri = format!("file://{}", music_dir.display());

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.accept_open_location("file:///tmp/existing-dir-base.mp3")
        .click_panel(PanelTarget::PlaylistAdd)
        .activate_playlist_menu_item(1)
        .assert_directory_dialog_visible()
        .accept_playlist_add_directory_dialog(&dir_uri)
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/existing-dir-base.mp3");

    fs::remove_dir_all(music_dir).unwrap();
}

#[test]
fn playlist_select_menu_items_update_row_selection() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    for index in 0..3 {
        app.accept_open_location(&format!("file:///tmp/select-{index}.mp3"));
    }

    app.click_panel(PanelTarget::PlaylistSelect)
        .activate_playlist_menu_item(2)
        .assert_playlist_selected(0, true)
        .assert_playlist_selected(1, true)
        .assert_playlist_selected(2, true)
        .click_panel(PanelTarget::PlaylistSelect)
        .activate_playlist_menu_item(1)
        .assert_playlist_selected(0, false)
        .assert_playlist_selected(1, false)
        .assert_playlist_selected(2, false)
        .select_playlist_entry(1)
        .click_panel(PanelTarget::PlaylistSelect)
        .activate_playlist_menu_item(0)
        .assert_playlist_selected(0, true)
        .assert_playlist_selected(1, false)
        .assert_playlist_selected(2, true);
}

#[test]
fn playlist_remove_and_list_menu_items_modify_entries() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    for index in 0..4 {
        app.accept_open_location(&format!("file:///tmp/remove-{index}.mp3"));
    }

    app.select_playlist_entry(1)
        .click_panel(PanelTarget::PlaylistRemove)
        .activate_playlist_menu_item(3)
        .assert_playlist_len(3)
        .assert_playlist_entry(0, "file:///tmp/remove-0.mp3")
        .assert_playlist_entry(1, "file:///tmp/remove-2.mp3")
        .assert_playlist_entry(2, "file:///tmp/remove-3.mp3")
        .select_playlist_entry(1)
        .click_panel(PanelTarget::PlaylistRemove)
        .activate_playlist_menu_item(2)
        .assert_playlist_len(1)
        .assert_playlist_entry(0, "file:///tmp/remove-2.mp3")
        .click_panel(PanelTarget::PlaylistList)
        .activate_playlist_menu_item(0)
        .assert_playlist_len(0);
}

#[test]
fn playlist_context_actions_select_and_remove_entries() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    for index in 0..3 {
        app.accept_open_location(&format!("file:///tmp/context-{index}.mp3"));
    }

    app.activate_playlist_context_action(PlaylistContextAction::SelectAll)
        .assert_playlist_selected(0, true)
        .assert_playlist_selected(1, true)
        .assert_playlist_selected(2, true)
        .activate_playlist_context_action(PlaylistContextAction::SelectNone)
        .assert_playlist_selected(0, false)
        .assert_playlist_selected(1, false)
        .assert_playlist_selected(2, false)
        .select_playlist_entry(1)
        .activate_playlist_context_action(PlaylistContextAction::RemoveSelected)
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/context-0.mp3")
        .assert_playlist_entry(1, "file:///tmp/context-2.mp3");
}

#[test]
fn playlist_delete_key_removes_selected_entries_only() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    for index in 0..4 {
        app.accept_open_location(&format!("file:///tmp/delete-key-{index}.mp3"));
    }

    app.press_playlist_delete()
        .assert_playlist_len(4)
        .select_playlist_entry(1)
        .select_playlist_entry(3)
        .press_playlist_delete()
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/delete-key-0.mp3")
        .assert_playlist_entry(1, "file:///tmp/delete-key-2.mp3");
}

#[test]
fn playlist_context_remove_dead_keeps_existing_local_files_and_urls() {
    let root = unique_temp_dir("xmms-rs-context-dead");
    fs::create_dir_all(&root).unwrap();
    let existing = root.join("existing.mp3");
    fs::write(&existing, b"audio").unwrap();
    let missing = root.join("missing.mp3");
    let existing_uri = format!("file://{}", existing.display());

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.accept_open_location(existing.to_str().unwrap())
        .accept_open_location(&format!("file://{}", missing.display()))
        .accept_open_location("https://example.test/live.mp3")
        .activate_playlist_context_action(PlaylistContextAction::RemoveDead)
        .assert_playlist_len(2)
        .assert_playlist_entry(0, &existing_uri)
        .assert_playlist_entry(1, "https://example.test/live.mp3");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn playlist_context_physical_delete_removes_selected_local_files() {
    let root = unique_temp_dir("xmms-rs-context-physical-delete");
    fs::create_dir_all(&root).unwrap();
    let keep = root.join("keep.mp3");
    let delete = root.join("delete.mp3");
    fs::write(&keep, b"keep").unwrap();
    fs::write(&delete, b"delete").unwrap();
    let keep_uri = format!("file://{}", keep.display());

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.accept_open_location(keep.to_str().unwrap())
        .accept_open_location(delete.to_str().unwrap())
        .select_playlist_entry(1)
        .activate_playlist_context_action(PlaylistContextAction::PhysicallyDelete)
        .assert_playlist_len(1)
        .assert_playlist_entry(0, &keep_uri);

    assert!(keep.exists());
    assert!(!delete.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn playlist_search_selects_matching_rows_and_tracks_query_editing() {
    let mut disabled = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    disabled
        .start_playlist_search()
        .assert_playlist_search_active(false)
        .assert_playlist_search_query("");

    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_vim_playlist_navigation(true),
    );

    for index in 0..20 {
        let name = if index == 18 {
            "target-track"
        } else {
            "ordinary-track"
        };
        app.accept_open_location(&format!("file:///tmp/{index:02}-{name}.mp3"));
    }

    app.start_playlist_search()
        .assert_playlist_search_active(true)
        .assert_playlist_search_query("")
        .type_playlist_search("TARGET")
        .assert_playlist_search_query("TARGET")
        .assert_playlist_selected(18, true)
        .assert_playlist_scroll_offset(4)
        .assert_visible_playlist_entry(14, "file:///tmp/18-target-track.mp3")
        .backspace_playlist_search()
        .assert_playlist_search_query("TARGE")
        .assert_playlist_selected(18, true)
        .stop_playlist_search()
        .assert_playlist_search_active(false)
        .assert_playlist_search_query("")
        .assert_playlist_selected(18, true)
        .start_playlist_search()
        .type_playlist_search("TARGET")
        .assert_playlist_selected(18, true)
        .submit_playlist_search()
        .assert_playlist_search_active(false)
        .assert_playlist_search_query("")
        .assert_player_state(PlayerState::Playing)
        .assert_playlist_position(Some(18))
        .assert_current_playlist_entry("file:///tmp/18-target-track.mp3");
}

#[test]
fn playlist_list_save_opens_dialog_and_writes_m3u() {
    let root = unique_temp_dir("xmms-rs-playlist-save");
    fs::create_dir_all(&root).unwrap();
    let playlist_path = root.join("saved.m3u");

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.accept_open_location("file:///tmp/save-one.mp3")
        .accept_open_location("https://example.test/save-two.ogg")
        .click_panel(PanelTarget::PlaylistList)
        .activate_playlist_menu_item(1)
        .assert_playlist_save_dialog_visible()
        .accept_playlist_save(&playlist_path);

    let saved = fs::read_to_string(&playlist_path).unwrap();
    assert!(saved.contains("#EXTM3U"));
    assert!(saved.contains("file:///tmp/save-one.mp3"));
    assert!(saved.contains("https://example.test/save-two.ogg"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn playlist_list_load_opens_dialog_and_replaces_entries_from_m3u() {
    let root = unique_temp_dir("xmms-rs-playlist-load");
    fs::create_dir_all(&root).unwrap();
    let playlist_path = root.join("loaded.m3u");
    fs::write(
        &playlist_path,
        "#EXTM3U\n#EXTINF:42,Loaded Title\nfile:///tmp/loaded-one.mp3\nhttps://example.test/loaded-two.ogg\n",
    )
    .unwrap();

    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));
    app.accept_open_location("file:///tmp/original.mp3")
        .click_panel(PanelTarget::PlaylistList)
        .activate_playlist_menu_item(2)
        .assert_playlist_load_dialog_visible()
        .accept_playlist_load(&playlist_path)
        .assert_playlist_len(2)
        .assert_playlist_entry(0, "file:///tmp/loaded-one.mp3")
        .assert_playlist_title(0, "Loaded Title")
        .assert_playlist_length_ms(0, 42_000)
        .assert_playlist_entry(1, "https://example.test/loaded-two.ogg");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn playlist_can_resize_from_default_dimensions() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.assert_playlist_size(275, 232)
        .resize_playlist(325, 280)
        .assert_playlist_size(325, 261)
        .resize_playlist(326, 280)
        .assert_playlist_size(325, 261)
        .resize_playlist(100, 80)
        .assert_playlist_size(275, 116);
}

#[test]
fn playlist_startup_size_opens_playlist_at_requested_dimensions() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_detached(true));

    app.start_playlist_size(325, 280)
        .assert_window_visible(Window::Playlist)
        .assert_playlist_size(325, 261);
}

#[test]
fn resized_playlist_bottom_buttons_use_current_geometry() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.resize_playlist(325, 280)
        .click_panel(PanelTarget::PlaylistAdd)
        .assert_playlist_menu(PlaylistMenuKind::Add)
        .click_panel(PanelTarget::PlaylistList)
        .assert_playlist_menu(PlaylistMenuKind::List)
        .assert_playlist_menu_hover(Some(2))
        .press_playlist_menu_item(1)
        .assert_playlist_menu_hover(Some(1));
}

#[test]
fn resized_playlist_title_buttons_use_current_width() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    app.resize_playlist(325, 280)
        .click_panel(PanelTarget::PlaylistShade)
        .assert_playlist_shaded()
        .click_panel(PanelTarget::PlaylistShade)
        .assert_playlist_unshaded()
        .click_panel(PanelTarget::PlaylistClose)
        .assert_window_hidden(Window::Playlist);
}

#[test]
fn docked_playlist_bottom_add_menu_opens_url_file_and_directory_controls() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_playlist_detached(false),
    );

    app.click_docked_panel(PanelTarget::PlaylistAdd)
        .assert_playlist_menu(PlaylistMenuKind::Add)
        .activate_playlist_menu_item(0)
        .assert_window_visible(Window::OpenLocation);

    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_playlist_detached(false),
    );
    app.click_docked_panel(PanelTarget::PlaylistAdd)
        .assert_playlist_menu(PlaylistMenuKind::Add)
        .activate_playlist_menu_item(1)
        .assert_directory_dialog_visible();

    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_playlist_detached(false),
    );
    app.click_docked_panel(PanelTarget::PlaylistAdd)
        .assert_playlist_menu(PlaylistMenuKind::Add)
        .activate_playlist_menu_item(2)
        .assert_file_dialog_visible();
}

#[test]
fn docked_playlist_resizes_vertically_only() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_playlist_detached(false),
    );

    app.resize_playlist(325, 232)
        .resize_docked_playlist_vertically(290)
        .assert_playlist_size(275, 290)
        .resize_docked_playlist_vertically(80)
        .assert_playlist_size(275, 116);
}

#[test]
fn playlist_scrollbar_drag_updates_visible_rows() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    for index in 0..30 {
        app.accept_open_location(&format!("file:///tmp/scroll-{index:02}.mp3"));
    }

    app.assert_playlist_scroll_offset(0)
        .assert_playlist_scrollbar_visible(true)
        .assert_visible_playlist_entry(0, "file:///tmp/scroll-00.mp3")
        .drag_playlist_scrollbar_to_bottom()
        .assert_playlist_scroll_offset(15)
        .assert_visible_playlist_entry(0, "file:///tmp/scroll-15.mp3");
}

#[test]
fn floating_panel_titlebars_track_active_window_state() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_equalizer_visible(true)
            .with_playlist_detached(true)
            .with_equalizer_detached(true),
    );

    app.assert_panel_focused(PanelKind::Playlist, false)
        .assert_panel_focused(PanelKind::Equalizer, false)
        .focus_panel(PanelKind::Playlist, true)
        .assert_panel_focused(PanelKind::Playlist, true)
        .focus_panel(PanelKind::Playlist, false)
        .assert_panel_focused(PanelKind::Playlist, false);
}

#[test]
fn startup_settings_can_open_equalizer_and_playlist() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_equalizer_visible(true)
            .with_playlist_detached(true)
            .with_equalizer_detached(true),
    );

    app.assert_window_visible(Window::Player)
        .assert_window_visible(Window::Playlist)
        .assert_window_visible(Window::Equalizer);
}

#[test]
fn startup_settings_show_docked_equalizer_and_playlist_in_main_window_stack() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_playlist_visible(true)
            .with_equalizer_visible(true),
    );

    app.assert_window_visible(Window::Player)
        .assert_window_hidden(Window::Equalizer)
        .assert_window_hidden(Window::Playlist)
        .assert_docked_panel_size((275, 464));
}

#[test]
fn docked_equalizer_and_playlist_can_be_shaded_from_main_window_stack() {
    let mut app = UiE2e::start_player(
        PlayerSettings::default()
            .with_equalizer_visible(true)
            .with_playlist_visible(true),
    );

    app.assert_docked_panel_size((275, 464))
        .click_docked_panel(PanelTarget::EqualizerShade)
        .assert_equalizer_shaded()
        .assert_docked_panel_size((275, 362))
        .click_docked_panel(PanelTarget::PlaylistShade)
        .assert_playlist_shaded()
        .assert_docked_panel_size((275, 144))
        .click_docked_panel(PanelTarget::EqualizerShade)
        .assert_equalizer_unshaded()
        .click_docked_panel(PanelTarget::PlaylistShade)
        .assert_playlist_unshaded()
        .assert_docked_panel_size((275, 464));
}

#[test]
fn podcast_e2e_classifies_feed_and_stream_responses() {
    assert_eq!(
        classify_url_response(Some("application/atom+xml"), false, b""),
        PodcastUrlKind::Feed
    );
    assert_eq!(
        classify_url_response(Some("audio/ogg"), false, b""),
        PodcastUrlKind::DirectStream
    );
    assert_eq!(
        classify_url_response(None, false, b"<?xml version='1.0'?><rss></rss>"),
        PodcastUrlKind::Feed
    );
    assert_eq!(
        classify_url_response(None, true, b""),
        PodcastUrlKind::DirectStream
    );
}

#[test]
fn podcast_e2e_parses_rss_atom_enclosure_urls_and_fallbacks() {
    let episodes = parse_feed(
        r#"
        <rss><channel>
          <item><title>First &amp; Best</title><guid>one</guid><enclosure url="media/one.mp3"/></item>
          <entry><title>Second</title><id>two</id><link href="/two.ogg" rel="enclosure"/></entry>
          <item><content url="https://cdn.example.test/three.mp3"/></item>
        </channel></rss>
        "#,
        "https://example.test/podcasts/feed.xml",
    );

    assert_eq!(episodes.len(), 3);
    assert_eq!(
        episodes[0].url,
        "https://example.test/podcasts/media/one.mp3"
    );
    assert_eq!(episodes[0].title, "First & Best");
    assert_eq!(episodes[0].guid.as_deref(), Some("one"));
    assert_eq!(episodes[1].url, "https://example.test/two.ogg");
    assert_eq!(episodes[1].guid.as_deref(), Some("two"));
    assert_eq!(episodes[2].title, "https://cdn.example.test/three.mp3");
}

#[test]
fn podcast_e2e_cache_paths_freshness_cleanup_and_playback_uri_follow_c_defaults() {
    let root = unique_temp_dir("xmms-rs-podcast-cache");
    fs::create_dir_all(&root).unwrap();
    let cache = cache_path_for_url(&root, "https://example.test/episode.mp3");

    assert_eq!(cache.parent().unwrap(), root.join("podcast-cache"));
    assert_eq!(cache.file_name().unwrap().to_string_lossy().len(), 64);
    assert!(cache_is_fresh(1, 1_000, 1_000 + 30, 0));
    assert!(!cache_is_fresh(0, 1_000, 1_000 + 30, 0));

    let stale = stale_cache_files(
        &[
            PodcastCacheEntry {
                name: "stale".to_string(),
                modified_unix: 0,
                size: 1,
            },
            PodcastCacheEntry {
                name: "stale.part".to_string(),
                modified_unix: 0,
                size: 1,
            },
        ],
        61 * 24 * 60 * 60,
        0,
    );
    assert_eq!(stale, vec!["stale"]);
    assert_eq!(
        prepare_playback_uri(true, "https://example.test/episode.mp3", &cache, true),
        format!("file://{}", cache.display())
    );

    let cache_dir = root.join("podcast-cache");
    fs::create_dir_all(&cache_dir).unwrap();
    let stale_path = cache_dir.join("stale-file");
    let part_path = cache_dir.join("stale-file.part");
    fs::write(&stale_path, b"stale").unwrap();
    fs::write(&part_path, b"partial").unwrap();
    assert_eq!(cleanup_cache_dir(&root, i64::MAX, 0).unwrap(), 1);
    assert!(!stale_path.exists());
    assert!(part_path.exists());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn podcast_e2e_retry_and_refresh_scheduling_match_c() {
    assert!(status_should_retry(429));
    assert!(status_should_retry(503));
    assert!(!status_should_retry(404));
    assert_eq!(retry_delay_seconds(Some("15"), 0), 15);
    assert_eq!(retry_delay_seconds(Some("120"), 0), 60);
    assert_eq!(retry_delay_seconds(None, 4), 16);
    assert_eq!(refresh_interval_seconds(0), 60 * 60);
    assert_eq!(refresh_interval_seconds(5), 5 * 60);
}

#[test]
fn podcast_e2e_imports_parsed_feed_into_playlist_with_deduplication() {
    let mut playlist = Playlist::new();
    let feed = r#"
        <rss><channel>
          <item><title>Episode One</title><guid>g1</guid><enclosure url="one.mp3"/></item>
          <item><title>Episode One Updated</title><guid>g1</guid><enclosure url="one-new.mp3"/></item>
        </channel></rss>
    "#;

    assert_eq!(
        add_feed_to_playlist(&mut playlist, feed, "https://example.test/feed.xml"),
        2
    );
    assert_eq!(playlist.len(), 1);
    assert_eq!(playlist.entries()[0].title, "Episode One Updated");
    assert_eq!(playlist.entries()[0].podcast_guid.as_deref(), Some("g1"));
}

#[test]
fn podcast_e2e_writes_cache_file_and_discovers_cached_duration() {
    let root = unique_temp_dir("xmms-rs-podcast-cache-write");
    fs::create_dir_all(&root).unwrap();
    let path =
        write_cache_file(&root, "https://example.test/audio.wav", &silent_wav_bytes()).unwrap();

    assert!(path.exists());
    assert!(!path.with_extension("part").exists());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(cache_file_is_fresh(&path, now, 0).unwrap());
    assert!(discover_cached_duration_ms(&path).unwrap().unwrap() >= 1_000);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn podcast_e2e_handles_fetched_feed_or_direct_stream_response() {
    let mut playlist = Playlist::new();
    let action = handle_url_response(
        &mut playlist,
        "https://example.test/feed.xml",
        &PodcastHttpResponse {
            status: 200,
            content_type: Some("application/rss+xml".to_string()),
            has_icy_name: false,
            retry_after: None,
            body: br#"<rss><channel><item><title>Fetched</title><enclosure url="fetched.mp3"/></item></channel></rss>"#.to_vec(),
        },
    )
    .unwrap();

    assert_eq!(action, PodcastResponseAction::AddedFeedEpisodes(1));
    assert_eq!(playlist.entries()[0].title, "Fetched");

    let action = handle_url_response(
        &mut playlist,
        "https://example.test/live.ogg",
        &PodcastHttpResponse {
            status: 200,
            content_type: Some("audio/ogg".to_string()),
            has_icy_name: false,
            retry_after: None,
            body: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(action, PodcastResponseAction::AddedDirectStream);
    assert_eq!(
        playlist.entries()[1].filename,
        "https://example.test/live.ogg"
    );
}

#[test]
fn podcast_e2e_download_retry_loop_writes_cache_after_retry() {
    let root = unique_temp_dir("xmms-rs-podcast-retry");
    fs::create_dir_all(&root).unwrap();
    let outcome = download_with_retries(&root, "https://example.test/retry.mp3", |attempt| {
        Ok(if attempt < 2 {
            PodcastDownloadAttempt {
                status: 429,
                retry_after: Some((attempt + 1).to_string()),
                body: Vec::new(),
            }
        } else {
            PodcastDownloadAttempt {
                status: 200,
                retry_after: None,
                body: b"downloaded".to_vec(),
            }
        })
    })
    .unwrap();

    assert_eq!(outcome.attempts, 3);
    assert_eq!(outcome.retry_delays, vec![1, 2]);
    assert_eq!(fs::read(outcome.cache_path).unwrap(), b"downloaded");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn podcast_e2e_failed_current_podcast_item_is_skipped() {
    let mut playlist = Playlist::new();
    playlist.add_podcast_entry(
        "https://example.test/fail.mp3",
        Some("Needs cache".to_string()),
        Some("https://example.test/feed.xml".to_string()),
        Some("guid".to_string()),
    );
    playlist.add_uri("file:///tmp/after-failure.mp3");
    playlist.set_position(0);

    assert!(mark_cache_failed_and_skip_current(
        &mut playlist,
        "https://example.test/fail.mp3"
    ));
    assert_eq!(playlist.position(), Some(1));
    assert_eq!(playlist.entries()[0].title, "failed: Needs cache");
}

#[test]
fn podcast_e2e_refresh_scheduler_returns_due_feeds_and_reschedules() {
    let mut scheduler = PodcastRefreshScheduler::new();
    scheduler.add_feed("https://example.test/z.xml");
    scheduler.add_feed("https://example.test/a.xml");
    scheduler.schedule_from(10, 1);

    assert_eq!(scheduler.next_refresh_unix(), Some(70));
    assert!(scheduler.due_feeds(69).is_empty());
    assert_eq!(
        scheduler.due_feeds(70),
        vec!["https://example.test/a.xml", "https://example.test/z.xml"]
    );
    scheduler.mark_refreshed(70, 2);
    assert_eq!(scheduler.next_refresh_unix(), Some(190));
}

#[test]
fn podcast_e2e_live_fetch_imports_feed_from_http_response() {
    let feed = r#"<rss><channel><item><title>Live</title><enclosure url="live.mp3"/></item></channel></rss>"#;
    let (url, server) = local_http_server(vec![format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/rss+xml\r\nContent-Length: {}\r\n\r\n{}",
        feed.len(),
        feed
    )]);

    let mut playlist = Playlist::new();
    let action = fetch_url_into_playlist(&mut playlist, &url).unwrap();

    assert_eq!(action, PodcastResponseAction::AddedFeedEpisodes(1));
    assert_eq!(playlist.entries()[0].title, "Live");
    server.join().unwrap();
}

#[test]
fn podcast_e2e_live_download_retries_http_503_and_writes_cache() {
    let (url, server) = local_http_server(vec![
        "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 1\r\nContent-Length: 0\r\n\r\n"
            .to_string(),
        "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 10\r\n\r\ndownloaded"
            .to_string(),
    ]);
    let root = unique_temp_dir("xmms-rs-podcast-live-download");
    fs::create_dir_all(&root).unwrap();

    let outcome = download_url_with_retries(&root, &url).unwrap();

    assert_eq!(outcome.attempts, 2);
    assert_eq!(outcome.retry_delays, vec![1]);
    assert_eq!(fs::read(outcome.cache_path).unwrap(), b"downloaded");
    fs::remove_dir_all(root).unwrap();
    server.join().unwrap();
}

fn silent_wav_bytes() -> Vec<u8> {
    let sample_rate = 8_000u32;
    let samples = 8_000u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + samples).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&8u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&samples.to_le_bytes());
    bytes.extend(std::iter::repeat_n(128u8, samples as usize));
    bytes
}

fn local_http_server(responses: Vec<String>) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request).unwrap();
            std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        }
    });
    (format!("http://{addr}/podcast"), handle)
}
