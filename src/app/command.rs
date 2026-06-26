//! Frontend-neutral user/application command types.
//!
//! Concrete frontends translate platform events into these commands. The
//! controller owns the state transition and returns effects for the frontend to
//! perform.

use std::path::PathBuf;

use crate::playlist::{PlaylistMenuKind, PlaylistSortKey};

#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    Play,
    Pause,
    Stop,
    TogglePause,
    PreviousTrack,
    NextTrack,
    SeekToMs(i64),
    SetVolume(i32),
    SetBalance(i32),
    ToggleShuffle,
    ToggleRepeat,
    ToggleNoPlaylistAdvance,
    SetEqualizerActive(bool),
    ToggleEqualizerActive,
    SetEqualizerAuto(bool),
    ToggleEqualizerAuto,
    SetEqualizerPreamp(i32),
    SetEqualizerBand { band: usize, position: i32 },
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
    SetPlaylistSize { width: i32, height: i32 },
    ExecutePlaylistMenu { kind: PlaylistMenuKind, index: usize },
    SortPlaylist(PlaylistSortKey),
    ReversePlaylist,
    RandomizePlaylist,
    AddPlaylistUris(Vec<String>),
    AddPlaylistFiles(Vec<PathBuf>),
    ClearPlaylist,
    RemoveSelectedPlaylistEntries,
    SelectAllPlaylistEntries,
    InvertPlaylistSelection,
}
