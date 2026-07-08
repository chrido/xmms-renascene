//! Playback backend abstraction for desktop and mobile implementations.

use crate::playback::model::{
    EqualizerBackendState, OutputDevice, OutputDeviceGroups, OutputDeviceSelection, PlaybackEvent,
    PlayerState, StreamInfo,
};
use crate::playlist::{DurationIndexItem, DurationIndexResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackBackendKind {
    Auto,
    GStreamer,
    Rodio,
}

pub trait PlaybackBackend {
    fn play_uri(&self, uri: &str) -> Result<(), String>;

    fn play_uri_at(&self, uri: &str, start_ms: i64) -> Result<(), String> {
        self.play_uri(uri)?;
        if start_ms > 0 {
            self.seek(start_ms)?;
        }
        Ok(())
    }

    fn pause(&self) -> Result<(), String>;
    fn unpause(&self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn seek(&self, position_ms: i64) -> Result<(), String>;
    fn set_volume(&self, volume: i32) -> Result<(), String>;
    fn set_balance(&self, balance: i32) -> Result<(), String>;
    fn set_equalizer(&self, state: EqualizerBackendState) -> Result<(), String>;

    fn poll_events(&self) -> Result<Vec<PlaybackEvent>, String> {
        Ok(Vec::new())
    }

    fn position_ms(&self) -> Option<i64> {
        None
    }

    fn duration_ms(&self) -> Option<i64> {
        None
    }

    fn stream_info(&self) -> StreamInfo {
        StreamInfo::default()
    }

    fn state(&self) -> PlayerState {
        PlayerState::Stopped
    }

    fn current_uri(&self) -> Option<String> {
        None
    }

    fn output_device_groups(&self) -> OutputDeviceGroups {
        OutputDeviceGroups::default()
    }

    fn select_output_device(&self, _selection: OutputDeviceSelection<'_>) -> Result<(), String> {
        Ok(())
    }

    fn current_output_device(&self) -> Option<OutputDevice> {
        None
    }
}

pub trait AudioMetadataProbe {
    fn probe(&self, item: &DurationIndexItem) -> Result<Option<DurationIndexResult>, String>;
}

pub struct NoopMetadataProbe;

impl AudioMetadataProbe for NoopMetadataProbe {
    fn probe(&self, _item: &DurationIndexItem) -> Result<Option<DurationIndexResult>, String> {
        Ok(None)
    }
}

pub fn create_backend(kind: PlaybackBackendKind) -> Result<Box<dyn PlaybackBackend>, String> {
    match resolve_backend_kind(kind)? {
        #[cfg(feature = "gstreamer-backend")]
        PlaybackBackendKind::GStreamer => Ok(Box::new(crate::player::GStreamerBackend::new()?)),
        #[cfg(feature = "rodio-backend")]
        PlaybackBackendKind::Rodio => Ok(Box::new(crate::playback::rodio::RodioBackend::new()?)),
        PlaybackBackendKind::Auto => unreachable!("Auto backend kind must be resolved"),
        unsupported => Err(format!(
            "playback backend {unsupported:?} is not enabled in this build"
        )),
    }
}

pub fn resolve_backend_kind(kind: PlaybackBackendKind) -> Result<PlaybackBackendKind, String> {
    match kind {
        PlaybackBackendKind::Auto => default_backend_kind(),
        other => Ok(other),
    }
}

fn default_backend_kind() -> Result<PlaybackBackendKind, String> {
    if cfg!(target_os = "android") && cfg!(feature = "rodio-backend") {
        return Ok(PlaybackBackendKind::Rodio);
    }
    if cfg!(feature = "gstreamer-backend") {
        return Ok(PlaybackBackendKind::GStreamer);
    }
    if cfg!(feature = "rodio-backend") {
        return Ok(PlaybackBackendKind::Rodio);
    }
    Err("no playback backend feature is enabled".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_metadata_probe_returns_no_result() {
        let probe = NoopMetadataProbe;
        let item = DurationIndexItem {
            index: 0,
            uri: "file:///tmp/missing.wav".to_string(),
        };
        assert_eq!(probe.probe(&item).unwrap(), None);
    }

    #[test]
    fn explicit_backend_kind_resolves_to_itself() {
        assert_eq!(
            resolve_backend_kind(PlaybackBackendKind::Rodio).unwrap(),
            PlaybackBackendKind::Rodio
        );
        assert_eq!(
            resolve_backend_kind(PlaybackBackendKind::GStreamer).unwrap(),
            PlaybackBackendKind::GStreamer
        );
    }
}
