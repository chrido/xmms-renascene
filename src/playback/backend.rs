//! Playback backend abstraction for desktop and future mobile implementations.

use crate::playback::model::EqualizerBackendState;

pub trait PlaybackBackend {
    fn play_uri(&self, uri: &str) -> Result<(), String>;
    fn pause(&self) -> Result<(), String>;
    fn unpause(&self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn seek(&self, position_ms: i64) -> Result<(), String>;
    fn set_volume(&self, volume: i32) -> Result<(), String>;
    fn set_balance(&self, balance: i32) -> Result<(), String>;
    fn set_equalizer(&self, state: EqualizerBackendState) -> Result<(), String>;
}
