//! Playback backend and visualization runtime for the egui frontend.

use crate::app_log_info;
use crate::playback::backend::PlaybackBackend;
#[cfg(not(test))]
use crate::playback::backend::{create_backend, PlaybackBackendKind};
use crate::playback::model::EqualizerBackendState;
use crate::skin::widget::{Visualization, WidgetId};

use super::effect_executor::PlaybackEffect;

pub struct PlaybackRuntime {
    pub(crate) backend: Option<Box<dyn PlaybackBackend>>,
    pub(crate) pending_seek_ms: Option<i64>,
    pub(crate) visualization: Visualization,
    pub(crate) visualization_tick_counter: i32,
}

impl PlaybackRuntime {
    pub fn new(backend: Option<Box<dyn PlaybackBackend>>) -> Self {
        Self {
            backend,
            pending_seek_ms: None,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
        }
    }

    pub(crate) fn apply_effect(
        &mut self,
        effect: &PlaybackEffect,
        equalizer: EqualizerBackendState,
        execute_backend: bool,
    ) -> Vec<String> {
        if !execute_backend {
            if matches!(
                effect,
                PlaybackEffect::Stop | PlaybackEffect::BeginStopFade { .. }
            ) {
                self.visualization_tick_counter = 0;
                self.visualization.clear_data();
            }
            return Vec::new();
        }
        #[cfg(not(test))]
        if matches!(effect, PlaybackEffect::StartUri { .. }) && self.backend.is_none() {
            match create_backend(PlaybackBackendKind::Auto) {
                Ok(backend) => self.backend = Some(backend),
                Err(err) => return vec![format!("failed to initialize audio output: {err}")],
            }
        }

        let mut errors = Vec::new();
        if let Some(backend) = &self.backend {
            let result = match effect {
                PlaybackEffect::StartUri { uri, position_ms } => {
                    let pending_seek = *position_ms > 0;
                    app_log_info!(backend, "egui play_uri", uri, position_ms, pending_seek);
                    let result = backend.play_uri(uri);
                    if result.is_ok() {
                        self.pending_seek_ms = pending_seek.then_some(*position_ms);
                    }
                    result
                }
                PlaybackEffect::Resume => backend.unpause(),
                PlaybackEffect::Pause => backend.pause(),
                PlaybackEffect::Stop | PlaybackEffect::BeginStopFade { .. } => backend.stop(),
                PlaybackEffect::Seek(position_ms) => {
                    app_log_info!(backend, "egui seek", position_ms);
                    backend.seek(*position_ms)
                }
                PlaybackEffect::SetBackendVolume(volume) => backend.set_volume(*volume),
                PlaybackEffect::SetBackendBalance(balance) => backend.set_balance(*balance),
                PlaybackEffect::SetBackendEqualizer => backend.set_equalizer(equalizer),
                PlaybackEffect::Start | PlaybackEffect::StartFromCurrent => Ok(()),
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }

        if matches!(
            effect,
            PlaybackEffect::Stop | PlaybackEffect::BeginStopFade { .. }
        ) {
            self.visualization_tick_counter = 0;
            self.visualization.clear_data();
        }
        errors
    }

    pub fn set_output_volume(&self, volume: i32) -> Option<String> {
        self.backend
            .as_ref()
            .and_then(|backend| backend.set_volume(volume).err())
    }

    pub fn position_ms(&self) -> Option<i64> {
        self.backend
            .as_ref()
            .and_then(|backend| backend.position_ms())
    }
}
