use std::fs;
use std::path::{Path, PathBuf};
use xmms_resuscitated::e2e::{
    MainTarget, MenuItem, PanelTarget, PlayerSettings, Shortcut, UiE2e, Window,
};
use xmms_resuscitated::player::PlayerState;
use xmms_resuscitated::playlist::PlaylistSortKey;
use xmms_resuscitated::ui::{PanelKind, PlaylistContextAction, PlaylistMenuKind};

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
        .assert_position(83);

    app.show_jump_time_prompt()
        .accept_jump_time("42")
        .assert_last_jump_time_ms(42_000)
        .assert_position(42);
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

    app.press_shortcut(Shortcut::Play)
        .assert_player_state(PlayerState::Playing)
        .press_shortcut(Shortcut::Pause)
        .assert_player_state(PlayerState::Paused)
        .press_shortcut(Shortcut::Stop)
        .assert_player_state(PlayerState::Stopped)
        .click(MainTarget::position(100))
        .assert_position(100)
        .press_shortcut(Shortcut::Previous)
        .assert_position(0)
        .click(MainTarget::position(100))
        .assert_position(100)
        .press_shortcut(Shortcut::Next)
        .assert_position(0);

    app.press_shortcut(Shortcut::OpenFiles)
        .assert_file_dialog_visible()
        .assert_shuffle(false)
        .press_shortcut(Shortcut::ToggleShuffle)
        .assert_shuffle(true)
        .assert_repeat(false)
        .press_shortcut(Shortcut::ToggleRepeat)
        .assert_repeat(true)
        .assert_no_advance(false)
        .press_shortcut(Shortcut::ToggleNoAdvance)
        .assert_no_advance(true)
        .press_shortcut(Shortcut::Preferences)
        .assert_window_visible(Window::Preferences)
        .press_shortcut(Shortcut::SkinBrowser)
        .assert_window_visible(Window::SkinBrowser);
}

#[test]
fn panel_keyboard_shortcuts_toggle_and_shade_windows() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

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
        .assert_player_state(PlayerState::Playing)
        .click(MainTarget::NEXT)
        .assert_playlist_position(Some(1))
        .click(MainTarget::NEXT)
        .assert_playlist_position(Some(1))
        .click(MainTarget::PREVIOUS)
        .assert_playlist_position(Some(0))
        .click(MainTarget::REPEAT)
        .click(MainTarget::PREVIOUS)
        .assert_playlist_position(Some(1))
        .press_shortcut(Shortcut::ToggleNoAdvance)
        .playlist_eof_reached()
        .assert_playlist_position(Some(1))
        .assert_player_state(PlayerState::Stopped);
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
fn update_timer_advances_position_while_playing_only() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_position(0)
        .update_timer_tick(1_000)
        .assert_position(0)
        .press_shortcut(Shortcut::Play)
        .update_timer_tick(900)
        .assert_position(0)
        .update_timer_tick(100)
        .assert_position(1)
        .update_timer_tick(2_000)
        .assert_position(3)
        .press_shortcut(Shortcut::Pause)
        .update_timer_tick(1_000)
        .assert_position(3)
        .press_shortcut(Shortcut::Stop)
        .update_timer_tick(1_000)
        .assert_position(0);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

#[test]
fn transport_buttons_update_player_state_and_position() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click(MainTarget::PLAY)
        .assert_player_state(PlayerState::Playing);

    app.click(MainTarget::PAUSE)
        .assert_player_state(PlayerState::Paused);

    app.click(MainTarget::PAUSE)
        .assert_player_state(PlayerState::Playing);

    app.click(MainTarget::position(100)).assert_position(100);

    app.click(MainTarget::PREVIOUS).assert_position(0);

    app.click(MainTarget::position(100)).assert_position(100);

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

    app.click(MainTarget::position(0)).assert_position(0);
    app.click(MainTarget::position(100)).assert_position(100);
    app.click(MainTarget::position(219)).assert_position(219);
}

#[test]
fn playlist_button_opens_and_closes_playlist_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_window_visible(Window::Player)
        .assert_window_hidden(Window::Playlist);

    app.click(MainTarget::PLAYLIST)
        .assert_window_visible(Window::Playlist);

    app.click(MainTarget::PLAYLIST)
        .assert_window_hidden(Window::Playlist);
}

#[test]
fn equalizer_button_opens_and_closes_equalizer_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_window_visible(Window::Player)
        .assert_window_hidden(Window::Equalizer);

    app.click(MainTarget::EQUALIZER)
        .assert_window_visible(Window::Equalizer);

    app.click(MainTarget::EQUALIZER)
        .assert_window_hidden(Window::Equalizer);
}

#[test]
fn equalizer_top_right_buttons_shade_and_close_equalizer_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

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
        .assert_equalizer_preamp_position(25)
        .drag_equalizer_band(0, 10)
        .assert_equalizer_band_position(0, 11)
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
fn playlist_top_right_buttons_shade_and_close_playlist_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

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
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

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
        .assert_playlist_search_query("");
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
        .assert_playlist_size(325, 280)
        .resize_playlist(100, 80)
        .assert_playlist_size(275, 116);
}

#[test]
fn playlist_startup_size_opens_playlist_at_requested_dimensions() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.start_playlist_size(325, 280)
        .assert_window_visible(Window::Playlist)
        .assert_playlist_size(325, 280);
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
fn playlist_scrollbar_drag_updates_visible_rows() {
    let mut app = UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true));

    for index in 0..30 {
        app.accept_open_location(&format!("file:///tmp/scroll-{index:02}.mp3"));
    }

    app.assert_playlist_scroll_offset(0)
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
            .with_equalizer_visible(true),
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
            .with_equalizer_visible(true),
    );

    app.assert_window_visible(Window::Player)
        .assert_window_visible(Window::Playlist)
        .assert_window_visible(Window::Equalizer);
}
