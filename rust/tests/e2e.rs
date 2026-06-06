use xmms_resuscitated::e2e::{MainTarget, PlayerSettings, UiE2e, Window};

#[test]
fn playlist_button_opens_playlist_window() {
    let mut app = UiE2e::start_player(PlayerSettings::default());

    app.assert_window_visible(Window::Player)
        .assert_window_hidden(Window::Playlist);

    app.click(MainTarget::PLAYLIST)
        .assert_window_visible(Window::Playlist);
}
