use crate::config::Config;
use crate::player::{Player, PlayerState};
use crate::playlist::Playlist;

/// Live application state.
///
/// `Player` owns runtime volume and balance. `Playlist` owns runtime shuffle,
/// repeat, no-advance, and position. The matching `Config` fields are startup
/// inputs and serialized output fields only; use [`AppState::persistence_snapshot`]
/// when writing them.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub config: Config,
    pub player: Player,
    pub playlist: Playlist,
    pub ui: AppUiState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppUiState {
    pub preferences_visible: bool,
    pub main_menu_visible: bool,
    pub skin_browser_visible: bool,
    pub file_info_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    pub player_state: PlayerState,
    pub playlist_position: Option<usize>,
    pub playlist_len: usize,
    pub playlist_visible: bool,
    pub playlist_detached: bool,
    pub equalizer_visible: bool,
    pub equalizer_detached: bool,
}

#[derive(Debug)]
pub struct PersistenceSnapshot<'a> {
    pub config: Config,
    pub playlist: &'a Playlist,
}

impl PersistenceSnapshot<'_> {
    pub fn with_playback_position(mut self, position_ms: i64) -> Self {
        self.config.playback_position_ms = position_ms.max(0);
        self
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::from_config(Config::default())
    }
}

impl AppState {
    pub fn from_config(config: Config) -> Self {
        let mut player = Player::default();
        player.set_volume(config.volume);
        player.set_balance(config.balance);

        let mut playlist = Playlist::new();
        playlist.set_shuffle(config.shuffle);
        playlist.set_repeat(config.repeat);
        playlist.set_no_advance(config.no_playlist_advance);

        Self {
            config,
            player,
            playlist,
            ui: AppUiState::default(),
        }
    }

    pub(crate) fn apply_config_to_runtime(&mut self) {
        self.player.set_volume(self.config.volume);
        self.player.set_balance(self.config.balance);
        self.playlist.set_shuffle(self.config.shuffle);
        self.playlist.set_repeat(self.config.repeat);
        self.playlist
            .set_no_advance(self.config.no_playlist_advance);
        if self.config.playlist_position >= 0 {
            self.playlist
                .set_position(self.config.playlist_position as usize);
        }
    }

    pub fn persistence_snapshot(&self) -> PersistenceSnapshot<'_> {
        let mut config = self.config.clone();
        config.volume = self.player.volume();
        config.balance = self.player.balance();
        config.shuffle = self.playlist.shuffle();
        config.repeat = self.playlist.repeat();
        config.no_playlist_advance = self.playlist.no_advance();
        config.playlist_position = self
            .playlist
            .position()
            .map_or(-1, |position| position.min(i32::MAX as usize) as i32);
        PersistenceSnapshot {
            config,
            playlist: &self.playlist,
        }
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            player_state: self.player.state(),
            playlist_position: self.playlist.position(),
            playlist_len: self.playlist.len(),
            playlist_visible: self.config.playlist_visible,
            playlist_detached: self.config.playlist_detached,
            equalizer_visible: self.config.equalizer_visible,
            equalizer_detached: self.config.equalizer_detached,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_applies_config_to_runtime_models() {
        let config = Config {
            volume: 33,
            balance: -25,
            shuffle: true,
            repeat: true,
            no_playlist_advance: true,
            ..Config::default()
        };

        let state = AppState::from_config(config);
        assert_eq!(state.player.volume(), 33);
        assert_eq!(state.player.balance(), -25);
        assert!(state.playlist.shuffle());
        assert!(state.playlist.repeat());
        assert!(state.playlist.no_advance());
    }

    #[test]
    fn persistence_snapshot_projects_runtime_owners_without_mutating_app_state() {
        let mut state = AppState::default();
        let original_config = state.config.clone();
        state.player.set_volume(44);
        state.player.set_balance(20);
        state.playlist.set_shuffle(true);
        state.playlist.set_repeat(true);
        state.playlist.set_no_advance(true);
        state.playlist.add_uri("file:///tmp/song.mp3");
        state.playlist.set_position(0);

        let snapshot = state.persistence_snapshot();

        assert_eq!(snapshot.config.volume, 44);
        assert_eq!(snapshot.config.balance, 20);
        assert!(snapshot.config.shuffle);
        assert!(snapshot.config.repeat);
        assert!(snapshot.config.no_playlist_advance);
        assert_eq!(snapshot.config.playlist_position, 0);
        assert_eq!(state.config, original_config);
    }
}
