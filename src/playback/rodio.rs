//! Rodio playback backend.
//!
//! This backend intentionally only supports platform-independent local playback
//! in the first Android-audio step. Android `content://` and streaming URL
//! support are handled by a later platform URI resolver.

use std::fs::File;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer, Source};

use crate::audio_model::{
    analyzer_spectrum_from_bins, analyzer_spectrum_level, equalizer_position_to_db,
    spectrum_data_for_layout, EqualizerBandPositions, SpectrumData, SpectrumLayout,
    ANALYZER_BAR_COUNT, ANALYZER_FFT_FRAMES, ANALYZER_FREQUENCY_BIN_COUNT, EQUALIZER_BANDS,
    SPECTRUM_BANDS,
};
use crate::playback::backend::{AudioMetadataProbe, PlaybackBackend};
use crate::playback::model::{
    EqualizerBackendState, OutputDevice, OutputDeviceGroups, PlaybackEvent, PlayerState, StreamInfo,
};
use crate::playlist::{DurationIndexItem, DurationIndexResult};

#[derive(Clone)]
pub struct RodioBackend {
    inner: Arc<Mutex<RodioBackendInner>>,
}

struct RodioBackendInner {
    output: RodioOutput,
    current_uri: Option<String>,
    state: PlayerState,
    duration_ms: Option<i64>,
    stream_info: StreamInfo,
    pending_events: Vec<PlaybackEvent>,
    eos_emitted: bool,
    volume: i32,
    balance: i32,
    equalizer: EqualizerBackendState,
    dsp_settings: SharedDspSettings,
    visualization: SharedVisualization,
    visualization_generation: u64,
    spectrum_layout: SpectrumLayout,
    emitted_spectrum_layout: SpectrumLayout,
    output_device: OutputDevice,
    last_debug_log: Option<Instant>,
}

enum RodioOutput {
    Device {
        sink: MixerDeviceSink,
        player: RodioPlayer,
    },
    #[cfg(test)]
    Detached { player: RodioPlayer },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAudioSource {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn peaking(sample_rate: f32, frequency: f32, q: f32, gain_db: f32) -> Self {
        if gain_db.abs() < 0.001 || frequency <= 0.0 || frequency >= sample_rate * 0.5 {
            return Self::identity();
        }
        let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate;
        let sin = omega.sin();
        let cos = omega.cos();
        let alpha = sin / (2.0 * q.max(0.001));
        let a = 10.0_f32.powf(gain_db / 40.0);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

struct RodioDspSource<S> {
    inner: S,
    settings: SharedDspSettings,
    spectrum: RodioSpectrumCapture,
    seen_version: u64,
    preamp_gain: f32,
    left_gain: f32,
    right_gain: f32,
    next_channel: usize,
    frame_sum: f32,
    filters: Vec<[Biquad; EQUALIZER_BANDS]>,
}

impl<S> RodioDspSource<S>
where
    S: Source,
{
    fn new(inner: S, settings: SharedDspSettings, visualization: SharedVisualization) -> Self {
        let channels = usize::from(inner.channels().get()).max(1);
        let sample_rate = inner.sample_rate().get() as f32;
        let mut source = Self {
            inner,
            settings,
            spectrum: RodioSpectrumCapture::new(sample_rate, visualization),
            seen_version: u64::MAX,
            preamp_gain: 1.0,
            left_gain: 1.0,
            right_gain: 1.0,
            next_channel: 0,
            frame_sum: 0.0,
            filters: vec![[Biquad::identity(); EQUALIZER_BANDS]; channels],
        };
        source.refresh_filters_if_needed();
        source
    }

    fn refresh_filters_if_needed(&mut self) {
        let settings = self
            .settings
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        if settings.version == self.seen_version {
            return;
        }
        self.seen_version = settings.version;
        self.preamp_gain = settings.preamp_gain();
        (self.left_gain, self.right_gain) = settings.channel_gains();
        let sample_rate = self.inner.sample_rate().get() as f32;
        let template = std::array::from_fn(|band| {
            Biquad::peaking(
                sample_rate,
                EQ_FREQUENCIES_HZ[band],
                EQ_Q,
                settings.band_db(band),
            )
        });
        for channel_filters in &mut self.filters {
            *channel_filters = template;
        }
    }
}

impl<S> Iterator for RodioDspSource<S>
where
    S: Source,
{
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        self.refresh_filters_if_needed();
        let channel = self.next_channel;
        let mut sample = self.inner.next()? as f32;
        for filter in &mut self.filters[channel] {
            sample = filter.process(sample);
        }
        let channel_gain = if self.filters.len() < 2 {
            1.0
        } else if channel % 2 == 0 {
            self.left_gain
        } else {
            self.right_gain
        };
        sample = (sample * self.preamp_gain * channel_gain).clamp(-1.0, 1.0);
        self.frame_sum += sample;
        self.next_channel = (channel + 1) % self.filters.len().max(1);
        if self.next_channel == 0 {
            self.spectrum
                .push_sample(self.frame_sum / self.filters.len().max(1) as f32);
            self.frame_sum = 0.0;
        }
        Some(sample as rodio::Sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Source for RodioDspSource<S>
where
    S: Source,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.next_channel = 0;
        self.frame_sum = 0.0;
        self.spectrum.reset();
        self.inner.try_seek(pos)
    }
}

pub struct RodioMetadataProbe;

const RODIO_ALSA_PIPEWIRE_ID: &str = "rodio-alsa-pipewire";
const RODIO_ALSA_PULSE_ID: &str = "rodio-alsa-pulse";
const RODIO_ALSA_SYSTEM_ID: &str = "rodio-alsa-system";
const EQ_FREQUENCIES_HZ: [f32; EQUALIZER_BANDS] = [
    60.0, 170.0, 310.0, 600.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0, 14_000.0, 16_000.0,
];
const EQ_Q: f32 = 1.0;

type SharedDspSettings = Arc<Mutex<RodioDspSettings>>;
type SharedVisualization = Arc<Mutex<RodioVisualization>>;

const SPECTRUM_WINDOW_FRAMES: usize = ANALYZER_FFT_FRAMES;

#[derive(Clone, Copy)]
struct RodioSpectrumFrame {
    lines: SpectrumData,
    bars: [f32; ANALYZER_BAR_COUNT],
}

impl RodioSpectrumFrame {
    const SILENT: Self = Self {
        lines: [0.0; SPECTRUM_BANDS],
        bars: [0.0; ANALYZER_BAR_COUNT],
    };

    fn data(self, layout: SpectrumLayout) -> SpectrumData {
        spectrum_data_for_layout(self.lines, self.bars, layout)
    }
}

struct RodioVisualization {
    frame: RodioSpectrumFrame,
    generation: u64,
}

impl RodioVisualization {
    fn new() -> Self {
        Self {
            frame: RodioSpectrumFrame::SILENT,
            generation: 0,
        }
    }

    fn publish(&mut self, frame: RodioSpectrumFrame) {
        self.frame = frame;
        self.generation = self.generation.wrapping_add(1);
    }
}

struct SpectrumWindow {
    generation: u64,
    samples: Vec<f32>,
}

struct RodioSpectrumCapture {
    sender: SyncSender<SpectrumWindow>,
    recycled: Receiver<Vec<f32>>,
    current_generation: Arc<AtomicU64>,
    samples: Vec<f32>,
}

impl RodioSpectrumCapture {
    fn new(sample_rate: f32, shared: SharedVisualization) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<SpectrumWindow>(2);
        let (recycle_sender, recycled) = mpsc::sync_channel(2);
        let current_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&current_generation);
        if let Err(err) = std::thread::Builder::new()
            .name("xmms-spectrum".to_string())
            .spawn(move || {
                while let Ok(mut window) = receiver.recv() {
                    let generation = window.generation;
                    let frame = analyze_spectrum_window(sample_rate, &window.samples);
                    if generation == worker_generation.load(Ordering::Acquire) {
                        shared
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner())
                            .publish(frame);
                    }
                    window.samples.clear();
                    let _ = recycle_sender.try_send(window.samples);
                }
            })
        {
            eprintln!("xmms-rs: failed to start spectrum worker: {err}");
        }
        Self {
            sender,
            recycled,
            current_generation,
            samples: Vec::with_capacity(SPECTRUM_WINDOW_FRAMES),
        }
    }

    fn push_sample(&mut self, sample: f32) {
        self.samples.push(sample);
        if self.samples.len() < SPECTRUM_WINDOW_FRAMES {
            return;
        }

        let replacement = match self.recycled.try_recv() {
            Ok(samples) => samples,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                Vec::with_capacity(SPECTRUM_WINDOW_FRAMES)
            }
        };
        let samples = std::mem::replace(&mut self.samples, replacement);
        let window = SpectrumWindow {
            generation: self.current_generation.load(Ordering::Relaxed),
            samples,
        };
        match self.sender.try_send(window) {
            Ok(()) => {}
            Err(TrySendError::Full(mut window) | TrySendError::Disconnected(mut window)) => {
                window.samples.clear();
                self.samples = window.samples;
            }
        }
    }

    fn reset(&mut self) {
        self.current_generation.fetch_add(1, Ordering::Release);
        self.samples.clear();
    }
}

fn analyze_spectrum_window(_sample_rate: f32, samples: &[f32]) -> RodioSpectrumFrame {
    if samples.is_empty() {
        return RodioSpectrumFrame::SILENT;
    }
    let mut real = [0.0; SPECTRUM_WINDOW_FRAMES];
    let mut imaginary = [0.0; SPECTRUM_WINDOW_FRAMES];
    for (target, sample) in real.iter_mut().zip(samples.iter().copied()) {
        *target = sample;
    }
    fft_in_place(&mut real, &mut imaginary);

    let frequency_bins: [f32; ANALYZER_FREQUENCY_BIN_COUNT] = std::array::from_fn(|index| {
        let fft_bin = index + 1;
        let amplitude =
            2.0 * real[fft_bin].hypot(imaginary[fft_bin]) / SPECTRUM_WINDOW_FRAMES as f32;
        analyzer_spectrum_level(amplitude)
    });
    let (lines, bars) = analyzer_spectrum_from_bins(&frequency_bins);
    RodioSpectrumFrame { lines, bars }
}

fn fft_in_place(
    real: &mut [f32; SPECTRUM_WINDOW_FRAMES],
    imaginary: &mut [f32; SPECTRUM_WINDOW_FRAMES],
) {
    let mut j = 0;
    for i in 1..SPECTRUM_WINDOW_FRAMES {
        let mut bit = SPECTRUM_WINDOW_FRAMES >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            real.swap(i, j);
            imaginary.swap(i, j);
        }
    }

    let mut length = 2;
    while length <= SPECTRUM_WINDOW_FRAMES {
        let angle = -std::f32::consts::TAU / length as f32;
        let (step_imaginary, step_real) = angle.sin_cos();
        for start in (0..SPECTRUM_WINDOW_FRAMES).step_by(length) {
            let mut twiddle_real = 1.0;
            let mut twiddle_imaginary = 0.0;
            for offset in 0..length / 2 {
                let even = start + offset;
                let odd = even + length / 2;
                let odd_real = real[odd] * twiddle_real - imaginary[odd] * twiddle_imaginary;
                let odd_imaginary = real[odd] * twiddle_imaginary + imaginary[odd] * twiddle_real;
                real[odd] = real[even] - odd_real;
                imaginary[odd] = imaginary[even] - odd_imaginary;
                real[even] += odd_real;
                imaginary[even] += odd_imaginary;
                let next_real = twiddle_real * step_real - twiddle_imaginary * step_imaginary;
                twiddle_imaginary = twiddle_real * step_imaginary + twiddle_imaginary * step_real;
                twiddle_real = next_real;
            }
        }
        length <<= 1;
    }
}

#[derive(Debug, Clone)]
struct RodioDspSettings {
    version: u64,
    balance: i32,
    active: bool,
    preamp_position: i32,
    band_positions: EqualizerBandPositions,
}

impl RodioDspSettings {
    fn new(state: EqualizerBackendState, balance: i32) -> Self {
        Self {
            version: 0,
            balance: balance.clamp(-100, 100),
            active: state.active,
            preamp_position: state.preamp_position,
            band_positions: state.band_positions,
        }
    }

    fn update(&mut self, state: EqualizerBackendState) {
        self.version = self.version.wrapping_add(1);
        self.active = state.active;
        self.preamp_position = state.preamp_position;
        self.band_positions = state.band_positions;
    }

    fn update_balance(&mut self, balance: i32) {
        self.version = self.version.wrapping_add(1);
        self.balance = balance.clamp(-100, 100);
    }

    fn channel_gains(&self) -> (f32, f32) {
        let balance = self.balance as f32 / 100.0;
        if balance < 0.0 {
            (1.0, 1.0 + balance)
        } else {
            (1.0 - balance, 1.0)
        }
    }

    fn preamp_gain(&self) -> f32 {
        if self.active {
            db_to_gain(equalizer_position_to_db(self.preamp_position))
        } else {
            1.0
        }
    }

    fn band_db(&self, band: usize) -> f32 {
        if self.active {
            equalizer_position_to_db(self.band_positions[band]) as f32
        } else {
            0.0
        }
    }
}

impl RodioBackend {
    pub fn new() -> Result<Self, String> {
        let (sink, output_device) = open_rodio_sink(None)?;
        let player = RodioPlayer::connect_new(sink.mixer());
        Ok(Self::from_output(
            RodioOutput::Device { sink, player },
            output_device,
        ))
    }

    fn from_output(output: RodioOutput, output_device: OutputDevice) -> Self {
        let equalizer = EqualizerBackendState {
            active: false,
            preamp_position: 0,
            band_positions: [0; EQUALIZER_BANDS],
        };
        let visualization = Arc::new(Mutex::new(RodioVisualization::new()));
        Self {
            inner: Arc::new(Mutex::new(RodioBackendInner {
                output,
                current_uri: None,
                state: PlayerState::Stopped,
                duration_ms: None,
                stream_info: StreamInfo::default(),
                pending_events: Vec::new(),
                eos_emitted: false,
                volume: 100,
                balance: 0,
                equalizer,
                dsp_settings: Arc::new(Mutex::new(RodioDspSettings::new(equalizer, 0))),
                visualization,
                visualization_generation: 0,
                spectrum_layout: SpectrumLayout::AnalyzerBars,
                emitted_spectrum_layout: SpectrumLayout::AnalyzerBars,
                output_device,
                last_debug_log: None,
            })),
        }
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, RodioBackendInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    #[cfg(test)]
    fn new_detached_for_tests() -> Self {
        let (player, _source) = RodioPlayer::new();
        Self::from_output(RodioOutput::Detached { player }, default_output_device())
    }

    #[cfg(test)]
    fn force_player_empty_for_tests(&self) {
        let inner = self.lock_inner();
        inner.output.player().stop();
    }

    pub fn volume(&self) -> i32 {
        self.lock_inner().volume
    }

    pub fn balance(&self) -> i32 {
        self.lock_inner().balance
    }

    pub fn equalizer(&self) -> EqualizerBackendState {
        self.lock_inner().equalizer
    }

    fn stop_current_locked(inner: &mut RodioBackendInner) {
        inner.output.player().stop();
        inner.current_uri = None;
        inner.state = PlayerState::Stopped;
        inner.duration_ms = None;
        inner.stream_info = StreamInfo::default();
        inner.eos_emitted = false;
    }

    fn record_error(&self, message: String) -> Result<(), String> {
        let mut inner = self.lock_inner();
        Self::stop_current_locked(&mut inner);
        inner
            .pending_events
            .push(PlaybackEvent::Error(message.clone()));
        Err(message)
    }
}

impl PlaybackBackend for RodioBackend {
    fn play_uri(&self, uri: &str) -> Result<(), String> {
        self.play_uri_at(uri, 0)
    }

    fn play_uri_at(&self, uri: &str, start_ms: i64) -> Result<(), String> {
        crate::app_log_info!(backend, "rodio play_uri_at", uri, start_ms);
        if start_ms < 0 {
            return Err("seek position must be non-negative".to_string());
        }

        {
            let mut inner = self.lock_inner();
            Self::stop_current_locked(&mut inner);
        }

        let source = match resolve_local_audio_source(uri) {
            Ok(source) => source,
            Err(err) => return self.record_error(err),
        };
        let file = match File::open(&source.path) {
            Ok(file) => file,
            Err(err) => {
                return self.record_error(format!(
                    "failed to open local audio file {}: {err}",
                    source.path.display()
                ));
            }
        };
        let decoder = match Decoder::try_from(file) {
            Ok(decoder) => decoder,
            Err(err) => {
                return self.record_error(format!(
                    "failed to decode local audio file {} with rodio: {err}",
                    source.path.display()
                ));
            }
        };

        let duration_ms = decoder.total_duration().map(duration_to_millis);
        let bitrate = estimate_bitrate_kbps(&source.path, duration_ms);
        let stream_info = StreamInfo {
            bitrate,
            frequency: Some(decoder.sample_rate().get() as i32),
            channels: Some(decoder.channels().get() as i32),
        };
        let path = source.path.display().to_string();
        let duration_ms_log = duration_ms
            .map(|duration| duration.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let bitrate = stream_info.bitrate.unwrap_or_default();
        let frequency = stream_info.frequency.unwrap_or_default();
        let channels = stream_info.channels.unwrap_or_default();
        crate::app_log_info!(
            backend,
            "rodio decoded source",
            path,
            duration_ms_log,
            bitrate,
            frequency,
            channels
        );

        let mut inner = self.lock_inner();
        inner.output.recreate_player();
        let player = inner.output.player();
        player.set_volume(percent_to_rodio_volume(inner.volume));
        let dsp_settings = Arc::clone(&inner.dsp_settings);
        let visualization = Arc::clone(&inner.visualization);
        player.append(RodioDspSource::new(decoder, dsp_settings, visualization));
        let queued_sources = player.len();
        let player_empty = player.empty();
        crate::app_log_info!(
            backend,
            "rodio appended source",
            queued_sources,
            player_empty
        );
        if start_ms > 0 {
            crate::app_log_info!(backend, "rodio start seek", start_ms);
            if let Err(err) = player.try_seek(Duration::from_millis(start_ms as u64)) {
                let message = format!("failed to seek rodio source: {err}");
                inner.state = PlayerState::Stopped;
                inner
                    .pending_events
                    .push(PlaybackEvent::Error(message.clone()));
                return Err(message);
            }
        }
        player.play();
        crate::app_log_info!(backend, "rodio playback started");

        inner.current_uri = Some(uri.to_string());
        inner.state = PlayerState::Playing;
        inner.duration_ms = duration_ms;
        inner.stream_info = stream_info;
        inner.eos_emitted = false;
        inner
            .pending_events
            .push(PlaybackEvent::DurationChanged(duration_ms));
        inner
            .pending_events
            .push(PlaybackEvent::StreamInfo(stream_info));
        Ok(())
    }

    fn pause(&self) -> Result<(), String> {
        let mut inner = self.lock_inner();
        inner.output.player().pause();
        if inner.state == PlayerState::Playing {
            inner.state = PlayerState::Paused;
        }
        Ok(())
    }

    fn unpause(&self) -> Result<(), String> {
        let mut inner = self.lock_inner();
        inner.output.player().play();
        if inner.current_uri.is_some() {
            inner.state = PlayerState::Playing;
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        let mut inner = self.lock_inner();
        inner.output.player().stop();
        inner.current_uri = None;
        inner.state = PlayerState::Stopped;
        inner.duration_ms = None;
        inner.stream_info = StreamInfo::default();
        inner.eos_emitted = false;
        Ok(())
    }

    fn seek(&self, position_ms: i64) -> Result<(), String> {
        if position_ms < 0 {
            return Err("seek position must be non-negative".to_string());
        }
        crate::app_log_info!(backend, "rodio seek", position_ms);
        self.lock_inner()
            .output
            .player()
            .try_seek(Duration::from_millis(position_ms as u64))
            .map_err(|err| format!("failed to seek rodio source: {err}"))
    }

    fn set_volume(&self, volume: i32) -> Result<(), String> {
        let mut inner = self.lock_inner();
        inner.volume = volume.clamp(0, 100);
        let rodio_volume = percent_to_rodio_volume(inner.volume);
        inner.output.player().set_volume(rodio_volume);
        crate::app_log_info!(backend, "rodio set volume", volume, rodio_volume);
        Ok(())
    }

    fn set_balance(&self, balance: i32) -> Result<(), String> {
        let mut inner = self.lock_inner();
        inner.balance = balance.clamp(-100, 100);
        inner
            .dsp_settings
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .update_balance(inner.balance);
        Ok(())
    }

    fn set_equalizer(&self, state: EqualizerBackendState) -> Result<(), String> {
        let mut inner = self.lock_inner();
        inner.equalizer = state;
        inner
            .dsp_settings
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .update(state);
        let preamp_db = if state.active {
            equalizer_position_to_db(state.preamp_position)
        } else {
            0.0
        };
        let active = state.active;
        crate::app_log_info!(backend, "rodio set equalizer", active, preamp_db);
        Ok(())
    }

    fn set_spectrum_layout(&self, layout: SpectrumLayout) {
        self.lock_inner().spectrum_layout = layout;
    }

    fn poll_events(&self) -> Result<Vec<PlaybackEvent>, String> {
        let mut inner = self.lock_inner();
        if rodio_debug_enabled()
            && inner
                .last_debug_log
                .is_none_or(|last| last.elapsed() >= Duration::from_millis(1_000))
        {
            let player = inner.output.player();
            let pos_ms = duration_to_millis(player.get_pos());
            let queued_sources = player.len();
            let player_empty = player.empty();
            let player_paused = player.is_paused();
            let state = format!("{:?}", inner.state);
            crate::app_log_info!(
                backend,
                "rodio poll",
                state,
                pos_ms,
                queued_sources,
                player_empty,
                player_paused
            );
            inner.last_debug_log = Some(Instant::now());
        }
        if inner.state == PlayerState::Playing
            && !inner.eos_emitted
            && inner.output.player().empty()
        {
            let pos_ms = duration_to_millis(inner.output.player().get_pos());
            inner.eos_emitted = true;
            inner.state = PlayerState::Stopped;
            crate::app_log_info!(backend, "rodio synthetic eos", pos_ms);
            inner.pending_events.push(PlaybackEvent::EndOfStream);
        }
        let visualization = Arc::clone(&inner.visualization);
        let visualization = visualization
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let visualization_update = (visualization.generation != inner.visualization_generation
            || inner.spectrum_layout != inner.emitted_spectrum_layout)
            .then_some((
                visualization.generation,
                visualization.frame.data(inner.spectrum_layout),
            ));
        drop(visualization);
        if let Some((generation, data)) = visualization_update {
            inner.visualization_generation = generation;
            inner.emitted_spectrum_layout = inner.spectrum_layout;
            inner.pending_events.push(PlaybackEvent::Spectrum(data));
        }
        Ok(std::mem::take(&mut inner.pending_events))
    }

    fn position_ms(&self) -> Option<i64> {
        Some(duration_to_millis(
            self.lock_inner().output.player().get_pos(),
        ))
    }

    fn duration_ms(&self) -> Option<i64> {
        self.lock_inner().duration_ms
    }

    fn stream_info(&self) -> StreamInfo {
        self.lock_inner().stream_info
    }

    fn state(&self) -> PlayerState {
        self.lock_inner().state
    }

    fn current_uri(&self) -> Option<String> {
        self.lock_inner().current_uri.clone()
    }

    fn output_device_groups(&self) -> OutputDeviceGroups {
        crate::player::group_output_devices(list_cpal_output_devices())
    }

    fn select_output_device(
        &mut self,
        selection: crate::playback::model::OutputDeviceSelection<'_>,
    ) -> Result<(), String> {
        let requested = match selection {
            crate::playback::model::OutputDeviceSelection::Automatic => None,
            crate::playback::model::OutputDeviceSelection::System(id) => Some(id),
        };
        let (sink, output_device) = open_rodio_sink(requested)?;
        let player = RodioPlayer::connect_new(sink.mixer());
        let mut inner = self.lock_inner();
        inner.output = RodioOutput::Device { sink, player };
        inner.output_device = output_device;
        inner.state = PlayerState::Stopped;
        inner.current_uri = None;
        Ok(())
    }

    fn current_output_device(&self) -> Option<OutputDevice> {
        Some(self.lock_inner().output_device.clone())
    }
}

impl AudioMetadataProbe for RodioMetadataProbe {
    fn probe(&self, item: &DurationIndexItem) -> Result<Option<DurationIndexResult>, String> {
        let source = match resolve_local_audio_source(&item.uri) {
            Ok(source) => source,
            Err(err) if is_unsupported_uri_error(&err) => return Ok(None),
            Err(err) => return Err(err),
        };
        if !source.path.exists() {
            return Ok(None);
        }
        let file = File::open(&source.path).map_err(|err| {
            format!(
                "failed to open local audio file {}: {err}",
                source.path.display()
            )
        })?;
        let decoder = Decoder::try_from(file).map_err(|err| {
            format!(
                "failed to decode local audio file {} with rodio: {err}",
                source.path.display()
            )
        })?;
        let title = {
            use id3::TagLike as _;

            id3::no_tag_ok(id3::v1v2::read_from_path(&source.path))
                .ok()
                .flatten()
                .and_then(|tag| tag.title().map(str::to_string))
        };
        Ok(Some(DurationIndexResult {
            index: item.index,
            uri: item.uri.clone(),
            length_ms: decoder
                .total_duration()
                .map(duration_to_millis)
                .unwrap_or(-1),
            title,
        }))
    }
}

impl RodioOutput {
    fn player(&self) -> &RodioPlayer {
        match self {
            RodioOutput::Device { player, .. } => player,
            #[cfg(test)]
            RodioOutput::Detached { player } => player,
        }
    }

    fn recreate_player(&mut self) {
        match self {
            RodioOutput::Device { sink, player } => {
                *player = RodioPlayer::connect_new(sink.mixer());
            }
            #[cfg(test)]
            RodioOutput::Detached { player } => {
                let (new_player, _source) = RodioPlayer::new();
                *player = new_player;
            }
        }
    }
}

pub fn resolve_local_audio_source(uri: &str) -> Result<LocalAudioSource, String> {
    let path = if let Some(rest) = uri.strip_prefix("file://") {
        file_uri_path(rest)?
    } else if uri.contains("://") {
        let scheme = uri
            .split_once("://")
            .map(|(scheme, _)| scheme)
            .unwrap_or(uri);
        return Err(format!(
            "URI scheme '{scheme}' is not supported by the platform-independent rodio backend"
        ));
    } else {
        PathBuf::from(uri)
    };

    if path.as_os_str().is_empty() {
        return Err("local audio path is empty".to_string());
    }
    Ok(LocalAudioSource { path })
}

fn file_uri_path(rest: &str) -> Result<PathBuf, String> {
    let path_part = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path_part.starts_with('/') {
        return Err(format!(
            "file URI must contain an absolute path for rodio playback: file://{rest}"
        ));
    }
    Ok(PathBuf::from(percent_decode(path_part)?))
}

fn percent_decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(format!("invalid percent escape in file URI path: {value}"));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|_| format!("invalid percent escape in file URI path: {value}"))?;
            let decoded = u8::from_str_radix(hex, 16)
                .map_err(|_| format!("invalid percent escape in file URI path: {value}"))?;
            output.push(decoded);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| format!("file URI path is not valid UTF-8: {value}"))
}

fn percent_to_rodio_volume(percent: i32) -> f32 {
    percent.clamp(0, 100) as f32 / 100.0
}

fn duration_to_millis(duration: Duration) -> i64 {
    duration.as_millis().min(i64::MAX as u128) as i64
}

fn estimate_bitrate_kbps(path: &PathBuf, duration_ms: Option<i64>) -> Option<i32> {
    let duration_ms = duration_ms.filter(|duration| *duration > 0)? as u128;
    let bytes = std::fs::metadata(path).ok()?.len() as u128;
    let kbps = ((bytes * 8) + (duration_ms / 2)) / duration_ms;
    i32::try_from(kbps).ok().filter(|value| *value > 0)
}

fn db_to_gain(db: f64) -> f32 {
    10.0_f32.powf(db as f32 / 20.0)
}

fn open_rodio_sink(device_id: Option<&str>) -> Result<(MixerDeviceSink, OutputDevice), String> {
    let cpal_device_id = configure_alsa_backend_for_selection(device_id)?;
    let host = cpal::default_host();
    let device = match cpal_device_id {
        Some(id) => host
            .output_devices()
            .map_err(|err| format!("failed to enumerate rodio/cpal output devices: {err}"))?
            .find(|device| device_name(device).as_deref() == Some(id))
            .ok_or_else(|| format!("rodio/cpal output device not found: {id}"))?,
        None => host
            .default_output_device()
            .ok_or_else(|| "rodio/cpal default output device not found".to_string())?,
    };
    #[cfg(target_os = "android")]
    let name = "Android audio output".to_string();
    #[cfg(not(target_os = "android"))]
    let name = device_name(&device).unwrap_or_else(|| device_id.unwrap_or("unknown").to_string());
    let output_device = device_id
        .and_then(rodio_alsa_backend_device)
        .unwrap_or_else(|| OutputDevice::system(&name, &name, "cpal", false));
    let selected_name = output_device.display_name.clone();
    let requested = device_id.unwrap_or("default").to_string();
    crate::app_log_info!(
        backend,
        "rodio opening output device",
        requested,
        selected_name
    );
    if rodio_debug_enabled() {
        log_supported_output_configs(&device, &selected_name);
    }

    #[cfg(target_os = "android")]
    let mut builder = DeviceSinkBuilder::default().with_device(device);
    #[cfg(not(target_os = "android"))]
    let mut builder = DeviceSinkBuilder::from_device(device)
        .map_err(|err| format!("failed to configure rodio audio output {selected_name}: {err}"))?;
    if let Some(sample_format) = requested_sample_format()? {
        crate::app_log_info!(backend, "rodio forcing sample format {sample_format:?}");
        builder = builder.with_sample_format(sample_format);
    }
    #[cfg(target_os = "android")]
    let mut sink = builder
        .open_stream()
        .map_err(|err| format!("failed to open rodio audio output {selected_name}: {err}"))?;
    #[cfg(not(target_os = "android"))]
    let mut sink = builder
        .open_sink_or_fallback()
        .map_err(|err| format!("failed to open rodio audio output {selected_name}: {err}"))?;
    sink.log_on_drop(false);
    let config = format!("{:?}", sink.config());
    crate::app_log_info!(backend, "rodio opened output", selected_name, config);
    Ok((sink, output_device))
}

fn list_cpal_output_devices() -> Vec<OutputDevice> {
    let mut devices = vec![
        OutputDevice::system(
            RODIO_ALSA_PIPEWIRE_ID,
            "PipeWire (via ALSA plugin)",
            "cpal",
            false,
        ),
        OutputDevice::system(
            RODIO_ALSA_PULSE_ID,
            "PulseAudio (via ALSA plugin)",
            "cpal",
            false,
        ),
        OutputDevice::system(RODIO_ALSA_SYSTEM_ID, "System ALSA default", "cpal", false),
    ];
    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|device| device_name(&device));
    if let Some(name) = default_name.clone() {
        devices.push(OutputDevice::system(
            &name,
            &format!("{name} (default)"),
            "cpal",
            false,
        ));
    }
    match host.output_devices() {
        Ok(output_devices) => {
            for device in output_devices {
                if let Some(name) = device_name(&device) {
                    if Some(name.as_str()) != default_name.as_deref() {
                        devices.push(OutputDevice::system(&name, &name, "cpal", false));
                    }
                }
            }
        }
        Err(err) => eprintln!("xmms-rs: failed to enumerate rodio/cpal output devices: {err}"),
    }
    if devices.is_empty() {
        devices.push(default_output_device());
    }
    devices
}

fn configure_alsa_backend_for_selection<'a>(
    device_id: Option<&'a str>,
) -> Result<Option<&'a str>, String> {
    match device_id {
        None => {
            configure_alsa_virtual_backend(auto_alsa_backend());
            Ok(None)
        }
        Some(RODIO_ALSA_PIPEWIRE_ID) => {
            configure_alsa_virtual_backend(Some("pipewire"));
            Ok(None)
        }
        Some(RODIO_ALSA_PULSE_ID) => {
            configure_alsa_virtual_backend(Some("pulse"));
            Ok(None)
        }
        Some(RODIO_ALSA_SYSTEM_ID) => {
            configure_alsa_virtual_backend(None);
            Ok(None)
        }
        Some(other) => Ok(Some(other)),
    }
}

fn auto_alsa_backend() -> Option<&'static str> {
    ["pipewire", "pulse"]
        .into_iter()
        .find(|backend| alsa_pcm_plugin_exists(backend))
}

fn configure_alsa_virtual_backend(backend_name: Option<&str>) {
    match backend_name {
        Some(backend_name) => match write_alsa_backend_config(backend_name) {
            Ok(path) => {
                std::env::set_var("ALSA_CONFIG_PATH", &path);
                let config_path = path.display().to_string();
                crate::app_log_info!(
                    backend,
                    "rodio using ALSA plugin",
                    backend_name,
                    config_path
                );
            }
            Err(err) => eprintln!("xmms-rs: failed to write rodio ALSA config: {err}"),
        },
        None => {
            std::env::remove_var("ALSA_CONFIG_PATH");
            crate::app_log_info!(backend, "rodio using system ALSA default");
        }
    }
}

fn write_alsa_backend_config(backend: &str) -> Result<PathBuf, String> {
    let dir = std::env::current_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("target");
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    let path = dir.join(format!("rodio-{backend}.asoundrc"));
    std::fs::write(
        &path,
        format!(
            "</usr/share/alsa/alsa.conf>\n\npcm.!default {{\n    type {backend}\n}}\nctl.!default {{\n    type {backend}\n}}\n"
        ),
    )
    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
}

fn alsa_pcm_plugin_exists(name: &str) -> bool {
    [
        format!("/usr/lib/alsa-lib/libasound_module_pcm_{name}.so"),
        format!("/usr/lib64/alsa-lib/libasound_module_pcm_{name}.so"),
        format!("/usr/lib/x86_64-linux-gnu/alsa-lib/libasound_module_pcm_{name}.so"),
        format!("/usr/lib/aarch64-linux-gnu/alsa-lib/libasound_module_pcm_{name}.so"),
    ]
    .into_iter()
    .any(|path| std::path::Path::new(&path).exists())
}

fn rodio_alsa_backend_device(id: &str) -> Option<OutputDevice> {
    match id {
        RODIO_ALSA_PIPEWIRE_ID => Some(OutputDevice::system(
            RODIO_ALSA_PIPEWIRE_ID,
            "PipeWire (via ALSA plugin)",
            "cpal",
            false,
        )),
        RODIO_ALSA_PULSE_ID => Some(OutputDevice::system(
            RODIO_ALSA_PULSE_ID,
            "PulseAudio (via ALSA plugin)",
            "cpal",
            false,
        )),
        RODIO_ALSA_SYSTEM_ID => Some(OutputDevice::system(
            RODIO_ALSA_SYSTEM_ID,
            "System ALSA default",
            "cpal",
            false,
        )),
        _ => None,
    }
}

fn rodio_debug_enabled() -> bool {
    std::env::var("XMMS_RODIO_DEBUG").is_ok_and(|value| value != "0")
}

fn requested_sample_format() -> Result<Option<cpal::SampleFormat>, String> {
    let Ok(value) = std::env::var("XMMS_RODIO_SAMPLE_FORMAT") else {
        return Ok(None);
    };
    match value.to_ascii_lowercase().as_str() {
        "" => Ok(None),
        "i8" => Ok(Some(cpal::SampleFormat::I8)),
        "i16" => Ok(Some(cpal::SampleFormat::I16)),
        "i32" => Ok(Some(cpal::SampleFormat::I32)),
        "i64" => Ok(Some(cpal::SampleFormat::I64)),
        "u8" => Ok(Some(cpal::SampleFormat::U8)),
        "u16" => Ok(Some(cpal::SampleFormat::U16)),
        "u32" => Ok(Some(cpal::SampleFormat::U32)),
        "u64" => Ok(Some(cpal::SampleFormat::U64)),
        "f32" => Ok(Some(cpal::SampleFormat::F32)),
        "f64" => Ok(Some(cpal::SampleFormat::F64)),
        other => Err(format!(
            "unsupported XMMS_RODIO_SAMPLE_FORMAT '{other}', expected i16, f32, etc."
        )),
    }
}

fn log_supported_output_configs(device: &cpal::Device, selected_name: &str) {
    match device.supported_output_configs() {
        Ok(configs) => {
            for config in configs.take(24) {
                let channels = config.channels();
                let min_rate = config.min_sample_rate();
                let max_rate = config.max_sample_rate();
                let sample_format = format!("{:?}", config.sample_format());
                crate::app_log_info!(
                    backend,
                    "rodio supported output config",
                    selected_name,
                    channels,
                    min_rate,
                    max_rate,
                    sample_format
                );
            }
        }
        Err(err) => eprintln!(
            "xmms-rs: failed to list rodio/cpal supported output configs for {selected_name}: {err}"
        ),
    }
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

fn default_output_device() -> OutputDevice {
    OutputDevice::system("rodio-default", "Default audio output", "cpal", false)
}

fn is_unsupported_uri_error(err: &str) -> bool {
    err.contains("is not supported by the platform-independent rodio backend")
}

#[cfg(test)]
fn test_wav_bytes(sample_rate: u32, channels: u16, frames: u32) -> Vec<u8> {
    let bits_per_sample = 16u16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * u32::from(block_align);
    let data_size = frames * u32::from(block_align);
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.resize(44 + data_size as usize, 0);
    wav
}

#[cfg(test)]
fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "xmms-renascene-rodio-test-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[cfg(test)]
fn path_to_file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::backend::PlaybackBackend;

    fn write_test_wav(path: &Path, sample_rate: u32, channels: u16, frames: u32) {
        std::fs::write(path, test_wav_bytes(sample_rate, channels, frames)).unwrap();
    }

    #[test]
    fn rodio_backend_implements_playback_backend_trait() {
        fn assert_backend_trait<T: PlaybackBackend>() {}
        assert_backend_trait::<RodioBackend>();
    }

    fn exact_fft_bin_tone(bin: usize) -> Vec<f32> {
        (0..SPECTRUM_WINDOW_FRAMES)
            .map(|frame| {
                (std::f32::consts::TAU * bin as f32 * frame as f32 / SPECTRUM_WINDOW_FRAMES as f32)
                    .sin()
                    * 0.8
            })
            .collect()
    }

    fn strongest_level(levels: &[f32]) -> usize {
        levels
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
            .map(|(index, _)| index)
            .unwrap()
    }

    #[test]
    fn rodio_spectrum_keeps_first_three_bars_independent() {
        for expected_bar in 0..3 {
            let frame = analyze_spectrum_window(44_100.0, &exact_fft_bin_tone(expected_bar + 1));
            assert_eq!(strongest_level(&frame.bars), expected_bar);
            assert!(frame.bars[expected_bar] > 0.8);
        }
    }

    #[test]
    fn rodio_spectrum_places_one_khz_in_bar_nine() {
        let sample_rate = 44_100.0;
        let tone_frequency = 1_000.0;
        let samples = (0..SPECTRUM_WINDOW_FRAMES)
            .map(|frame| {
                let phase =
                    2.0 * std::f32::consts::PI * tone_frequency * frame as f32 / sample_rate;
                phase.sin() * 0.8
            })
            .collect::<Vec<_>>();
        let frame = analyze_spectrum_window(sample_rate, &samples);
        assert_eq!(strongest_level(&frame.bars), 9);
        assert!(frame.bars[9] > 0.8);
    }

    #[test]
    fn rodio_spectrum_places_high_tone_in_right_side_bar() {
        let frame = analyze_spectrum_window(44_100.0, &exact_fft_bin_tone(100));
        assert_eq!(strongest_level(&frame.bars), 16);
        assert!(frame.bars[16] > 0.8);
    }

    #[test]
    fn rodio_spectrum_layout_change_emits_existing_frame_in_new_layout() {
        let backend = RodioBackend::new_detached_for_tests();
        let mut lines = [0.0; SPECTRUM_BANDS];
        lines[30] = 0.4;
        let mut bars = [0.0; ANALYZER_BAR_COUNT];
        bars[2] = 0.9;
        let visualization = Arc::clone(&backend.lock_inner().visualization);
        visualization
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .publish(RodioSpectrumFrame { lines, bars });

        backend.set_spectrum_layout(SpectrumLayout::Lines);
        let line_events = backend.poll_events().unwrap();
        assert!(line_events
            .iter()
            .any(|event| matches!(event, PlaybackEvent::Spectrum(data) if data[30] == 0.4)));

        backend.set_spectrum_layout(SpectrumLayout::AnalyzerBars);
        let bar_events = backend.poll_events().unwrap();
        assert!(bar_events.iter().any(
            |event| matches!(event, PlaybackEvent::Spectrum(data) if data[2] == 0.9 && data[30] == 0.0)
        ));
    }

    #[test]
    fn rodio_spectrum_capture_drops_a_window_when_worker_queue_is_full() {
        let (sender, _receiver) = mpsc::sync_channel(0);
        let (_recycle_sender, recycled) = mpsc::sync_channel(1);
        let mut capture = RodioSpectrumCapture {
            sender,
            recycled,
            current_generation: Arc::new(AtomicU64::new(0)),
            samples: Vec::with_capacity(SPECTRUM_WINDOW_FRAMES),
        };

        for _ in 0..SPECTRUM_WINDOW_FRAMES {
            capture.push_sample(0.25);
        }

        assert!(capture.samples.is_empty());
        assert!(capture.samples.capacity() >= SPECTRUM_WINDOW_FRAMES);
    }

    #[test]
    fn local_uri_resolver_accepts_paths_and_file_uris() {
        let plain = resolve_local_audio_source("/tmp/example.wav").unwrap();
        assert_eq!(plain.path, PathBuf::from("/tmp/example.wav"));

        let file = resolve_local_audio_source("file:///tmp/example%20song.wav").unwrap();
        assert_eq!(file.path, PathBuf::from("/tmp/example song.wav"));

        let localhost = resolve_local_audio_source("file://localhost/tmp/example.wav").unwrap();
        assert_eq!(localhost.path, PathBuf::from("/tmp/example.wav"));
    }

    #[test]
    fn local_uri_resolver_rejects_unsupported_schemes() {
        let err = resolve_local_audio_source("content://media/external/audio/1")
            .expect_err("content URI should not be supported in step 1");
        assert!(err.contains("content"));
        assert!(err.contains("not supported"));

        let err = resolve_local_audio_source("https://example.invalid/song.ogg")
            .expect_err("streaming URL should not be supported in step 1");
        assert!(err.contains("https"));
        assert!(err.contains("not supported"));
    }

    #[test]
    fn rodio_metadata_probe_reports_playlist_audio_length() {
        let dir = unique_temp_dir();
        let wav = dir.join("one-second.wav");
        write_test_wav(&wav, 8_000, 2, 8_000);
        let uri = path_to_file_uri(&wav);

        let result = RodioMetadataProbe
            .probe(&DurationIndexItem {
                index: 4,
                uri: uri.clone(),
            })
            .unwrap()
            .unwrap();

        assert_eq!(result.index, 4);
        assert_eq!(result.uri, uri);
        assert_eq!(result.length_ms, 1_000);
        assert_eq!(result.title, None);
    }

    #[test]
    fn rodio_metadata_probe_ignores_step_one_unsupported_uris() {
        let result = RodioMetadataProbe
            .probe(&DurationIndexItem {
                index: 0,
                uri: "content://media/external/audio/1".to_string(),
            })
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn rodio_backend_decodes_local_file_and_queues_metadata_events() {
        let dir = unique_temp_dir();
        let wav = dir.join("tone.wav");
        write_test_wav(&wav, 8_000, 1, 4_000);
        let uri = path_to_file_uri(&wav);
        let backend = RodioBackend::new_detached_for_tests();

        backend.play_uri(&uri).unwrap();
        let events = backend.poll_events().unwrap();

        assert_eq!(backend.current_uri(), Some(uri));
        assert_eq!(backend.state(), PlayerState::Playing);
        assert_eq!(backend.duration_ms(), Some(500));
        assert_eq!(backend.stream_info().bitrate, Some(129));
        assert_eq!(backend.stream_info().frequency, Some(8_000));
        assert_eq!(backend.stream_info().channels, Some(1));
        assert!(events.contains(&PlaybackEvent::DurationChanged(Some(500))));
        assert!(events.contains(&PlaybackEvent::StreamInfo(StreamInfo {
            bitrate: Some(129),
            frequency: Some(8_000),
            channels: Some(1),
        })));
    }

    #[test]
    fn rodio_backend_emits_synthetic_eos_once() {
        let backend = RodioBackend::new_detached_for_tests();
        backend.lock_inner().state = PlayerState::Playing;
        backend.force_player_empty_for_tests();

        assert_eq!(
            backend.poll_events().unwrap(),
            vec![PlaybackEvent::EndOfStream]
        );
        assert_eq!(backend.poll_events().unwrap(), Vec::new());
        assert_eq!(backend.state(), PlayerState::Stopped);
    }

    #[test]
    fn rodio_backend_queues_error_event_for_play_failures() {
        let backend = RodioBackend::new_detached_for_tests();
        let err = backend
            .play_uri("content://media/external/audio/1")
            .unwrap_err();

        assert!(err.contains("not supported"));
        assert_eq!(
            backend.poll_events().unwrap(),
            vec![PlaybackEvent::Error(err)]
        );
        assert_eq!(backend.state(), PlayerState::Stopped);
    }

    #[test]
    fn rodio_backend_stops_existing_track_when_next_uri_fails() {
        let backend = RodioBackend::new_detached_for_tests();
        {
            let mut inner = backend.lock_inner();
            inner.current_uri = Some("file:///tmp/playing.mp3".to_string());
            inner.state = PlayerState::Playing;
            inner.duration_ms = Some(10_000);
            inner.stream_info = StreamInfo {
                bitrate: None,
                frequency: Some(44_100),
                channels: Some(2),
            };
        }

        let err = backend
            .play_uri("content://media/external/audio/1")
            .unwrap_err();

        assert!(err.contains("not supported"));
        assert_eq!(backend.current_uri(), None);
        assert_eq!(backend.state(), PlayerState::Stopped);
        assert_eq!(backend.duration_ms(), None);
        assert_eq!(backend.stream_info(), StreamInfo::default());
        assert_eq!(
            backend.poll_events().unwrap(),
            vec![PlaybackEvent::Error(err)]
        );
    }

    #[test]
    fn rodio_backend_stores_and_shares_equalizer_controls() {
        let backend = RodioBackend::new_detached_for_tests();
        backend.set_volume(150).unwrap();
        backend.set_balance(-150).unwrap();
        let equalizer = EqualizerBackendState {
            active: true,
            preamp_position: 42,
            band_positions: [7; EQUALIZER_BANDS],
        };
        backend.set_equalizer(equalizer).unwrap();

        assert_eq!(backend.volume(), 100);
        assert_eq!(backend.balance(), -100);
        assert_eq!(backend.equalizer(), equalizer);
        let settings = backend.lock_inner().dsp_settings.clone();
        let settings = settings.lock().unwrap();
        assert_eq!(settings.balance, -100);
        assert!(settings.active);
        assert_eq!(settings.preamp_position, 42);
        assert_eq!(settings.band_positions, [7; EQUALIZER_BANDS]);
    }

    #[test]
    fn rodio_dsp_source_applies_preamp_gain() {
        let state = EqualizerBackendState {
            active: true,
            preamp_position: 25,
            band_positions: [50; EQUALIZER_BANDS],
        };
        let settings = Arc::new(Mutex::new(RodioDspSettings::new(state, 0)));
        let source = rodio::buffer::SamplesBuffer::new(
            std::num::NonZero::new(1).unwrap(),
            std::num::NonZero::new(44_100).unwrap(),
            vec![0.1],
        );
        let visualization = Arc::new(Mutex::new(RodioVisualization::new()));
        let mut source = RodioDspSource::new(source, settings, visualization);

        let sample = source.next().unwrap();

        assert!(
            sample > 0.25,
            "expected +10 dB preamp to boost sample, got {sample}"
        );
    }

    #[test]
    fn rodio_dsp_source_applies_stereo_balance() {
        let settings = Arc::new(Mutex::new(RodioDspSettings::new(
            EqualizerBackendState {
                active: false,
                preamp_position: 50,
                band_positions: [50; EQUALIZER_BANDS],
            },
            -100,
        )));
        let source = rodio::buffer::SamplesBuffer::new(
            std::num::NonZero::new(2).unwrap(),
            std::num::NonZero::new(44_100).unwrap(),
            vec![0.5, 0.5],
        );
        let visualization = Arc::new(Mutex::new(RodioVisualization::new()));
        let samples = RodioDspSource::new(source, settings, visualization).collect::<Vec<_>>();

        assert!((samples[0] - 0.5).abs() < 0.001);
        assert!(samples[1].abs() < 0.001);
    }

    #[test]
    fn rodio_dsp_source_applies_equalizer_bands() {
        let mut bands = [50; EQUALIZER_BANDS];
        bands[4] = 20; // boost the 1 kHz band.
        let boosted = EqualizerBackendState {
            active: true,
            preamp_position: 50,
            band_positions: bands,
        };
        let flat = EqualizerBackendState {
            active: false,
            preamp_position: 50,
            band_positions: [50; EQUALIZER_BANDS],
        };
        let sine = (0..1_000)
            .map(|n| {
                let phase = 2.0 * std::f32::consts::PI * 1_000.0 * n as f32 / 44_100.0;
                0.1 * phase.sin()
            })
            .collect::<Vec<_>>();
        let make_source = |state| {
            let settings = Arc::new(Mutex::new(RodioDspSettings::new(state, 0)));
            let source = rodio::buffer::SamplesBuffer::new(
                std::num::NonZero::new(1).unwrap(),
                std::num::NonZero::new(44_100).unwrap(),
                sine.clone(),
            );
            let visualization = Arc::new(Mutex::new(RodioVisualization::new()));
            RodioDspSource::new(source, settings, visualization)
        };

        let boosted_sum: f32 = make_source(boosted).map(f32::abs).sum();
        let flat_sum: f32 = make_source(flat).map(f32::abs).sum();

        assert!(
            boosted_sum > flat_sum * 1.2,
            "expected 1 kHz EQ boost to increase amplitude, flat={flat_sum}, boosted={boosted_sum}"
        );
    }
}
