//! Frontend-neutral application controller.
//!
//! The controller owns application state transitions. It remains free of GTK
//! widgets, platform windows, and concrete backend objects.

use crate::app::command::{
    AppCommand, AudioCommand, EqualizerCommand, PanelCommand, PlayerCommand, PlaylistCommand,
    UiCommand,
};
use crate::app::effect::{AppEffect, FileDialogRequest, RenderTarget};
use crate::app::playlist_actions::PlaylistMenuCommand;
use crate::app_state::AppState;
use crate::player::{PlaybackEvent, PlayerState};

#[derive(Debug, Clone)]
pub struct AppController {
    state: AppState,
}

impl AppController {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn into_state(self) -> AppState {
        self.state
    }

    pub fn handle_command(&mut self, command: AppCommand) -> Vec<AppEffect> {
        match command {
            AppCommand::Player(command) => self.handle_player_command(command),
            AppCommand::Audio(command) => self.handle_audio_command(command),
            AppCommand::Playlist(command) => self.handle_playlist_command(command),
            AppCommand::Equalizer(command) => self.handle_equalizer_command(command),
            AppCommand::Panel(command) => self.handle_panel_command(command),
            AppCommand::Ui(command) => self.handle_ui_command(command),
        }
    }

    fn handle_player_command(&mut self, command: PlayerCommand) -> Vec<AppEffect> {
        match command {
            PlayerCommand::Play => self.play(),
            PlayerCommand::StartCurrentTrack => self.start_current_playlist_playback(0),
            PlayerCommand::Pause => self.pause(),
            PlayerCommand::TogglePause => self.toggle_pause(),
            PlayerCommand::Stop => self.stop(),
            PlayerCommand::PreviousTrack => self.previous_track(),
            PlayerCommand::NextTrack => self.next_track(),
            PlayerCommand::SeekToMs(position_ms) => self.seek_to(position_ms),
        }
    }

    fn handle_audio_command(&mut self, command: AudioCommand) -> Vec<AppEffect> {
        match command {
            AudioCommand::SetVolume(volume) => {
                self.state.player.set_volume(volume);
                vec![
                    AppEffect::SetOutputVolume(self.state.player.volume()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
            AudioCommand::SetBalance(balance) => {
                self.state.player.set_balance(balance);
                vec![
                    AppEffect::SetBackendBalance(self.state.player.balance()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
        }
    }

    fn handle_equalizer_command(&mut self, command: EqualizerCommand) -> Vec<AppEffect> {
        match command {
            EqualizerCommand::SetActive(active) => self.state.config.equalizer_active = active,
            EqualizerCommand::ToggleActive => {
                self.state.config.equalizer_active = !self.state.config.equalizer_active;
            }
            EqualizerCommand::SetAuto(auto) => self.state.config.equalizer_auto = auto,
            EqualizerCommand::ToggleAuto => {
                self.state.config.equalizer_auto = !self.state.config.equalizer_auto;
            }
            EqualizerCommand::SetPreamp(position) => {
                self.state.config.equalizer_preamp_pos = position.clamp(0, 100);
            }
            EqualizerCommand::SetBand { band, position } => {
                if let Some(value) = self.state.config.equalizer_band_pos.get_mut(band) {
                    *value = position.clamp(0, 100);
                }
            }
        }
        vec![
            AppEffect::SetBackendEqualizer,
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::Equalizer),
        ]
    }

    fn handle_playlist_command(&mut self, command: PlaylistCommand) -> Vec<AppEffect> {
        match command {
            PlaylistCommand::ToggleShuffle => {
                self.state
                    .playlist
                    .set_shuffle(!self.state.playlist.shuffle());
                self.playlist_changed_effects()
            }
            PlaylistCommand::ToggleRepeat => {
                self.state
                    .playlist
                    .set_repeat(!self.state.playlist.repeat());
                self.playlist_changed_effects()
            }
            PlaylistCommand::ToggleNoAdvance => {
                self.state
                    .playlist
                    .set_no_advance(!self.state.playlist.no_advance());
                self.playlist_changed_effects()
            }
            PlaylistCommand::SetSize { .. } => Vec::new(),
            PlaylistCommand::ExecuteMenu { kind, index } => self.execute_playlist_menu(kind, index),
            PlaylistCommand::Sort(key) => {
                self.state.playlist.sort_by(key);
                self.playlist_changed_effects()
            }
            PlaylistCommand::SortSelected(key) => {
                self.state.playlist.sort_selected_by(key);
                self.playlist_changed_effects()
            }
            PlaylistCommand::Reverse => {
                self.state.playlist.reverse();
                self.playlist_changed_effects()
            }
            PlaylistCommand::Randomize => {
                self.state.playlist.randomize();
                self.playlist_changed_effects()
            }
            PlaylistCommand::AddUris(uris) => {
                for uri in uris {
                    self.state.playlist.add_uri(uri);
                }
                self.playlist_changed_effects()
            }
            PlaylistCommand::AddLocations(locations) => {
                let mut added = false;
                for location in locations {
                    match self.state.playlist.add_location(&location) {
                        Ok(count) => added |= count > 0,
                        Err(err) => {
                            return vec![AppEffect::ShowError(format!(
                                "failed to add playlist location {location}: {err}"
                            ))];
                        }
                    }
                }
                if added {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::AddFiles(paths) => {
                for path in paths {
                    let _ = self.state.playlist.add_path_or_directory(&path);
                }
                self.playlist_changed_effects()
            }
            PlaylistCommand::Clear => self.clear_playlist(),
            PlaylistCommand::RemoveSelectedOrCurrent => {
                self.state.playlist.remove_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistCommand::RemoveSelected => {
                if self.state.playlist.remove_selected() {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::CropToSelection => {
                self.state.playlist.crop_to_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistCommand::RemoveDead => {
                self.state.playlist.remove_dead_files();
                self.playlist_changed_effects()
            }
            PlaylistCommand::PhysicallyDeleteSelected => {
                match self.state.playlist.physically_delete_selected() {
                    Ok(_) => self.playlist_changed_effects(),
                    Err(err) => vec![AppEffect::ShowError(format!(
                        "failed to delete selected playlist files: {err}"
                    ))],
                }
            }
            PlaylistCommand::SelectAll => {
                self.state.playlist.select_all(true);
                self.playlist_changed_effects()
            }
            PlaylistCommand::SelectNone => {
                self.state.playlist.select_all(false);
                self.playlist_changed_effects()
            }
            PlaylistCommand::InvertSelection => {
                self.state.playlist.invert_selection();
                self.playlist_changed_effects()
            }
            PlaylistCommand::SetPosition(index) => {
                self.state.playlist.set_position(index);
                self.playlist_changed_effects()
            }
            PlaylistCommand::ToggleEntrySelection(index) => {
                if let Some(entry) = self.state.playlist.entries_mut().get_mut(index) {
                    entry.selected = !entry.selected;
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::MoveEntry { from, to } => {
                if self.state.playlist.move_entry(from, to) {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::UpdateTitleForUri { uri, title } => {
                let title = title.trim();
                if title.is_empty() {
                    return Vec::new();
                }
                let mut changed = false;
                for entry in self.state.playlist.entries_mut() {
                    if entry.filename == uri && entry.title != title {
                        entry.title = title.to_string();
                        changed = true;
                    }
                }
                if changed {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn handle_panel_command(&mut self, command: PanelCommand) -> Vec<AppEffect> {
        match command {
            PanelCommand::ToggleMainShade => {
                self.state.config.main_shaded = !self.state.config.main_shaded;
                self.panel_changed_effects()
            }
            PanelCommand::SetMainShade(shaded) => {
                self.state.config.main_shaded = shaded;
                self.panel_changed_effects()
            }
            PanelCommand::TogglePlaylistVisibility => {
                self.state.config.playlist_visible = !self.state.config.playlist_visible;
                self.panel_changed_effects()
            }
            PanelCommand::SetPlaylistVisibility(visible) => {
                self.state.config.playlist_visible = visible;
                self.panel_changed_effects()
            }
            PanelCommand::TogglePlaylistShade => {
                self.state.config.playlist_shaded = !self.state.config.playlist_shaded;
                self.panel_changed_effects()
            }
            PanelCommand::SetPlaylistShade(shaded) => {
                self.state.config.playlist_shaded = shaded;
                self.panel_changed_effects()
            }
            PanelCommand::TogglePlaylistDetached => {
                self.state.config.playlist_detached = !self.state.config.playlist_detached;
                self.panel_changed_effects()
            }
            PanelCommand::SetPlaylistDetached(detached) => {
                self.state.config.playlist_detached = detached;
                self.panel_changed_effects()
            }
            PanelCommand::ToggleEqualizerVisibility => {
                self.state.config.equalizer_visible = !self.state.config.equalizer_visible;
                self.panel_changed_effects()
            }
            PanelCommand::SetEqualizerVisibility(visible) => {
                self.state.config.equalizer_visible = visible;
                self.panel_changed_effects()
            }
            PanelCommand::ToggleEqualizerShade => {
                self.state.config.equalizer_shaded = !self.state.config.equalizer_shaded;
                self.panel_changed_effects()
            }
            PanelCommand::SetEqualizerShade(shaded) => {
                self.state.config.equalizer_shaded = shaded;
                self.panel_changed_effects()
            }
            PanelCommand::ToggleEqualizerDetached => {
                self.state.config.equalizer_detached = !self.state.config.equalizer_detached;
                self.panel_changed_effects()
            }
            PanelCommand::SetEqualizerDetached(detached) => {
                self.state.config.equalizer_detached = detached;
                self.panel_changed_effects()
            }
        }
    }

    fn handle_ui_command(&mut self, command: UiCommand) -> Vec<AppEffect> {
        match command {
            UiCommand::SetPreferencesVisible(visible) => {
                self.state.ui.preferences_visible = visible;
            }
            UiCommand::TogglePreferences => {
                self.state.ui.preferences_visible = !self.state.ui.preferences_visible;
            }
            UiCommand::SetMainMenuVisible(visible) => {
                self.state.ui.main_menu_visible = visible;
            }
            UiCommand::SetSkinBrowserVisible(visible) => {
                self.state.ui.skin_browser_visible = visible;
            }
            UiCommand::ToggleSkinBrowser => {
                self.state.ui.skin_browser_visible = !self.state.ui.skin_browser_visible;
            }
            UiCommand::SetFileInfoVisible(visible) => {
                self.state.ui.file_info_visible = visible;
            }
            UiCommand::ToggleFileInfo => {
                self.state.ui.file_info_visible = !self.state.ui.file_info_visible;
            }
        }
        vec![AppEffect::QueueRender(RenderTarget::All)]
    }

    fn execute_playlist_menu(
        &mut self,
        kind: crate::playlist::PlaylistMenuKind,
        index: usize,
    ) -> Vec<AppEffect> {
        let Some(command) = PlaylistMenuCommand::from_menu_item(kind, index) else {
            return Vec::new();
        };
        match command {
            PlaylistMenuCommand::OpenLocationWindow => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)]
            }
            PlaylistMenuCommand::OpenDirectoryDialog => vec![AppEffect::OpenFileDialog(
                FileDialogRequest::AddAudioDirectory,
            )],
            PlaylistMenuCommand::OpenFileDialog => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)]
            }
            PlaylistMenuCommand::ShowSortMenu => Vec::new(),
            PlaylistMenuCommand::ShowFileInfo => vec![AppEffect::OpenFileInfoDialog],
            PlaylistMenuCommand::OpenOptions => vec![AppEffect::OpenPreferences],
            PlaylistMenuCommand::ClearList => self.clear_playlist(),
            PlaylistMenuCommand::CropToSelection => {
                self.state.playlist.crop_to_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::RemoveSelectedOrCurrent => {
                self.state.playlist.remove_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::InvertSelection => {
                self.state.playlist.invert_selection();
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::SelectNone => {
                self.state.playlist.select_all(false);
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::SelectAll => {
                self.state.playlist.select_all(true);
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::SavePlaylist => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::SavePlaylist)]
            }
            PlaylistMenuCommand::LoadPlaylist => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::LoadPlaylist)]
            }
        }
    }

    fn playlist_changed_effects(&self) -> Vec<AppEffect> {
        vec![
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::Playlist),
        ]
    }

    fn clear_playlist(&mut self) -> Vec<AppEffect> {
        self.state.playlist.clear();
        self.state.player.stop();
        self.state.player.clear_visualization_data();
        self.state.config.playback_position_ms = 0;
        vec![
            AppEffect::StopPlayback,
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn panel_changed_effects(&self) -> Vec<AppEffect> {
        vec![
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn play(&mut self) -> Vec<AppEffect> {
        match self.state.player.state() {
            PlayerState::Paused => {
                self.state.player.unpause();
                vec![
                    AppEffect::ResumePlayback,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
            PlayerState::Stopped => {
                self.start_current_playlist_playback(self.state.config.playback_position_ms.max(0))
            }
            PlayerState::Playing => Vec::new(),
        }
    }

    fn pause(&mut self) -> Vec<AppEffect> {
        if self.state.player.state() != PlayerState::Playing {
            return Vec::new();
        }
        self.state.player.pause();
        vec![
            AppEffect::PausePlayback,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn toggle_pause(&mut self) -> Vec<AppEffect> {
        match self.state.player.state() {
            PlayerState::Playing => self.pause(),
            PlayerState::Paused => self.play(),
            PlayerState::Stopped => Vec::new(),
        }
    }

    fn stop(&mut self) -> Vec<AppEffect> {
        if self.state.config.stop_with_fadeout && self.state.player.state() != PlayerState::Stopped
        {
            let start_volume = self.state.player.volume().max(0);
            if start_volume > 0 {
                return vec![
                    AppEffect::BeginStopFade { start_volume },
                    AppEffect::QueueRender(RenderTarget::All),
                ];
            }
        }
        self.state.player.stop();
        self.state.player.clear_visualization_data();
        vec![
            AppEffect::StopPlayback,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn previous_track(&mut self) -> Vec<AppEffect> {
        self.state.config.playback_position_ms = 0;
        let advanced = self.state.playlist.previous();
        if advanced {
            self.start_current_playlist_playback(0)
        } else {
            vec![
                AppEffect::SeekPlayback(0),
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ]
        }
    }

    fn next_track(&mut self) -> Vec<AppEffect> {
        self.state.config.playback_position_ms = 0;
        let advanced = self.state.playlist.advance();
        if advanced {
            self.start_current_playlist_playback(0)
        } else {
            vec![
                AppEffect::SeekPlayback(0),
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ]
        }
    }

    fn seek_to(&mut self, position_ms: i64) -> Vec<AppEffect> {
        let position_ms = position_ms.max(0);
        self.state.config.playback_position_ms = position_ms;
        vec![
            AppEffect::SeekPlayback(position_ms),
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn start_current_playlist_playback(&mut self, position_ms: i64) -> Vec<AppEffect> {
        if self.state.playlist.position().is_none() && !self.state.playlist.is_empty() {
            self.state.playlist.set_position(0);
        }
        let Some(position) = self.state.playlist.position() else {
            self.state.player.stop();
            return vec![
                AppEffect::StopPlayback,
                AppEffect::QueueRender(RenderTarget::All),
            ];
        };
        let Some(entry) = self.state.playlist.entries().get(position) else {
            self.state.player.stop();
            return vec![
                AppEffect::StopPlayback,
                AppEffect::QueueRender(RenderTarget::All),
            ];
        };
        let uri = entry.filename.clone();
        let position_ms = position_ms.max(0);
        self.state.config.playback_position_ms = position_ms;
        self.state.player.mark_playing();
        vec![
            AppEffect::StartPlaybackUri { uri, position_ms },
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    pub fn handle_playback_event(&mut self, event: PlaybackEvent) -> Vec<AppEffect> {
        if self.state.player.apply_playback_event(&event) {
            vec![AppEffect::QueueRender(RenderTarget::All)]
        } else {
            Vec::new()
        }
    }

    pub fn handle_playlist_eof(&mut self) -> Vec<AppEffect> {
        if self.state.playlist.eof_reached() {
            self.start_current_playlist_playback(0)
        } else {
            self.stop()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::RenderTarget;

    #[test]
    fn controller_volume_command_clamps_and_returns_output_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AudioCommand::SetVolume(150).into());

        assert_eq!(controller.state().player.volume(), 100);
        assert_eq!(effects[0], AppEffect::SetOutputVolume(100));
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::SetBackendVolume(_))));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn equalizer_preamp_remains_a_backend_equalizer_effect() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(EqualizerCommand::SetPreamp(73).into());

        assert_eq!(controller.state().config.equalizer_preamp_pos, 73);
        assert!(effects.contains(&AppEffect::SetBackendEqualizer));
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::SetOutputVolume(_))));
    }

    #[test]
    fn controller_balance_command_clamps_and_returns_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AudioCommand::SetBalance(-150).into());

        assert_eq!(controller.state().player.balance(), -100);
        assert_eq!(effects[0], AppEffect::SetBackendBalance(-100));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn play_from_stopped_selects_first_entry_and_requests_playback() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(PlayerCommand::Play.into());

        assert_eq!(controller.state().playlist.position(), Some(0));
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/one.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn next_track_starts_from_beginning() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        state.playlist.set_position(0);
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(PlayerCommand::NextTrack.into());

        assert_eq!(controller.state().playlist.position(), Some(1));
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/two.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn start_current_track_restarts_selected_entry_while_playing() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        state.playlist.set_position(0);
        let mut controller = AppController::new(state);
        controller.handle_command(PlayerCommand::Play.into());
        controller.handle_command(PlaylistCommand::SetPosition(1).into());

        let effects = controller.handle_command(PlayerCommand::StartCurrentTrack.into());

        assert_eq!(controller.state().playlist.position(), Some(1));
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/two.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn pause_and_toggle_pause_follow_player_state() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        let mut controller = AppController::new(state);
        controller.handle_command(PlayerCommand::Play.into());

        let pause_effects = controller.handle_command(PlayerCommand::Pause.into());
        assert_eq!(controller.state().player.state(), PlayerState::Paused);
        assert!(pause_effects.contains(&AppEffect::PausePlayback));

        let resume_effects = controller.handle_command(PlayerCommand::TogglePause.into());
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
        assert!(resume_effects.contains(&AppEffect::ResumePlayback));
    }

    #[test]
    fn playlist_menu_commands_mutate_playlist_state() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(
            PlaylistCommand::ExecuteMenu {
                kind: crate::playlist::PlaylistMenuKind::Select,
                index: 0,
            }
            .into(),
        );

        assert!(controller
            .state()
            .playlist
            .entries()
            .iter()
            .all(|entry| entry.selected));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::Playlist)));
    }

    #[test]
    fn seek_command_updates_saved_position_and_requests_backend_seek() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(PlayerCommand::SeekToMs(42_000).into());

        assert_eq!(controller.state().config.playback_position_ms, 42_000);
        assert!(effects.contains(&AppEffect::SeekPlayback(42_000)));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn playback_events_update_controller_player_state() {
        let mut controller = AppController::new(AppState::default());

        let effects =
            controller.handle_playback_event(PlaybackEvent::DurationChanged(Some(12_000)));

        assert_eq!(controller.state().player.duration_ms(), Some(12_000));
        assert_eq!(effects, vec![AppEffect::QueueRender(RenderTarget::All)]);
    }

    #[test]
    fn add_playlist_uris_command_preserves_current_position() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.set_position(0);
        let mut controller = AppController::new(state);

        controller.handle_command(
            PlaylistCommand::AddUris(vec!["file:///tmp/two.ogg".to_string()]).into(),
        );

        assert_eq!(controller.state().playlist.position(), Some(0));
        assert_eq!(controller.state().playlist.len(), 2);
    }

    #[test]
    fn clearing_playlist_stops_and_resets_player() {
        for command in [
            AppCommand::from(PlaylistCommand::Clear),
            AppCommand::from(PlaylistCommand::ExecuteMenu {
                kind: crate::playlist::PlaylistMenuKind::List,
                index: 0,
            }),
        ] {
            let mut state = AppState::default();
            state.playlist.add_uri("file:///tmp/one.ogg");
            state.playlist.set_position(0);
            state.player.mark_playing();
            state.config.playback_position_ms = 42_000;
            let mut controller = AppController::new(state);

            let effects = controller.handle_command(command);

            assert!(controller.state().playlist.is_empty());
            assert_eq!(controller.state().playlist.position(), None);
            assert_eq!(controller.state().player.state(), PlayerState::Stopped);
            assert_eq!(controller.state().config.playback_position_ms, 0);
            assert!(effects.contains(&AppEffect::StopPlayback));
            assert!(effects.contains(&AppEffect::SaveConfig));
            assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
        }
    }

    #[test]
    fn panel_commands_update_config_and_request_redraw() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(PanelCommand::SetPlaylistVisibility(true).into());
        controller.handle_command(PanelCommand::ToggleEqualizerShade.into());
        controller.handle_command(PanelCommand::SetPlaylistDetached(true).into());

        assert!(controller.state().config.playlist_visible);
        assert!(controller.state().config.equalizer_shaded);
        assert!(controller.state().config.playlist_detached);
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn equalizer_commands_update_config_and_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(
            EqualizerCommand::SetBand {
                band: 2,
                position: 150,
            }
            .into(),
        );
        controller.handle_command(EqualizerCommand::SetActive(true).into());

        assert_eq!(controller.state().config.equalizer_band_pos[2], 100);
        assert!(controller.state().config.equalizer_active);
        assert!(effects.contains(&AppEffect::SetBackendEqualizer));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::Equalizer)));
    }
}
