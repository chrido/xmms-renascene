//! Frontend-neutral user/application command types.
//!
//! Concrete frontends translate platform events into these commands. The
//! controller owns the state transition and returns effects for the frontend to
//! perform.

use std::path::PathBuf;

use crate::playlist::{PlaylistMenuKind, PlaylistSortKey};

#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    Player(PlayerCommand),
    Audio(AudioCommand),
    Playlist(PlaylistCommand),
    Equalizer(EqualizerCommand),
    Panel(PanelCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerCommand {
    Play,
    Pause,
    Stop,
    TogglePause,
    PreviousTrack,
    NextTrack,
    SeekToMs(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCommand {
    SetVolume(i32),
    SetBalance(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaylistCommand {
    ToggleShuffle,
    ToggleRepeat,
    ToggleNoAdvance,
    SetSize {
        width: i32,
        height: i32,
    },
    ExecuteMenu {
        kind: PlaylistMenuKind,
        index: usize,
    },
    Sort(PlaylistSortKey),
    SortSelected(PlaylistSortKey),
    Reverse,
    Randomize,
    AddUris(Vec<String>),
    AddFiles(Vec<PathBuf>),
    Clear,
    RemoveSelectedOrCurrent,
    RemoveDead,
    PhysicallyDeleteSelected,
    SelectAll,
    SelectNone,
    InvertSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EqualizerCommand {
    SetActive(bool),
    ToggleActive,
    SetAuto(bool),
    ToggleAuto,
    SetPreamp(i32),
    SetBand { band: usize, position: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelCommand {
    ToggleMainShade,
    SetMainShade(bool),
    TogglePlaylistVisibility,
    SetPlaylistVisibility(bool),
    TogglePlaylistShade,
    SetPlaylistShade(bool),
    TogglePlaylistDetached,
    SetPlaylistDetached(bool),
    ToggleEqualizerVisibility,
    SetEqualizerVisibility(bool),
    ToggleEqualizerShade,
    SetEqualizerShade(bool),
    ToggleEqualizerDetached,
    SetEqualizerDetached(bool),
}

impl From<PlayerCommand> for AppCommand {
    fn from(command: PlayerCommand) -> Self {
        Self::Player(command)
    }
}

impl From<AudioCommand> for AppCommand {
    fn from(command: AudioCommand) -> Self {
        Self::Audio(command)
    }
}

impl From<PlaylistCommand> for AppCommand {
    fn from(command: PlaylistCommand) -> Self {
        Self::Playlist(command)
    }
}

impl From<EqualizerCommand> for AppCommand {
    fn from(command: EqualizerCommand) -> Self {
        Self::Equalizer(command)
    }
}

impl From<PanelCommand> for AppCommand {
    fn from(command: PanelCommand) -> Self {
        Self::Panel(command)
    }
}
