//! Pure interpretation of socket and MPRIS requests. Frontends own transport,
//! command execution, window actions, and service emission.

use crate::app::command::{AppCommand, PlayerCommand, PlaylistCommand, UiCommand};
use crate::app::runtime::RuntimeEvent;
use crate::app_state::AppState;
use crate::mpris::{mpris_player_properties, MprisCommand, MprisEvent, MprisPlayerProperties};
use crate::socket_control::{SocketCommand, SocketUiCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendAction {
    Raise,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketUiTarget {
    Preferences,
    SkinBrowser,
    MainMenu,
}

impl SocketUiTarget {
    pub fn visible(self, state: &AppState) -> bool {
        match self {
            Self::Preferences => state.ui.preferences_visible,
            Self::SkinBrowser => state.ui.skin_browser_visible,
            Self::MainMenu => state.ui.main_menu_visible,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SocketTranslation {
    pub events: Vec<RuntimeEvent>,
    pub ui_target: Option<SocketUiTarget>,
    pub action: Option<FrontendAction>,
    pub redraw: bool,
}

pub fn translate_socket_ui_command(command: &SocketUiCommand) -> (AppCommand, SocketUiTarget) {
    let (command, target) = match command {
        SocketUiCommand::SetPreferencesVisible(visible) => (
            UiCommand::SetPreferencesVisible(*visible),
            SocketUiTarget::Preferences,
        ),
        SocketUiCommand::TogglePreferences => {
            (UiCommand::TogglePreferences, SocketUiTarget::Preferences)
        }
        SocketUiCommand::SetMainMenuVisible(visible) => (
            UiCommand::SetMainMenuVisible(*visible),
            SocketUiTarget::MainMenu,
        ),
        SocketUiCommand::SetSkinBrowserVisible(visible) => (
            UiCommand::SetSkinBrowserVisible(*visible),
            SocketUiTarget::SkinBrowser,
        ),
        SocketUiCommand::ToggleSkinBrowser => {
            (UiCommand::ToggleSkinBrowser, SocketUiTarget::SkinBrowser)
        }
    };
    (command.into(), target)
}

pub fn translate_socket_command(command: &SocketCommand) -> SocketTranslation {
    let mut translation = SocketTranslation {
        events: Vec::new(),
        ui_target: None,
        action: None,
        redraw: false,
    };
    match command {
        SocketCommand::App(command) => {
            translation
                .events
                .push(RuntimeEvent::Command(command.clone()));
            translation.redraw = true;
        }
        SocketCommand::Ui(command) => {
            let (command, target) = translate_socket_ui_command(command);
            translation.events.push(RuntimeEvent::Command(command));
            translation.ui_target = Some(target);
            translation.redraw = true;
        }
        SocketCommand::Ping => {}
        SocketCommand::Quit => translation.action = Some(FrontendAction::Quit),
    }
    translation
}

#[derive(Debug, Clone, PartialEq)]
pub struct MprisTranslation {
    pub events: Vec<RuntimeEvent>,
    pub action: Option<FrontendAction>,
    /// Only OpenUri needs a pre-dispatch snapshot to report a failed replacement's clear.
    pub properties_before: Option<MprisPlayerProperties>,
}

pub fn translate_mpris_command(command: &MprisCommand, state: &AppState) -> MprisTranslation {
    let mut translation = MprisTranslation {
        events: Vec::new(),
        action: None,
        properties_before: None,
    };
    let app_command: Option<AppCommand> = match command {
        MprisCommand::Raise => {
            translation.action = Some(FrontendAction::Raise);
            None
        }
        MprisCommand::Quit => {
            translation.action = Some(FrontendAction::Quit);
            None
        }
        MprisCommand::Next => Some(PlayerCommand::NextTrack.into()),
        MprisCommand::Previous => Some(PlayerCommand::PreviousTrack.into()),
        MprisCommand::Pause => Some(PlayerCommand::Pause.into()),
        MprisCommand::PlayPause => Some(PlayerCommand::PlayPause.into()),
        MprisCommand::Stop => Some(PlayerCommand::Halt.into()),
        MprisCommand::Play => Some(PlayerCommand::Play.into()),
        MprisCommand::Seek { offset_us } => {
            let target_ms = state
                .config
                .playback_position_ms
                .max(0)
                .saturating_mul(1_000)
                .saturating_add(*offset_us)
                .max(0)
                / 1_000;
            Some(PlayerCommand::SeekToMs(target_ms).into())
        }
        MprisCommand::SetPosition { position_us, .. } => {
            Some(PlayerCommand::SeekToMs((position_us / 1_000).max(0)).into())
        }
        MprisCommand::OpenUri(uri) => {
            if !uri.trim().is_empty() {
                translation.properties_before = Some(mpris_player_properties(
                    state,
                    state.config.playback_position_ms,
                ));
                translation.events.extend([
                    RuntimeEvent::Command(PlaylistCommand::Clear.into()),
                    RuntimeEvent::Command(PlaylistCommand::AddLocations(vec![uri.clone()]).into()),
                ]);
            }
            None
        }
    };
    if let Some(command) = app_command {
        translation.events.push(RuntimeEvent::Command(command));
    }
    translation
}

/// Called after the OpenUri Clear/AddLocations events have been executed.
pub fn mpris_open_uri_playback_events(
    command: &MprisCommand,
    state: &AppState,
) -> Vec<RuntimeEvent> {
    if matches!(command, MprisCommand::OpenUri(uri) if !uri.trim().is_empty())
        && !state.playlist.is_empty()
    {
        vec![
            RuntimeEvent::Command(PlaylistCommand::SetPosition(0).into()),
            RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
        ]
    } else {
        Vec::new()
    }
}

/// Inspect post-dispatch state so Seeked reports the actual resulting position.
pub fn mpris_service_events(
    command: &MprisCommand,
    state: &AppState,
    properties_before: Option<&MprisPlayerProperties>,
) -> Vec<MprisEvent> {
    let position_us = state.config.playback_position_ms * 1_000;
    match command {
        MprisCommand::Raise => vec![MprisEvent::Raised],
        MprisCommand::Quit => vec![MprisEvent::QuitRequested],
        MprisCommand::Seek { .. } | MprisCommand::SetPosition { .. } => {
            vec![MprisEvent::Seeked(position_us)]
        }
        MprisCommand::Stop => vec![
            MprisEvent::PlaybackStatusChanged,
            MprisEvent::Seeked(position_us),
        ],
        MprisCommand::Next
        | MprisCommand::Previous
        | MprisCommand::Pause
        | MprisCommand::PlayPause
        | MprisCommand::Play => vec![MprisEvent::PlaybackStatusChanged],
        MprisCommand::OpenUri(uri) => {
            if uri.trim().is_empty() {
                return Vec::new();
            }
            if !state.playlist.is_empty() {
                return vec![
                    MprisEvent::MetadataChanged,
                    MprisEvent::PlaybackStatusChanged,
                ];
            }
            // A failed AddLocations may still have cleared the previous track and
            // stopped playback. Notify only about properties that actually changed.
            let Some(before) = properties_before else {
                return Vec::new();
            };
            let after = mpris_player_properties(state, state.config.playback_position_ms);
            let mut events = Vec::new();
            if before.metadata != after.metadata {
                events.push(MprisEvent::MetadataChanged);
            }
            if before.playback_status != after.playback_status
                || before.position_us != after.position_us
            {
                events.push(MprisEvent::PlaybackStatusChanged);
            }
            events
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::AudioCommand;
    use crate::app::store::AppStore;

    fn event(command: impl Into<AppCommand>) -> RuntimeEvent {
        RuntimeEvent::Command(command.into())
    }

    #[test]
    fn socket_ui_commands_map_all_visibility_variants() {
        use SocketUiCommand::*;
        let cases = [
            (
                SetPreferencesVisible(true),
                UiCommand::SetPreferencesVisible(true),
                SocketUiTarget::Preferences,
            ),
            (
                SetPreferencesVisible(false),
                UiCommand::SetPreferencesVisible(false),
                SocketUiTarget::Preferences,
            ),
            (
                TogglePreferences,
                UiCommand::TogglePreferences,
                SocketUiTarget::Preferences,
            ),
            (
                SetMainMenuVisible(true),
                UiCommand::SetMainMenuVisible(true),
                SocketUiTarget::MainMenu,
            ),
            (
                SetMainMenuVisible(false),
                UiCommand::SetMainMenuVisible(false),
                SocketUiTarget::MainMenu,
            ),
            (
                SetSkinBrowserVisible(true),
                UiCommand::SetSkinBrowserVisible(true),
                SocketUiTarget::SkinBrowser,
            ),
            (
                SetSkinBrowserVisible(false),
                UiCommand::SetSkinBrowserVisible(false),
                SocketUiTarget::SkinBrowser,
            ),
            (
                ToggleSkinBrowser,
                UiCommand::ToggleSkinBrowser,
                SocketUiTarget::SkinBrowser,
            ),
        ];
        for (input, expected_command, target) in cases {
            let expected_visible = match &input {
                SetPreferencesVisible(visible)
                | SetMainMenuVisible(visible)
                | SetSkinBrowserVisible(visible) => *visible,
                TogglePreferences | ToggleSkinBrowser => true,
            };
            assert_eq!(
                translate_socket_ui_command(&input),
                (expected_command.clone().into(), target)
            );
            let translated = translate_socket_command(&SocketCommand::Ui(input));
            assert_eq!(translated.events, vec![event(expected_command.clone())]);
            assert_eq!(translated.ui_target, Some(target));
            assert_eq!(translated.action, None);
            assert!(translated.redraw);
            let mut store = AppStore::new(AppState::default());
            store.dispatch(expected_command);
            assert_eq!(target.visible(store.state()), expected_visible);
        }
        let mut state = AppState::default();
        state.ui.preferences_visible = true;
        state.ui.skin_browser_visible = true;
        state.ui.main_menu_visible = true;
        for target in [
            SocketUiTarget::Preferences,
            SocketUiTarget::SkinBrowser,
            SocketUiTarget::MainMenu,
        ] {
            assert!(target.visible(&state));
        }
    }

    #[test]
    fn socket_app_ping_and_quit_translate_to_single_actions() {
        let cases = [
            (
                SocketCommand::App(AudioCommand::SetVolume(42).into()),
                vec![event(AudioCommand::SetVolume(42))],
                None,
                true,
            ),
            (SocketCommand::Ping, vec![], None, false),
            (
                SocketCommand::Quit,
                vec![],
                Some(FrontendAction::Quit),
                false,
            ),
        ];
        for (input, events, action, redraw) in cases {
            let translated = translate_socket_command(&input);
            assert_eq!(translated.events, events);
            assert_eq!(translated.action, action);
            assert_eq!(translated.redraw, redraw);
            assert_eq!(translated.ui_target, None);
        }
    }

    #[test]
    fn mpris_commands_map_every_variant_and_service_event() {
        let mut state = AppState::default();
        state.config.playback_position_ms = 5_000;
        let status = vec![MprisEvent::PlaybackStatusChanged];
        let cases = [
            (
                MprisCommand::Raise,
                vec![],
                Some(FrontendAction::Raise),
                vec![MprisEvent::Raised],
            ),
            (
                MprisCommand::Quit,
                vec![],
                Some(FrontendAction::Quit),
                vec![MprisEvent::QuitRequested],
            ),
            (
                MprisCommand::Next,
                vec![event(PlayerCommand::NextTrack)],
                None,
                status.clone(),
            ),
            (
                MprisCommand::Previous,
                vec![event(PlayerCommand::PreviousTrack)],
                None,
                status.clone(),
            ),
            (
                MprisCommand::Pause,
                vec![event(PlayerCommand::Pause)],
                None,
                status.clone(),
            ),
            (
                MprisCommand::PlayPause,
                vec![event(PlayerCommand::PlayPause)],
                None,
                status.clone(),
            ),
            (
                MprisCommand::Stop,
                vec![event(PlayerCommand::Halt)],
                None,
                vec![
                    MprisEvent::PlaybackStatusChanged,
                    MprisEvent::Seeked(5_000_000),
                ],
            ),
            (
                MprisCommand::Play,
                vec![event(PlayerCommand::Play)],
                None,
                status,
            ),
            (
                MprisCommand::Seek {
                    offset_us: -2_000_000,
                },
                vec![event(PlayerCommand::SeekToMs(3_000))],
                None,
                vec![MprisEvent::Seeked(5_000_000)],
            ),
            (
                MprisCommand::SetPosition {
                    track_id: "/org/xmms/Track/0".into(),
                    position_us: 42_000_000,
                },
                vec![event(PlayerCommand::SeekToMs(42_000))],
                None,
                vec![MprisEvent::Seeked(5_000_000)],
            ),
            (
                MprisCommand::OpenUri("file:///tmp/a.ogg".into()),
                vec![
                    event(PlaylistCommand::Clear),
                    event(PlaylistCommand::AddLocations(vec![
                        "file:///tmp/a.ogg".into()
                    ])),
                ],
                None,
                vec![],
            ),
        ];
        for (command, events, action, service_events) in cases {
            let translation = translate_mpris_command(&command, &state);
            assert_eq!(translation.events, events, "{command:?}");
            assert_eq!(translation.action, action, "{command:?}");
            assert_eq!(
                mpris_service_events(&command, &state, translation.properties_before.as_ref()),
                service_events,
                "{command:?}"
            );
        }
        assert_eq!(
            translate_mpris_command(&MprisCommand::Seek { offset_us: -999 }, &state).events,
            vec![event(PlayerCommand::SeekToMs(4_999))]
        );
        assert_eq!(
            translate_mpris_command(
                &MprisCommand::Seek {
                    offset_us: -999_999_999
                },
                &state
            )
            .events,
            vec![event(PlayerCommand::SeekToMs(0))]
        );
        assert_eq!(
            translate_mpris_command(
                &MprisCommand::SetPosition {
                    track_id: "ignored".into(),
                    position_us: -1_000
                },
                &state
            )
            .events,
            vec![event(PlayerCommand::SeekToMs(0))]
        );
        state.config.playback_position_ms = 42_000;
        assert_eq!(
            mpris_service_events(&MprisCommand::Seek { offset_us: 1 }, &state, None),
            vec![MprisEvent::Seeked(42_000_000)]
        );
    }

    #[test]
    fn mpris_failed_open_uri_reports_only_properties_changed_by_clear() {
        let command = MprisCommand::OpenUri("/empty/directory".into());
        let mut store = AppStore::new(AppState::default());
        assert!(mpris_service_events(
            &command,
            store.state(),
            translate_mpris_command(&command, store.state())
                .properties_before
                .as_ref(),
        )
        .is_empty());
        store.dispatch(PlaylistCommand::AddUris(vec!["file:///tmp/old.ogg".into()]));
        store.dispatch(PlaylistCommand::SetPosition(0));
        store.dispatch(PlayerCommand::StartCurrentTrack);
        let translation = translate_mpris_command(&command, store.state());
        store.dispatch(PlaylistCommand::Clear);
        assert!(store.state().playlist.is_empty());
        assert_eq!(
            mpris_service_events(
                &command,
                store.state(),
                translation.properties_before.as_ref()
            ),
            vec![
                MprisEvent::MetadataChanged,
                MprisEvent::PlaybackStatusChanged
            ]
        );
    }

    #[test]
    fn mpris_open_uri_runs_only_after_success_and_blank_preserves_playlist() {
        let mut store = AppStore::new(AppState::default());
        store.dispatch(PlaylistCommand::AddUris(
            vec!["file:///tmp/old.ogg".into()].into(),
        ));
        for blank in ["", " \t\n "] {
            let command = MprisCommand::OpenUri(blank.into());
            assert!(translate_mpris_command(&command, store.state())
                .events
                .is_empty());
            assert!(mpris_open_uri_playback_events(&command, store.state()).is_empty());
            assert!(mpris_service_events(&command, store.state(), None).is_empty());
            assert_eq!(
                store.state().playlist.entries()[0].filename,
                "file:///tmp/old.ogg"
            );
        }
        let command = MprisCommand::OpenUri("file:///tmp/new.ogg".into());
        let translation = translate_mpris_command(&command, store.state());
        for event in translation.events {
            let RuntimeEvent::Command(app_command) = event else {
                panic!("expected command")
            };
            store.dispatch(app_command);
        }
        assert_eq!(store.state().playlist.len(), 1);
        assert_eq!(
            store.state().playlist.entries()[0].filename,
            "file:///tmp/new.ogg"
        );
        assert_eq!(
            mpris_open_uri_playback_events(&command, store.state()),
            vec![
                event(PlaylistCommand::SetPosition(0)),
                event(PlayerCommand::StartCurrentTrack)
            ]
        );
        for event in mpris_open_uri_playback_events(&command, store.state()) {
            let RuntimeEvent::Command(app_command) = event else {
                panic!("expected command")
            };
            store.dispatch(app_command);
        }
        assert_eq!(store.state().playlist.position(), Some(0));
        assert_eq!(
            store.state().player.state(),
            crate::player::PlayerState::Playing
        );
        assert_eq!(
            mpris_service_events(
                &command,
                store.state(),
                translation.properties_before.as_ref()
            ),
            vec![
                MprisEvent::MetadataChanged,
                MprisEvent::PlaybackStatusChanged
            ]
        );
        let empty = AppState::default();
        assert!(mpris_open_uri_playback_events(&command, &empty).is_empty());
        assert!(mpris_service_events(&command, &empty, None).is_empty());
    }
}
