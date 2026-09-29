//! Playback backend and visualization state shared across frontend boundaries.

use crate::app::effect::PlaybackEffect;
use crate::app_log_info;
use crate::audio_model::SpectrumLayout;
use crate::playback::backend::PlaybackBackend;
#[cfg(all(not(test), not(target_os = "android")))]
use crate::playback::backend::{create_backend, PlaybackBackendKind};
use crate::playback::model::{EqualizerBackendState, PlaybackEvent, StreamInfo};
#[cfg(any(feature = "gtk-ui", test))]
use crate::playback::model::{OutputDeviceGroups, OutputDeviceSelection};
use crate::render::VisualizationRenderState;
use crate::skin::widget::{VisAnalyzerStyle, VisMode, Visualization, WidgetId};

pub struct PlaybackRuntime {
    backend: Option<Box<dyn PlaybackBackend>>,
    #[cfg(any(not(target_os = "android"), test))]
    auto_initialize_backend: bool,
    visualization: Visualization,
    visualization_tick_counter: i32,
}

impl PlaybackRuntime {
    pub fn new(backend: Option<Box<dyn PlaybackBackend>>) -> Self {
        Self {
            backend,
            #[cfg(any(not(target_os = "android"), test))]
            auto_initialize_backend: true,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
        }
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn use_installed_backend_only(&mut self) {
        self.auto_initialize_backend = false;
    }

    pub(crate) fn has_backend(&self) -> bool {
        self.backend.is_some()
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn install_gtk_backend(
        &mut self,
        mut backend: Box<dyn PlaybackBackend>,
        output_device: Option<&str>,
        volume: i32,
        balance: i32,
        equalizer: EqualizerBackendState,
    ) -> (OutputDeviceGroups, Option<String>) {
        // Device selection may rebuild the sink, so apply DSP only afterward.
        let selection = output_device
            .map(OutputDeviceSelection::System)
            .unwrap_or(OutputDeviceSelection::Automatic);
        let selection_error = backend.select_output_device(selection).err();
        let _ = backend.set_volume(volume);
        let _ = backend.set_balance(balance);
        let _ = backend.set_equalizer(equalizer);
        let groups = backend.output_device_groups();
        self.backend = Some(backend);
        (groups, selection_error)
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn select_output_device(
        &mut self,
        device: Option<&str>,
    ) -> Option<(Result<(), String>, OutputDeviceGroups)> {
        let backend = self.backend.as_mut()?;
        let selection = device
            .map(OutputDeviceSelection::System)
            .unwrap_or(OutputDeviceSelection::Automatic);
        let result = backend.select_output_device(selection);
        Some((result, backend.output_device_groups()))
    }

    pub(crate) fn poll_events(&self) -> Option<Result<Vec<PlaybackEvent>, String>> {
        let backend = self.backend.as_ref()?;
        let layout = if self.visualization.mode() == VisMode::Analyzer
            && self.visualization.analyzer_style() == VisAnalyzerStyle::Bars
        {
            SpectrumLayout::AnalyzerBars
        } else {
            SpectrumLayout::Lines
        };
        backend.set_spectrum_layout(layout);
        Some(backend.poll_events())
    }

    pub(crate) fn stream_info(&self) -> Option<StreamInfo> {
        self.backend.as_ref().map(|backend| backend.stream_info())
    }

    pub(crate) fn duration_ms(&self) -> Option<i64> {
        self.backend
            .as_ref()
            .and_then(|backend| backend.duration_ms())
    }

    pub(crate) fn seek(&mut self, position_ms: i64) -> Option<Result<(), String>> {
        let backend = self.backend.as_ref()?;
        Some(backend.seek(position_ms))
    }

    pub(crate) fn visualization_render_state(
        &self,
        vu_mode: crate::skin::widget::VisVuMode,
    ) -> VisualizationRenderState {
        VisualizationRenderState {
            mode: self.visualization.mode(),
            analyzer_style: self.visualization.analyzer_style(),
            analyzer_mode: self.visualization.analyzer_mode(),
            scope_mode: self.visualization.scope_mode(),
            peaks_enabled: self.visualization.peaks_enabled(),
            vu_mode,
            data: *self.visualization.data(),
            peak: *self.visualization.peak(),
            milkdrop_energy: self.visualization.milkdrop_energy(),
            milkdrop_phase: self.visualization.milkdrop_phase(),
        }
    }

    pub(crate) fn apply_visualization_preferences(&mut self, config: &crate::config::Config) {
        self.visualization.set_mode(config.vis_mode);
        self.visualization
            .set_analyzer_mode(config.vis_analyzer_mode);
        self.visualization
            .set_analyzer_style(config.vis_analyzer_style);
        self.visualization.set_scope_mode(config.vis_scope_mode);
        self.visualization
            .set_peaks_enabled(config.vis_peaks_enabled);
        self.visualization
            .set_falloff(config.vis_analyzer_falloff, config.vis_peaks_falloff);
    }

    pub(crate) fn reset_visualization_tick(&mut self) {
        self.visualization_tick_counter = 0;
    }

    pub(crate) fn tick_visualization(&mut self, data: Option<&[f32]>, steps: usize) {
        self.reset_visualization_tick();
        self.visualization.tick_with_steps(data, steps);
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn advance_visualization_tick(&mut self, divisor: i32, data: Option<&[f32]>) {
        self.visualization_tick_counter += 1;
        if self.visualization_tick_counter >= divisor {
            self.tick_visualization(data, divisor as usize);
        }
    }

    pub(crate) fn apply_effect(
        &mut self,
        effect: &PlaybackEffect,
        equalizer: EqualizerBackendState,
        execute_backend: bool,
    ) -> Vec<String> {
        if !execute_backend {
            if matches!(effect, PlaybackEffect::Stop) {
                self.visualization_tick_counter = 0;
                self.visualization.clear_data();
            }
            return Vec::new();
        }
        // GTK installs its backend after the window opens; Android injects its
        // shared backend at the frontend boundary. Desktop egui may lazy-load.
        #[cfg(all(not(test), not(target_os = "android")))]
        if matches!(effect, PlaybackEffect::StartUri { .. })
            && self.backend.is_none()
            && self.auto_initialize_backend
        {
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
                    app_log_info!(backend, "play_uri", uri, position_ms, pending_seek);

                    backend.play_uri(uri)
                }
                PlaybackEffect::Resume => backend.unpause(),
                PlaybackEffect::Pause => backend.pause(),
                PlaybackEffect::Stop => backend.stop(),
                PlaybackEffect::BeginStopFade { .. } => Ok(()),
                PlaybackEffect::Seek(position_ms) => {
                    app_log_info!(backend, "seek", position_ms);
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

        if matches!(effect, PlaybackEffect::Stop) {
            self.visualization_tick_counter = 0;
            self.visualization.clear_data();
        }
        errors
    }

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn install_backend_with_dsp(
        &mut self,
        backend: Box<dyn PlaybackBackend>,
        balance: i32,
        equalizer: EqualizerBackendState,
    ) -> Result<(), String> {
        backend
            .set_balance(balance)
            .map_err(|err| format!("failed to initialize audio balance: {err}"))?;
        backend
            .set_equalizer(equalizer)
            .map_err(|err| format!("failed to initialize audio equalizer: {err}"))?;
        self.backend = Some(backend);
        Ok(())
    }

    #[cfg(any(feature = "gtk-ui", feature = "egui-ui", test))]
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum BackendCall {
        Balance(i32),
        Equalizer(EqualizerBackendState),
        Play(String),
        Pause,
        Resume,
        Stop,
        Seek(i64),
        Volume(i32),
    }

    struct RecordingBackend {
        calls: Arc<Mutex<Vec<BackendCall>>>,
        fail_on: Option<BackendCall>,
    }

    impl RecordingBackend {
        fn record(&self, call: BackendCall) -> Result<(), String> {
            let fail = self.fail_on.as_ref() == Some(&call);
            self.calls.lock().unwrap().push(call);
            if fail {
                Err("backend failed".into())
            } else {
                Ok(())
            }
        }
    }

    impl PlaybackBackend for RecordingBackend {
        fn play_uri(&self, uri: &str) -> Result<(), String> {
            self.record(BackendCall::Play(uri.into()))
        }

        fn pause(&self) -> Result<(), String> {
            self.record(BackendCall::Pause)
        }

        fn unpause(&self) -> Result<(), String> {
            self.record(BackendCall::Resume)
        }

        fn stop(&self) -> Result<(), String> {
            self.record(BackendCall::Stop)
        }

        fn seek(&self, position_ms: i64) -> Result<(), String> {
            self.record(BackendCall::Seek(position_ms))
        }

        fn set_volume(&self, volume: i32) -> Result<(), String> {
            self.record(BackendCall::Volume(volume))
        }

        fn set_balance(&self, balance: i32) -> Result<(), String> {
            self.record(BackendCall::Balance(balance))
        }

        fn set_equalizer(&self, equalizer: EqualizerBackendState) -> Result<(), String> {
            self.record(BackendCall::Equalizer(equalizer))
        }
    }

    fn equalizer() -> EqualizerBackendState {
        EqualizerBackendState {
            active: true,
            preamp_position: 41,
            band_positions: [10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
        }
    }

    fn recording_runtime(
        fail_on: Option<BackendCall>,
    ) -> (PlaybackRuntime, Arc<Mutex<Vec<BackendCall>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let backend = RecordingBackend {
            calls: Arc::clone(&calls),
            fail_on,
        };
        (PlaybackRuntime::new(Some(Box::new(backend))), calls)
    }

    #[test]
    fn installed_backend_applies_dsp_state_before_first_playback() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let backend = RecordingBackend {
            calls: Arc::clone(&calls),
            fail_on: None,
        };
        let mut runtime = PlaybackRuntime::new(None);

        runtime
            .install_backend_with_dsp(Box::new(backend), -25, equalizer())
            .unwrap();
        assert!(runtime
            .apply_effect(
                &PlaybackEffect::StartUri {
                    uri: "file:///song.ogg".to_string(),
                    position_ms: 0,
                },
                equalizer(),
                true,
            )
            .is_empty());

        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                BackendCall::Balance(-25),
                BackendCall::Equalizer(equalizer()),
                BackendCall::Play("file:///song.ogg".into()),
            ]
        );
    }

    #[test]
    fn failed_dsp_install_does_not_attach_backend() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let backend = RecordingBackend {
            calls: Arc::clone(&calls),
            fail_on: Some(BackendCall::Equalizer(equalizer())),
        };
        let mut runtime = PlaybackRuntime::new(None);

        assert_eq!(
            runtime.install_backend_with_dsp(Box::new(backend), 12, equalizer()),
            Err("failed to initialize audio equalizer: backend failed".into())
        );
        assert!(runtime.backend.is_none());
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                BackendCall::Balance(12),
                BackendCall::Equalizer(equalizer())
            ]
        );
    }

    #[test]
    fn playback_effects_call_backend_without_owning_lifecycle_state() {
        let (mut runtime, calls) = recording_runtime(None);
        let uri = "file:///song.ogg";
        let effects = [
            PlaybackEffect::Start,
            PlaybackEffect::StartFromCurrent,
            PlaybackEffect::StartUri {
                uri: uri.into(),
                position_ms: 123,
            },
            PlaybackEffect::Pause,
            PlaybackEffect::Resume,
            PlaybackEffect::Seek(42),
            PlaybackEffect::SetBackendVolume(60),
            PlaybackEffect::SetBackendBalance(-10),
            PlaybackEffect::SetBackendEqualizer,
            PlaybackEffect::BeginStopFade { start_volume: 60 },
            PlaybackEffect::Stop,
        ];
        for effect in effects {
            assert!(runtime.apply_effect(&effect, equalizer(), true).is_empty());
        }
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                BackendCall::Play(uri.into()),
                BackendCall::Pause,
                BackendCall::Resume,
                BackendCall::Seek(42),
                BackendCall::Volume(60),
                BackendCall::Balance(-10),
                BackendCall::Equalizer(equalizer()),
                BackendCall::Stop,
            ]
        );
        assert!(runtime
            .apply_effect(
                &PlaybackEffect::StartUri {
                    uri: uri.into(),
                    position_ms: 0
                },
                equalizer(),
                true,
            )
            .is_empty());
    }

    #[test]
    fn externally_executed_effects_skip_backend_and_only_stop_clears_visualization() {
        let (mut runtime, calls) = recording_runtime(None);
        runtime.visualization.tick(Some(&[0.5]));
        assert_eq!(runtime.visualization.data()[0], 0.5);
        runtime.visualization_tick_counter = 3;
        assert!(runtime
            .apply_effect(&PlaybackEffect::Pause, equalizer(), false)
            .is_empty());
        assert_eq!(runtime.visualization_tick_counter, 3);
        assert!(runtime
            .apply_effect(
                &PlaybackEffect::BeginStopFade { start_volume: 42 },
                equalizer(),
                false
            )
            .is_empty());
        assert_eq!(runtime.visualization_tick_counter, 3);
        assert_eq!(runtime.visualization.data()[0], 0.5);
        assert!(runtime
            .apply_effect(&PlaybackEffect::Stop, equalizer(), false)
            .is_empty());
        assert_eq!(runtime.visualization_tick_counter, 0);
        assert_eq!(runtime.visualization.data()[0], 0.0);
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn backend_errors_are_returned_for_failed_start() {
        let uri = "file:///song.ogg";
        let (mut runtime, calls) = recording_runtime(Some(BackendCall::Play(uri.into())));
        assert_eq!(
            runtime.apply_effect(
                &PlaybackEffect::StartUri {
                    uri: uri.into(),
                    position_ms: 123
                },
                equalizer(),
                true,
            ),
            vec!["backend failed"]
        );
        assert_eq!(*calls.lock().unwrap(), vec![BackendCall::Play(uri.into())]);

        let (mut runtime, _) = recording_runtime(Some(BackendCall::Volume(50)));
        assert_eq!(runtime.set_output_volume(50), Some("backend failed".into()));
        assert_eq!(
            runtime.apply_effect(&PlaybackEffect::SetBackendVolume(50), equalizer(), true),
            vec!["backend failed"]
        );
    }
}
