//! Frontend/backend-neutral playback data types.

pub use crate::player::{
    OutputDevice, OutputDeviceGroups, OutputDeviceSelection, PlaybackEvent, PlaybackTags,
    PlayerAction, PlayerState, PlayerTransition, StreamInfo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EqualizerBackendState {
    pub active: bool,
    pub preamp_position: i32,
    pub band_positions: crate::audio_model::EqualizerBandPositions,
}
