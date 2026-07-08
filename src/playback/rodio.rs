//! Rodio playback backend.
//!
//! This backend intentionally only supports platform-independent local playback
//! in the first Android-audio step. Android `content://` and streaming URL
//! support are handled by a later platform URI resolver.

use std::cell::RefCell;
use std::fs::File;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

use ::rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer, Source};

use crate::audio_model::EQUALIZER_BANDS;
use crate::playback::backend::{AudioMetadataProbe, PlaybackBackend};
use crate::playback::model::{
    EqualizerBackendState, OutputDevice, OutputDeviceGroups, PlaybackEvent, PlayerState,
    StreamInfo,
};
use crate::playlist::{DurationIndexItem, DurationIndexResult};

pub struct RodioBackend {
    inner: RefCell<RodioBackendInner>,
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

pub struct RodioMetadataProbe;

impl RodioBackend {
    pub fn new() -> Result<Self, String> {
        let mut sink = DeviceSinkBuilder::open_default_sink()
            .map_err(|err| format!("failed to open default rodio audio output: {err}"))?;
        sink.log_on_drop(false);
        let player = RodioPlayer::connect_new(sink.mixer());
        Ok(Self::from_output(RodioOutput::Device { sink, player }))
    }

    fn from_output(output: RodioOutput) -> Self {
        Self {
            inner: RefCell::new(RodioBackendInner {
                output,
                current_uri: None,
                state: PlayerState::Stopped,
                duration_ms: None,
                stream_info: StreamInfo::default(),
                pending_events: Vec::new(),
                eos_emitted: false,
                volume: 100,
                balance: 0,
                equalizer: EqualizerBackendState {
                    active: false,
                    preamp_position: 0,
                    band_positions: [0; EQUALIZER_BANDS],
                },
            }),
        }
    }

    #[cfg(test)]
    fn new_detached_for_tests() -> Self {
        let (player, _source) = RodioPlayer::new();
        Self::from_output(RodioOutput::Detached { player })
    }

    #[cfg(test)]
    fn force_player_empty_for_tests(&self) {
        let inner = self.inner.borrow();
        inner.output.player().stop();
    }

    pub fn volume(&self) -> i32 {
        self.inner.borrow().volume
    }

    pub fn balance(&self) -> i32 {
        self.inner.borrow().balance
    }

    pub fn equalizer(&self) -> EqualizerBackendState {
        self.inner.borrow().equalizer
    }
}

impl PlaybackBackend for RodioBackend {
    fn play_uri(&self, uri: &str) -> Result<(), String> {
        self.play_uri_at(uri, 0)
    }

    fn play_uri_at(&self, uri: &str, start_ms: i64) -> Result<(), String> {
        if start_ms < 0 {
            return Err("seek position must be non-negative".to_string());
        }

        let source = resolve_local_audio_source(uri)?;
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

        let duration_ms = decoder.total_duration().map(duration_to_millis);
        let stream_info = StreamInfo {
            bitrate: None,
            frequency: Some(decoder.sample_rate().get() as i32),
            channels: Some(decoder.channels().get() as i32),
        };

        let mut inner = self.inner.borrow_mut();
        inner.output.recreate_player();
        let player = inner.output.player();
        player.set_volume(percent_to_rodio_volume(inner.volume));
        player.append(decoder);
        if start_ms > 0 {
            player
                .try_seek(Duration::from_millis(start_ms as u64))
                .map_err(|err| format!("failed to seek rodio source: {err}"))?;
        }
        player.play();

        inner.current_uri = Some(uri.to_string());
        inner.state = PlayerState::Playing;
        inner.duration_ms = duration_ms;
        inner.stream_info = stream_info;
        inner.eos_emitted = false;
        inner
            .pending_events
            .push(PlaybackEvent::DurationChanged(duration_ms));
        inner.pending_events.push(PlaybackEvent::StreamInfo(stream_info));
        Ok(())
    }

    fn pause(&self) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
        inner.output.player().pause();
        if inner.state == PlayerState::Playing {
            inner.state = PlayerState::Paused;
        }
        Ok(())
    }

    fn unpause(&self) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
        inner.output.player().play();
        if inner.current_uri.is_some() {
            inner.state = PlayerState::Playing;
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
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
        self.inner
            .borrow()
            .output
            .player()
            .try_seek(Duration::from_millis(position_ms as u64))
            .map_err(|err| format!("failed to seek rodio source: {err}"))
    }

    fn set_volume(&self, volume: i32) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
        inner.volume = volume.clamp(0, 100);
        inner
            .output
            .player()
            .set_volume(percent_to_rodio_volume(inner.volume));
        Ok(())
    }

    fn set_balance(&self, balance: i32) -> Result<(), String> {
        self.inner.borrow_mut().balance = balance.clamp(-100, 100);
        Ok(())
    }

    fn set_equalizer(&self, state: EqualizerBackendState) -> Result<(), String> {
        self.inner.borrow_mut().equalizer = state;
        Ok(())
    }

    fn poll_events(&self) -> Result<Vec<PlaybackEvent>, String> {
        let mut inner = self.inner.borrow_mut();
        if inner.state == PlayerState::Playing && !inner.eos_emitted && inner.output.player().empty()
        {
            inner.eos_emitted = true;
            inner.state = PlayerState::Stopped;
            inner.pending_events.push(PlaybackEvent::EndOfStream);
        }
        Ok(std::mem::take(&mut inner.pending_events))
    }

    fn position_ms(&self) -> Option<i64> {
        Some(duration_to_millis(
            self.inner.borrow().output.player().get_pos(),
        ))
    }

    fn duration_ms(&self) -> Option<i64> {
        self.inner.borrow().duration_ms
    }

    fn stream_info(&self) -> StreamInfo {
        self.inner.borrow().stream_info
    }

    fn state(&self) -> PlayerState {
        self.inner.borrow().state
    }

    fn current_uri(&self) -> Option<String> {
        self.inner.borrow().current_uri.clone()
    }

    fn output_device_groups(&self) -> OutputDeviceGroups {
        OutputDeviceGroups {
            local: vec![default_output_device()],
            network: Vec::new(),
        }
    }

    fn current_output_device(&self) -> Option<OutputDevice> {
        Some(default_output_device())
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
        Ok(Some(DurationIndexResult {
            index: item.index,
            uri: item.uri.clone(),
            length_ms: decoder.total_duration().map(duration_to_millis).unwrap_or(-1),
            title: None,
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
        let scheme = uri.split_once("://").map(|(scheme, _)| scheme).unwrap_or(uri);
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

fn default_output_device() -> OutputDevice {
    OutputDevice::system("rodio-default", "Default audio output", "rodio", false)
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
        assert_eq!(backend.stream_info().frequency, Some(8_000));
        assert_eq!(backend.stream_info().channels, Some(1));
        assert!(events.contains(&PlaybackEvent::DurationChanged(Some(500))));
        assert!(events.contains(&PlaybackEvent::StreamInfo(StreamInfo {
            bitrate: None,
            frequency: Some(8_000),
            channels: Some(1),
        })));
    }

    #[test]
    fn rodio_backend_emits_synthetic_eos_once() {
        let backend = RodioBackend::new_detached_for_tests();
        backend.inner.borrow_mut().state = PlayerState::Playing;
        backend.force_player_empty_for_tests();

        assert_eq!(backend.poll_events().unwrap(), vec![PlaybackEvent::EndOfStream]);
        assert_eq!(backend.poll_events().unwrap(), Vec::new());
        assert_eq!(backend.state(), PlayerState::Stopped);
    }

    #[test]
    fn rodio_backend_stores_first_step_noop_controls() {
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
    }
}
