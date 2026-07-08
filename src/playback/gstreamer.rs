//! GStreamer playback backend adapter.
//!
//! This adapter intentionally delegates to the existing `crate::player` backend
//! while the UI separation proceeds incrementally.

use crate::playback::backend::PlaybackBackend;
use crate::playback::model::{
    EqualizerBackendState, OutputDevice, OutputDeviceGroups, OutputDeviceSelection, PlaybackEvent,
    PlayerState, StreamInfo,
};
use crate::player::GStreamerBackend;

impl PlaybackBackend for GStreamerBackend {
    fn play_uri(&self, uri: &str) -> Result<(), String> {
        GStreamerBackend::play_uri(self, uri)
    }

    fn pause(&self) -> Result<(), String> {
        GStreamerBackend::pause(self)
    }

    fn unpause(&self) -> Result<(), String> {
        GStreamerBackend::unpause(self)
    }

    fn stop(&self) -> Result<(), String> {
        GStreamerBackend::stop(self)
    }

    fn seek(&self, position_ms: i64) -> Result<(), String> {
        GStreamerBackend::seek_to_ms(self, position_ms)
    }

    fn set_volume(&self, volume: i32) -> Result<(), String> {
        self.set_volume_percent(volume);
        Ok(())
    }

    fn set_balance(&self, balance: i32) -> Result<(), String> {
        self.set_balance_percent(balance);
        Ok(())
    }

    fn set_equalizer(&self, state: EqualizerBackendState) -> Result<(), String> {
        self.set_equalizer_from_positions(
            state.active,
            state.preamp_position,
            state.band_positions,
        );
        Ok(())
    }

    fn poll_events(&self) -> Result<Vec<PlaybackEvent>, String> {
        GStreamerBackend::poll_bus_events(self)
    }

    fn position_ms(&self) -> Option<i64> {
        GStreamerBackend::position_ms(self)
    }

    fn duration_ms(&self) -> Option<i64> {
        GStreamerBackend::duration_ms(self)
    }

    fn stream_info(&self) -> StreamInfo {
        GStreamerBackend::audio_stream_info(self)
    }

    fn state(&self) -> PlayerState {
        GStreamerBackend::playback_state(self)
    }

    fn current_uri(&self) -> Option<String> {
        GStreamerBackend::uri(self)
    }

    fn output_device_groups(&self) -> OutputDeviceGroups {
        crate::player::group_output_devices(
            crate::player::list_gstreamer_output_devices().unwrap_or_default(),
        )
    }

    fn select_output_device(&mut self, selection: OutputDeviceSelection<'_>) -> Result<(), String> {
        match selection {
            OutputDeviceSelection::Automatic => self.rebuild_output_sink("autoaudiosink", None),
            OutputDeviceSelection::System(id) => self.rebuild_output_sink("autoaudiosink", Some(id)),
        }
    }

    fn current_output_device(&self) -> Option<OutputDevice> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_backend_trait<T: PlaybackBackend>() {}

    #[test]
    fn gstreamer_backend_implements_playback_backend_trait() {
        assert_backend_trait::<GStreamerBackend>();
    }
}
