//! Frontend-neutral effect types requested by application logic.
//!
//! Effects describe work that must be performed by a concrete frontend or
//! platform runtime, such as starting playback, opening a dialog, or queuing a
//! redraw.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    Main,
    Playlist,
    Equalizer,
    DockedPanels,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDialogRequest {
    AddAudioFiles,
    AddAudioDirectory,
    LoadPlaylist,
    SavePlaylist,
    LoadEqualizerPreset,
    SaveEqualizerPreset,
    ImportSkin,
    ExportSkin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppEffect {
    StartPlayback,
    StartPlaybackFromCurrent,
    StartPlaybackUri { uri: String, position_ms: i64 },
    ResumePlayback,
    PausePlayback,
    StopPlayback,
    SeekPlayback(i64),
    SetBackendVolume(i32),
    SetBackendBalance(i32),
    SetBackendEqualizer,
    SaveConfig,
    QueueRender(RenderTarget),
    OpenFileDialog(FileDialogRequest),
    OpenPath(PathBuf),
    OpenFileInfoDialog,
    OpenPreferences,
    OpenSkinBrowser,
    OpenSkinEditor,
    ShowError(String),
    ShowMessage(String),
}
