//! GStreamer playback backend adapter.
//!
//! This adapter intentionally delegates to the existing `crate::player` backend
//! while the UI separation proceeds incrementally.

use crate::playback::backend::PlaybackBackend;
use crate::playback::model::EqualizerBackendState;
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
        self.set_equalizer_from_positions(state.active, state.preamp_position, state.band_positions);
        Ok(())
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
