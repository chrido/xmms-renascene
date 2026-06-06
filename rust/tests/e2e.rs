use xmms_resuscitated::e2e::{MainTarget, MenuItem, PanelTarget, PlayerSettings, UiE2e, Window};
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
