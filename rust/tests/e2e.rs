use xmms_resuscitated::e2e::{
    MainTarget, MenuItem, PanelTarget, PlayerSettings, Shortcut, UiE2e, Window,
};
use xmms_resuscitated::player::PlayerState;
use xmms_resuscitated::ui::{PanelKind, PlaylistMenuKind};

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
        .assert_last_open_location("https://example.test/song.ogg");

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
        .assert_last_open_location("file:///tmp/example.mp3");

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

    app.press_shortcut(Shortcut::OpenDirectory)
        .assert_directory_dialog_visible()
        .accept_directory_dialog("file:///tmp/music")
        .assert_playlist_len(1)
        .assert_playlist_entry(0, "file:///tmp/music")
        .assert_player_state(PlayerState::Playing);
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
