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
    ImportPlaylist,
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
    BeginStopFade { start_volume: i32 },
    SeekPlayback(i64),
    SetOutputVolume(i32),
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

/// The single execution category of an application effect; frontends own its policy.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EffectOwner {
    Playback(PlaybackEffect),
    Ui(UiEffect),
    Platform(PlatformEffect),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PlaybackEffect {
    Start,
    StartFromCurrent,
    StartUri { uri: String, position_ms: i64 },
    Resume,
    Pause,
    Stop,
    BeginStopFade { start_volume: i32 },
    Seek(i64),
    SetBackendVolume(i32),
    SetBackendBalance(i32),
    SetBackendEqualizer,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UiEffect {
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PlatformEffect {
    SetOutputVolume(i32),
    SaveConfig,
}

/// Classifies without executing; platform-specific behavior remains in the frontend.
pub(crate) fn owner(effect: AppEffect) -> EffectOwner {
    match effect {
        AppEffect::StartPlayback => EffectOwner::Playback(PlaybackEffect::Start),
        AppEffect::StartPlaybackFromCurrent => {
            EffectOwner::Playback(PlaybackEffect::StartFromCurrent)
        }
        AppEffect::StartPlaybackUri { uri, position_ms } => {
            EffectOwner::Playback(PlaybackEffect::StartUri { uri, position_ms })
        }
        AppEffect::ResumePlayback => EffectOwner::Playback(PlaybackEffect::Resume),
        AppEffect::PausePlayback => EffectOwner::Playback(PlaybackEffect::Pause),
        AppEffect::StopPlayback => EffectOwner::Playback(PlaybackEffect::Stop),
        AppEffect::BeginStopFade { start_volume } => {
            EffectOwner::Playback(PlaybackEffect::BeginStopFade { start_volume })
        }
        AppEffect::SeekPlayback(position_ms) => {
            EffectOwner::Playback(PlaybackEffect::Seek(position_ms))
        }
        AppEffect::SetBackendVolume(volume) => {
            EffectOwner::Playback(PlaybackEffect::SetBackendVolume(volume))
        }
        AppEffect::SetBackendBalance(balance) => {
            EffectOwner::Playback(PlaybackEffect::SetBackendBalance(balance))
        }
        AppEffect::SetBackendEqualizer => {
            EffectOwner::Playback(PlaybackEffect::SetBackendEqualizer)
        }
        AppEffect::SetOutputVolume(volume) => {
            EffectOwner::Platform(PlatformEffect::SetOutputVolume(volume))
        }
        AppEffect::SaveConfig => EffectOwner::Platform(PlatformEffect::SaveConfig),
        AppEffect::QueueRender(target) => EffectOwner::Ui(UiEffect::QueueRender(target)),
        AppEffect::OpenFileDialog(request) => EffectOwner::Ui(UiEffect::OpenFileDialog(request)),
        AppEffect::OpenPath(path) => EffectOwner::Ui(UiEffect::OpenPath(path)),
        AppEffect::OpenFileInfoDialog => EffectOwner::Ui(UiEffect::OpenFileInfoDialog),
        AppEffect::OpenPreferences => EffectOwner::Ui(UiEffect::OpenPreferences),
        AppEffect::OpenSkinBrowser => EffectOwner::Ui(UiEffect::OpenSkinBrowser),
        AppEffect::OpenSkinEditor => EffectOwner::Ui(UiEffect::OpenSkinEditor),
        AppEffect::ShowError(message) => EffectOwner::Ui(UiEffect::ShowError(message)),
        AppEffect::ShowMessage(message) => EffectOwner::Ui(UiEffect::ShowMessage(message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_effect_has_exactly_one_owner() {
        // The exhaustive owner match enforces coverage of new AppEffect variants.
        let cases = [
            (
                AppEffect::StartPlayback,
                EffectOwner::Playback(PlaybackEffect::Start),
            ),
            (
                AppEffect::StartPlaybackFromCurrent,
                EffectOwner::Playback(PlaybackEffect::StartFromCurrent),
            ),
            (
                AppEffect::StartPlaybackUri {
                    uri: "song".into(),
                    position_ms: 25,
                },
                EffectOwner::Playback(PlaybackEffect::StartUri {
                    uri: "song".into(),
                    position_ms: 25,
                }),
            ),
            (
                AppEffect::ResumePlayback,
                EffectOwner::Playback(PlaybackEffect::Resume),
            ),
            (
                AppEffect::PausePlayback,
                EffectOwner::Playback(PlaybackEffect::Pause),
            ),
            (
                AppEffect::StopPlayback,
                EffectOwner::Playback(PlaybackEffect::Stop),
            ),
            (
                AppEffect::BeginStopFade { start_volume: 42 },
                EffectOwner::Playback(PlaybackEffect::BeginStopFade { start_volume: 42 }),
            ),
            (
                AppEffect::SeekPlayback(100),
                EffectOwner::Playback(PlaybackEffect::Seek(100)),
            ),
            (
                AppEffect::SetBackendVolume(43),
                EffectOwner::Playback(PlaybackEffect::SetBackendVolume(43)),
            ),
            (
                AppEffect::SetBackendBalance(-4),
                EffectOwner::Playback(PlaybackEffect::SetBackendBalance(-4)),
            ),
            (
                AppEffect::SetBackendEqualizer,
                EffectOwner::Playback(PlaybackEffect::SetBackendEqualizer),
            ),
            (
                AppEffect::SetOutputVolume(51),
                EffectOwner::Platform(PlatformEffect::SetOutputVolume(51)),
            ),
            (
                AppEffect::SaveConfig,
                EffectOwner::Platform(PlatformEffect::SaveConfig),
            ),
            (
                AppEffect::QueueRender(RenderTarget::Main),
                EffectOwner::Ui(UiEffect::QueueRender(RenderTarget::Main)),
            ),
            (
                AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles),
                EffectOwner::Ui(UiEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)),
            ),
            (
                AppEffect::OpenPath(PathBuf::from("song")),
                EffectOwner::Ui(UiEffect::OpenPath(PathBuf::from("song"))),
            ),
            (
                AppEffect::OpenFileInfoDialog,
                EffectOwner::Ui(UiEffect::OpenFileInfoDialog),
            ),
            (
                AppEffect::OpenPreferences,
                EffectOwner::Ui(UiEffect::OpenPreferences),
            ),
            (
                AppEffect::OpenSkinBrowser,
                EffectOwner::Ui(UiEffect::OpenSkinBrowser),
            ),
            (
                AppEffect::OpenSkinEditor,
                EffectOwner::Ui(UiEffect::OpenSkinEditor),
            ),
            (
                AppEffect::ShowError("error".into()),
                EffectOwner::Ui(UiEffect::ShowError("error".into())),
            ),
            (
                AppEffect::ShowMessage("message".into()),
                EffectOwner::Ui(UiEffect::ShowMessage("message".into())),
            ),
        ];
        for (effect, expected) in cases {
            assert_eq!(owner(effect), expected);
        }
    }
}
