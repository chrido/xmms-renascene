//! Frontend-neutral application controller.
//!
//! The controller owns application state transitions. It remains free of GTK
//! widgets, platform windows, and concrete backend objects.

use crate::app::command::AppCommand;
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
            AppCommand::Play => self.play(),
            AppCommand::Pause => self.pause(),
            AppCommand::TogglePause => self.toggle_pause(),
            AppCommand::Stop => self.stop(),
            AppCommand::PreviousTrack => self.previous_track(),
            AppCommand::NextTrack => self.next_track(),
            AppCommand::SeekToMs(position_ms) => self.seek_to(position_ms),
            AppCommand::SetVolume(volume) => {
                self.state.player.set_volume(volume);
                vec![
                    AppEffect::SetBackendVolume(self.state.player.volume()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
            AppCommand::SetBalance(balance) => {
                self.state.player.set_balance(balance);
                vec![
                    AppEffect::SetBackendBalance(self.state.player.balance()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
            AppCommand::ExecutePlaylistMenu { kind, index } => self.execute_playlist_menu(kind, index),
            AppCommand::SortPlaylist(key) => {
                self.state.playlist.sort_by(key);
                self.playlist_changed_effects()
            }
            AppCommand::ReversePlaylist => {
                self.state.playlist.reverse();
                self.playlist_changed_effects()
            }
            AppCommand::RandomizePlaylist => {
                self.state.playlist.randomize();
                self.playlist_changed_effects()
            }
            AppCommand::AddPlaylistUris(uris) => {
                for uri in uris {
                    self.state.playlist.add_uri(uri);
                }
                self.playlist_changed_effects()
            }
            AppCommand::AddPlaylistFiles(paths) => {
                for path in paths {
                    self.state.playlist.add_path(path);
                }
                self.playlist_changed_effects()
            }
            AppCommand::ClearPlaylist => {
                self.state.playlist.clear();
                self.playlist_changed_effects()
            }
            AppCommand::RemoveSelectedPlaylistEntries => {
                self.state.playlist.remove_selected_or_current();
                self.playlist_changed_effects()
            }
            AppCommand::SelectAllPlaylistEntries => {
                self.state.playlist.select_all(true);
                self.playlist_changed_effects()
            }
            AppCommand::InvertPlaylistSelection => {
                self.state.playlist.invert_selection();
                self.playlist_changed_effects()
            }
            _ => Vec::new(),
        }
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
            PlaylistMenuCommand::OpenLocationWindow => vec![AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)],
            PlaylistMenuCommand::OpenDirectoryDialog => vec![AppEffect::OpenFileDialog(FileDialogRequest::AddAudioDirectory)],
            PlaylistMenuCommand::OpenFileDialog => vec![AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)],
            PlaylistMenuCommand::ShowSortMenu => Vec::new(),
            PlaylistMenuCommand::ShowFileInfo => vec![AppEffect::OpenFileInfoDialog],
            PlaylistMenuCommand::OpenOptions => vec![AppEffect::OpenPreferences],
            PlaylistMenuCommand::ClearList => {
                self.state.playlist.clear();
                self.playlist_changed_effects()
            }
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
            PlaylistMenuCommand::SavePlaylist => vec![AppEffect::OpenFileDialog(FileDialogRequest::SavePlaylist)],
            PlaylistMenuCommand::LoadPlaylist => vec![AppEffect::OpenFileDialog(FileDialogRequest::LoadPlaylist)],
        }
    }

    fn playlist_changed_effects(&self) -> Vec<AppEffect> {
        vec![AppEffect::SaveConfig, AppEffect::QueueRender(RenderTarget::Playlist)]
    }

    fn play(&mut self) -> Vec<AppEffect> {
        match self.state.player.state() {
            PlayerState::Paused => {
                self.state.player.unpause();
                vec![AppEffect::ResumePlayback, AppEffect::QueueRender(RenderTarget::All)]
            }
            PlayerState::Stopped => self.start_current_playlist_playback(0),
            PlayerState::Playing => Vec::new(),
        }
    }

    fn pause(&mut self) -> Vec<AppEffect> {
        if self.state.player.state() != PlayerState::Playing {
            return Vec::new();
        }
        self.state.player.pause();
        vec![AppEffect::PausePlayback, AppEffect::QueueRender(RenderTarget::All)]
    }

    fn toggle_pause(&mut self) -> Vec<AppEffect> {
        match self.state.player.state() {
            PlayerState::Playing => self.pause(),
            PlayerState::Paused => self.play(),
            PlayerState::Stopped => Vec::new(),
        }
    }

    fn stop(&mut self) -> Vec<AppEffect> {
        self.state.player.stop();
        self.state.player.clear_visualization_data();
        vec![AppEffect::StopPlayback, AppEffect::QueueRender(RenderTarget::All)]
    }

    fn previous_track(&mut self) -> Vec<AppEffect> {
        let advanced = self.state.playlist.previous();
        if advanced {
            self.start_current_playlist_playback(0)
        } else {
            vec![AppEffect::QueueRender(RenderTarget::All)]
        }
    }

    fn next_track(&mut self) -> Vec<AppEffect> {
        let advanced = self.state.playlist.advance();
        if advanced {
            self.start_current_playlist_playback(0)
        } else {
            vec![AppEffect::QueueRender(RenderTarget::All)]
        }
    }

    fn seek_to(&mut self, position_ms: i64) -> Vec<AppEffect> {
        let position_ms = position_ms.max(0);
        vec![
            AppEffect::SeekPlayback(position_ms),
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn start_current_playlist_playback(&mut self, position_ms: i64) -> Vec<AppEffect> {
        if self.state.playlist.position().is_none() && !self.state.playlist.is_empty() {
            self.state.playlist.set_position(0);
        }
        let Some(position) = self.state.playlist.position() else {
            self.state.player.stop();
            return vec![AppEffect::StopPlayback, AppEffect::QueueRender(RenderTarget::All)];
        };
        let Some(entry) = self.state.playlist.entries().get(position) else {
            self.state.player.stop();
            return vec![AppEffect::StopPlayback, AppEffect::QueueRender(RenderTarget::All)];
        };
        let uri = entry.filename.clone();
        self.state.player.mark_playing();
        vec![
            AppEffect::StartPlaybackUri {
                uri,
                position_ms: position_ms.max(0),
            },
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    pub fn handle_playback_event(&mut self, _event: PlaybackEvent) -> Vec<AppEffect> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::RenderTarget;

    #[test]
    fn controller_volume_command_clamps_and_returns_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AppCommand::SetVolume(150));

        assert_eq!(controller.state().player.volume(), 100);
        assert_eq!(effects[0], AppEffect::SetBackendVolume(100));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn controller_balance_command_clamps_and_returns_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AppCommand::SetBalance(-150));

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

        let effects = controller.handle_command(AppCommand::Play);

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

        let effects = controller.handle_command(AppCommand::NextTrack);

        assert_eq!(controller.state().playlist.position(), Some(1));
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
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
        controller.handle_command(AppCommand::Play);

        let pause_effects = controller.handle_command(AppCommand::Pause);
        assert_eq!(controller.state().player.state(), PlayerState::Paused);
        assert!(pause_effects.contains(&AppEffect::PausePlayback));

        let resume_effects = controller.handle_command(AppCommand::TogglePause);
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
        assert!(resume_effects.contains(&AppEffect::ResumePlayback));
    }

    #[test]
    fn playlist_menu_commands_mutate_playlist_state() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(AppCommand::ExecutePlaylistMenu {
            kind: crate::playlist::PlaylistMenuKind::Select,
            index: 0,
        });

        assert!(controller.state().playlist.entries().iter().all(|entry| entry.selected));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::Playlist)));
    }

    #[test]
    fn add_playlist_uris_command_preserves_current_position() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.set_position(0);
        let mut controller = AppController::new(state);

        controller.handle_command(AppCommand::AddPlaylistUris(vec!["file:///tmp/two.ogg".to_string()]));

        assert_eq!(controller.state().playlist.position(), Some(0));
        assert_eq!(controller.state().playlist.len(), 2);
    }
}
