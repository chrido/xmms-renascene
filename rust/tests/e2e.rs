use xmms_resuscitated::e2e::{MainTarget, PlayerSettings, UiE2e, Window};
use xmms_resuscitated::player::PlayerState;

#[test]
fn titlebar_buttons_keep_player_open_minimize_shade_and_close() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.click(MainTarget::MENU)
        .assert_window_visible(Window::Player)
        .assert_player_not_minimized()
        .assert_player_unshaded();

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
        .assert_player_state(PlayerState::Stopped);
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
