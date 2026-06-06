use gst::prelude::*;
use gstreamer as gst;
use std::cell::{Cell, RefCell};

const SPECTRUM_BANDS: usize = 75;
const EQUALIZER_BANDS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    state: PlayerState,
    duration_ms: Option<i64>,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    volume: i32,
    balance: i32,
    vis_data: [f32; 75],
    vis_data_valid: bool,
}

pub struct GStreamerBackend {
    pipeline: gst::Element,
    audio_sink_bin: gst::Bin,
    audio_chain: Vec<String>,
    panorama: gst::Element,
    equalizer: gst::Element,
    requested_state: Cell<PlayerState>,
    requested_uri: RefCell<Option<String>>,
}

struct AudioSinkBin {
    bin: gst::Bin,
    chain: Vec<String>,
    panorama: gst::Element,
    equalizer: gst::Element,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub audio_codec: Option<String>,
    pub bitrate: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamInfo {
    pub bitrate: Option<i32>,
    pub frequency: Option<i32>,
    pub channels: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackEvent {
    EndOfStream,
    Error(String),
    DurationChanged(Option<i64>),
    Tags(PlaybackTags),
    StreamInfo(StreamInfo),
    Spectrum([f32; SPECTRUM_BANDS]),
}

impl GStreamerBackend {
    pub fn new() -> Result<Self, String> {
        gst::init().map_err(|err| format!("failed to initialize GStreamer: {err}"))?;

        let pipeline = make_element("playbin", "player")?;
        let fake_video = make_element("fakesink", "fakevideo")?;
        pipeline.set_property("video-sink", &fake_video);

        let audio_sink = build_audio_sink_bin("autoaudiosink", None)?;
        pipeline.set_property("audio-sink", &audio_sink.bin);

        Ok(Self {
            pipeline,
            audio_sink_bin: audio_sink.bin,
            audio_chain: audio_sink.chain,
            panorama: audio_sink.panorama,
            equalizer: audio_sink.equalizer,
            requested_state: Cell::new(PlayerState::Stopped),
            requested_uri: RefCell::new(None),
        })
    }

    pub fn pipeline_factory_name(&self) -> Option<String> {
        self.pipeline.factory().map(|factory| factory.name().into())
    }

    pub fn video_sink_factory_name(&self) -> Option<String> {
        self.pipeline
            .property::<gst::Element>("video-sink")
            .factory()
            .map(|factory| factory.name().into())
    }

    pub fn audio_chain(&self) -> &[String] {
        &self.audio_chain
    }

    pub fn audio_sink_bin_name(&self) -> String {
        self.audio_sink_bin.name().into()
    }

    pub fn poll_bus_events(&self) -> Result<Vec<PlaybackEvent>, String> {
        let bus = self
            .pipeline
            .bus()
            .ok_or_else(|| "GStreamer playbin has no bus".to_string())?;
        let mut events = Vec::new();
        while let Some(message) = bus.pop() {
            if let Some(event) = self.event_from_message(&message) {
                events.push(event);
            }
        }
        Ok(events)
    }

    pub fn play_uri(&self, uri: &str) -> Result<(), String> {
        self.pipeline.set_property("uri", Some(uri));
        self.requested_uri.replace(Some(uri.to_string()));
        self.set_state(gst::State::Playing)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.set_state(gst::State::Null)
    }

    pub fn pause(&self) -> Result<(), String> {
        self.set_state(gst::State::Paused)
    }

    pub fn unpause(&self) -> Result<(), String> {
        self.set_state(gst::State::Playing)
    }

    pub fn toggle_pause(&self) -> Result<PlayerState, String> {
        match self.playback_state() {
            PlayerState::Playing => {
                self.pause()?;
                Ok(PlayerState::Paused)
            }
            PlayerState::Paused | PlayerState::Stopped => {
                self.unpause()?;
                Ok(PlayerState::Playing)
            }
        }
    }

    pub fn seek_to_ms(&self, milliseconds: i64) -> Result<(), String> {
        if milliseconds < 0 {
            return Err("seek position must be non-negative".to_string());
        }

        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(milliseconds as u64),
            )
            .map_err(|err| format!("failed to seek GStreamer pipeline: {err}"))
    }

    pub fn position_ms(&self) -> Option<i64> {
        self.pipeline
            .query_position::<gst::ClockTime>()
            .map(|position| position.mseconds() as i64)
    }

    pub fn duration_ms(&self) -> Option<i64> {
        query_duration_ms(&self.pipeline)
    }

    pub fn audio_stream_info(&self) -> StreamInfo {
        self.audio_sink_bin
            .static_pad("sink")
            .and_then(|pad| pad.current_caps())
            .as_ref()
            .map(stream_info_from_caps)
            .unwrap_or_default()
    }

    pub fn playback_state(&self) -> PlayerState {
        self.requested_state.get()
    }

    pub fn uri(&self) -> Option<String> {
        self.requested_uri.borrow().clone()
    }

    pub fn set_volume_percent(&self, percent: i32) {
        self.pipeline
            .set_property("volume", (percent.clamp(0, 100) as f64) / 100.0);
    }

    pub fn volume_percent(&self) -> i32 {
        (self.pipeline.property::<f64>("volume") * 100.0).round() as i32
    }

    pub fn set_balance_percent(&self, percent: i32) {
        self.panorama
            .set_property("panorama", (percent.clamp(-100, 100) as f32) / 100.0);
    }

    pub fn balance_percent(&self) -> i32 {
        (self.panorama.property::<f32>("panorama") * 100.0).round() as i32
    }

    pub fn set_equalizer_band_db(&self, band: usize, db: f64) -> Result<(), String> {
        let property = equalizer_band_property(band)?;
        self.equalizer.set_property(property, db.clamp(-24.0, 12.0));
        Ok(())
    }

    pub fn equalizer_band_db(&self, band: usize) -> Result<f64, String> {
        let property = equalizer_band_property(band)?;
        Ok(self.equalizer.property::<f64>(property))
    }

    pub fn set_equalizer_bands_db(&self, bands: [f64; EQUALIZER_BANDS]) {
        for (index, db) in bands.into_iter().enumerate() {
            let _ = self.set_equalizer_band_db(index, db);
        }
    }

    pub fn rebuild_output_sink(
        &mut self,
        sink_factory: &str,
        device: Option<&str>,
    ) -> Result<(), String> {
        let audio_sink = build_audio_sink_bin(sink_factory, device)?;
        self.pipeline.set_property("audio-sink", &audio_sink.bin);
        self.audio_sink_bin = audio_sink.bin;
        self.audio_chain = audio_sink.chain;
        self.panorama = audio_sink.panorama;
        self.equalizer = audio_sink.equalizer;
        Ok(())
    }

    fn event_from_message(&self, message: &gst::Message) -> Option<PlaybackEvent> {
        event_from_message(message, || query_duration_ms(&self.pipeline))
    }

    fn set_state(&self, state: gst::State) -> Result<(), String> {
        self.pipeline
            .set_state(state)
            .map(|_| {
                self.requested_state.set(match state {
                    gst::State::Playing => PlayerState::Playing,
                    gst::State::Paused => PlayerState::Paused,
                    gst::State::Null => PlayerState::Stopped,
                    _ => self.requested_state.get(),
                });
            })
            .map_err(|err| format!("failed to set GStreamer state to {state:?}: {err}"))
    }
}

impl Drop for GStreamerBackend {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl Default for Player {
    fn default() -> Self {
        Self {
            state: PlayerState::Stopped,
            duration_ms: None,
            bitrate: 0,
            frequency: 0,
            channels: 0,
            volume: 100,
            balance: 0,
            vis_data: [0.0; 75],
            vis_data_valid: false,
        }
    }
}

impl Player {
    pub fn state(&self) -> PlayerState {
        self.state
    }

    pub fn duration_ms(&self) -> Option<i64> {
        self.duration_ms
    }

    pub fn mark_playing(&mut self) {
        self.state = PlayerState::Playing;
    }

    pub fn pause(&mut self) {
        if self.state == PlayerState::Playing {
            self.state = PlayerState::Paused;
        }
    }

    pub fn unpause(&mut self) {
        if self.state == PlayerState::Paused {
            self.state = PlayerState::Playing;
        }
    }

    pub fn stop(&mut self) {
        self.state = PlayerState::Stopped;
        self.duration_ms = None;
        self.bitrate = 0;
        self.frequency = 0;
        self.channels = 0;
    }

    pub fn set_volume(&mut self, percent: i32) {
        self.volume = percent.clamp(0, 100);
    }

    pub fn volume(&self) -> i32 {
        self.volume
    }

    pub fn set_balance(&mut self, balance: i32) {
        self.balance = balance.clamp(-100, 100);
    }

    pub fn balance(&self) -> i32 {
        self.balance
    }

    pub fn set_stream_info(
        &mut self,
        bitrate: Option<i32>,
        frequency: Option<i32>,
        channels: Option<i32>,
    ) {
        if let Some(bitrate) = bitrate {
            self.bitrate = bitrate.max(0);
        }
        if let Some(frequency) = frequency {
            self.frequency = frequency.max(0);
        }
        if let Some(channels) = channels {
            self.channels = channels.max(0);
        }
    }

    pub fn bitrate(&self) -> i32 {
        self.bitrate
    }

    pub fn frequency(&self) -> i32 {
        self.frequency
    }

    pub fn channels(&self) -> i32 {
        self.channels
    }

    pub fn set_visualization_data(&mut self, data: [f32; SPECTRUM_BANDS]) {
        self.vis_data = data;
        self.vis_data_valid = true;
    }

    pub fn visualization_data(&self) -> &[f32; SPECTRUM_BANDS] {
        &self.vis_data
    }

    pub fn visualization_data_valid(&self) -> bool {
        self.vis_data_valid
    }

    pub fn apply_playback_event(&mut self, event: &PlaybackEvent) {
        match event {
            PlaybackEvent::Tags(tags) => {
                self.set_stream_info(tags.bitrate, None, None);
            }
            PlaybackEvent::StreamInfo(info) => {
                self.set_stream_info(info.bitrate, info.frequency, info.channels);
            }
            PlaybackEvent::Spectrum(data) => self.set_visualization_data(*data),
            PlaybackEvent::EndOfStream | PlaybackEvent::Error(_) => self.stop(),
            PlaybackEvent::DurationChanged(duration) => {
                self.duration_ms = *duration;
            }
        }
    }
}

fn build_audio_sink_bin(sink_factory: &str, device: Option<&str>) -> Result<AudioSinkBin, String> {
    let bin = gst::Bin::builder().name("audio-sink-bin").build();
    let convert = make_element("audioconvert", "convert")?;
    let panorama = make_element("audiopanorama", "panorama")?;
    let equalizer = make_element("equalizer-10bands", "eq")?;
    let spectrum = make_element("spectrum", "spectrum")?;
    let sink = make_element(sink_factory, "sink")?;

    if let Some(device) = device {
        if sink.find_property("device").is_none() {
            return Err(format!(
                "GStreamer sink {sink_factory} does not support selecting an output device"
            ));
        }
        sink.set_property("device", device);
    }

    spectrum.set_property("bands", 75u32);
    spectrum.set_property("threshold", -80i32);
    spectrum.set_property("post-messages", true);
    spectrum.set_property("interval", 50_000_000u64);
    spectrum.set_property("message-magnitude", true);

    bin.add(&convert)
        .map_err(|err| format!("failed to add audioconvert to bin: {err}"))?;
    bin.add(&panorama)
        .map_err(|err| format!("failed to add audiopanorama to bin: {err}"))?;
    bin.add(&equalizer)
        .map_err(|err| format!("failed to add equalizer-10bands to bin: {err}"))?;
    bin.add(&spectrum)
        .map_err(|err| format!("failed to add spectrum to bin: {err}"))?;
    bin.add(&sink)
        .map_err(|err| format!("failed to add autoaudiosink to bin: {err}"))?;

    let chain = vec![
        convert.clone(),
        panorama.clone(),
        equalizer.clone(),
        spectrum.clone(),
        sink.clone(),
    ];

    for pair in chain.windows(2) {
        pair[0]
            .link(&pair[1])
            .map_err(|err| format!("failed to link audio output chain: {err}"))?;
    }

    let sink_pad = convert
        .static_pad("sink")
        .ok_or_else(|| "audioconvert sink pad is missing".to_string())?;
    let ghost_pad = gst::GhostPad::with_target(&sink_pad)
        .map_err(|err| format!("failed to create audio sink ghost pad: {err}"))?;
    ghost_pad
        .set_active(true)
        .map_err(|err| format!("failed to activate audio sink ghost pad: {err}"))?;
    bin.add_pad(&ghost_pad)
        .map_err(|err| format!("failed to add audio sink ghost pad: {err}"))?;

    let names = chain
        .iter()
        .filter_map(|element| element.factory().map(|factory| factory.name()))
        .map(|name| name.to_string())
        .collect();
    Ok(AudioSinkBin {
        bin,
        chain: names,
        panorama,
        equalizer,
    })
}

fn make_element(factory: &str, name: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .map_err(|err| format!("failed to create GStreamer {factory}: {err}"))
}

fn equalizer_band_property(band: usize) -> Result<&'static str, String> {
    match band {
        0 => Ok("band0"),
        1 => Ok("band1"),
        2 => Ok("band2"),
        3 => Ok("band3"),
        4 => Ok("band4"),
        5 => Ok("band5"),
        6 => Ok("band6"),
        7 => Ok("band7"),
        8 => Ok("band8"),
        9 => Ok("band9"),
        _ => Err(format!("equalizer band index {band} is out of range")),
    }
}

fn event_from_message(
    message: &gst::Message,
    duration_query: impl FnOnce() -> Option<i64>,
) -> Option<PlaybackEvent> {
    match message.view() {
        gst::MessageView::Eos(_) => Some(PlaybackEvent::EndOfStream),
        gst::MessageView::Error(error) => Some(PlaybackEvent::Error(error.error().to_string())),
        gst::MessageView::DurationChanged(_) => {
            Some(PlaybackEvent::DurationChanged(duration_query()))
        }
        gst::MessageView::Tag(tag) => Some(PlaybackEvent::Tags(tags_from_tag_list(&tag.tags()))),
        gst::MessageView::Element(element) => element
            .structure()
            .and_then(spectrum_from_structure)
            .map(PlaybackEvent::Spectrum),
        _ => None,
    }
}

fn query_duration_ms(pipeline: &gst::Element) -> Option<i64> {
    pipeline
        .query_duration::<gst::ClockTime>()
        .map(|duration| duration.mseconds() as i64)
}

fn tags_from_tag_list(tags: &gst::TagList) -> PlaybackTags {
    PlaybackTags {
        title: tag_string::<gst::tags::Title>(tags),
        artist: tag_string::<gst::tags::Artist>(tags),
        audio_codec: tag_string::<gst::tags::AudioCodec>(tags),
        bitrate: tags
            .get::<gst::tags::Bitrate>()
            .map(|value| (value.get() / 1000) as i32),
    }
}

fn stream_info_from_caps(caps: &gst::Caps) -> StreamInfo {
    let mut info = StreamInfo::default();
    for structure in caps.iter() {
        if !structure.name().starts_with("audio/") {
            continue;
        }
        if info.frequency.is_none() {
            info.frequency = structure.get::<i32>("rate").ok().filter(|value| *value > 0);
        }
        if info.channels.is_none() {
            info.channels = structure
                .get::<i32>("channels")
                .ok()
                .filter(|value| *value > 0);
        }
    }
    info
}

fn tag_string<'a, T>(tags: &'a gst::TagList) -> Option<String>
where
    T: gst::Tag<'a, TagType = &'a str>,
{
    tags.get::<T>()
        .map(|value| value.get().to_string())
        .filter(|value| !value.is_empty())
}

fn spectrum_from_structure(structure: &gst::StructureRef) -> Option<[f32; SPECTRUM_BANDS]> {
    if structure.name() != "spectrum" {
        return None;
    }

    let magnitudes = structure.get::<gst::Array>("magnitude").ok()?;
    let mut bands = [0.0; SPECTRUM_BANDS];
    for (index, value) in magnitudes
        .as_slice()
        .iter()
        .take(SPECTRUM_BANDS)
        .enumerate()
    {
        let magnitude = value.get::<f64>().ok()? as f32;
        bands[index] = ((magnitude + 80.0) / 80.0).clamp(0.0, 1.0);
    }
    Some(bands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};

    static GST_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn gst_test_guard() -> MutexGuard<'static, ()> {
        GST_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn backend_with_test_sink() -> GStreamerBackend {
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");
        let fake_audio = make_element("fakesink", "testaudio").expect("fakesink should construct");
        backend.pipeline.set_property("audio-sink", &fake_audio);
        backend
    }

    fn silent_wav_uri(name: &str) -> String {
        let path = std::env::temp_dir().join(format!("xmms-rs-{name}.wav"));
        std::fs::write(&path, silent_wav_bytes()).expect("silent wav fixture should be written");
        path_to_uri(&path)
    }

    fn path_to_uri(path: &PathBuf) -> String {
        format!("file://{}", path.to_string_lossy())
    }

    fn silent_wav_bytes() -> Vec<u8> {
        let sample_rate = 8_000u32;
        let samples = 8_000u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + samples).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&samples.to_le_bytes());
        bytes.extend(std::iter::repeat_n(128u8, samples as usize));
        bytes
    }

    #[test]
    fn volume_and_balance_are_clamped_like_the_c_player() {
        let mut player = Player::default();
        player.set_volume(150);
        player.set_balance(-250);
        assert_eq!(player.volume(), 100);
        assert_eq!(player.balance(), -100);
    }

    #[test]
    fn pause_only_changes_a_playing_player() {
        let mut player = Player::default();
        player.pause();
        assert_eq!(player.state(), PlayerState::Stopped);
        player.mark_playing();
        player.pause();
        assert_eq!(player.state(), PlayerState::Paused);
        player.unpause();
        assert_eq!(player.state(), PlayerState::Playing);
    }

    #[test]
    fn gstreamer_backend_uses_playbin_with_video_disabled() {
        let _guard = gst_test_guard();
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");

        assert_eq!(backend.pipeline_factory_name().as_deref(), Some("playbin"));
        assert_eq!(
            backend.video_sink_factory_name().as_deref(),
            Some("fakesink")
        );
    }

    #[test]
    fn gstreamer_backend_builds_audio_output_chain() {
        let _guard = gst_test_guard();
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");
        let chain = backend.audio_chain();

        assert_eq!(backend.audio_sink_bin_name(), "audio-sink-bin");
        assert_eq!(
            chain,
            &[
                "audioconvert".to_string(),
                "audiopanorama".to_string(),
                "equalizer-10bands".to_string(),
                "spectrum".to_string(),
                "autoaudiosink".to_string(),
            ]
        );
    }

    #[test]
    fn gstreamer_backend_rebuilds_output_sink_chain() {
        let _guard = gst_test_guard();
        let mut backend = GStreamerBackend::new().expect("GStreamer backend should construct");

        backend
            .rebuild_output_sink("fakesink", None)
            .expect("fakesink output chain should rebuild");
        assert_eq!(
            backend.audio_chain(),
            &[
                "audioconvert".to_string(),
                "audiopanorama".to_string(),
                "equalizer-10bands".to_string(),
                "spectrum".to_string(),
                "fakesink".to_string(),
            ]
        );

        assert!(backend
            .rebuild_output_sink("fakesink", Some("device-id"))
            .is_err());
    }

    #[test]
    fn gstreamer_backend_play_uri_sets_uri_and_stop_returns_to_stopped() {
        let _guard = gst_test_guard();
        let backend = backend_with_test_sink();
        let uri = silent_wav_uri("play-uri");

        backend.play_uri(&uri).expect("play should request playing");
        assert_eq!(backend.uri().as_deref(), Some(uri.as_str()));

        backend.stop().expect("stop should request the null state");
        assert_eq!(backend.playback_state(), PlayerState::Stopped);
    }

    #[test]
    fn gstreamer_backend_pause_unpause_and_toggle_drive_state_requests() {
        let _guard = gst_test_guard();
        let backend = backend_with_test_sink();
        let uri = silent_wav_uri("pause-toggle");

        backend.play_uri(&uri).expect("play should request playing");
        backend.pause().expect("pause should request paused state");
        assert_eq!(backend.playback_state(), PlayerState::Paused);

        let toggled = backend
            .toggle_pause()
            .expect("toggle from paused should request playing");
        assert_eq!(toggled, PlayerState::Playing);

        let toggled = backend
            .toggle_pause()
            .expect("toggle from playing should request paused");
        assert_eq!(toggled, PlayerState::Paused);
        backend.stop().expect("stop should request null state");
    }

    #[test]
    fn gstreamer_backend_seek_and_position_queries_are_safe_without_media() {
        let _guard = gst_test_guard();
        let backend = backend_with_test_sink();
        let uri = silent_wav_uri("seek");

        assert!(backend.seek_to_ms(-1).is_err());
        backend.play_uri(&uri).expect("play should request playing");
        let _ = backend.seek_to_ms(1_000);
        let _ = backend.position_ms();
        let _ = backend.duration_ms();
    }

    #[test]
    fn gstreamer_backend_volume_and_balance_map_to_pipeline_properties() {
        let _guard = gst_test_guard();
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");

        backend.set_volume_percent(150);
        backend.set_balance_percent(-250);
        assert_eq!(backend.volume_percent(), 100);
        assert_eq!(backend.balance_percent(), -100);

        backend.set_volume_percent(33);
        backend.set_balance_percent(25);
        assert_eq!(backend.volume_percent(), 33);
        assert_eq!(backend.balance_percent(), 25);
    }

    #[test]
    fn gstreamer_backend_equalizer_bands_map_to_gstreamer_properties() {
        let _guard = gst_test_guard();
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");

        backend
            .set_equalizer_band_db(0, 24.0)
            .expect("band 0 should be set");
        backend
            .set_equalizer_band_db(9, -48.0)
            .expect("band 9 should be set");
        assert_eq!(backend.equalizer_band_db(0).unwrap(), 12.0);
        assert_eq!(backend.equalizer_band_db(9).unwrap(), -24.0);
        assert!(backend.set_equalizer_band_db(10, 0.0).is_err());

        backend.set_equalizer_bands_db([-9.0, -7.0, -5.0, -3.0, -1.0, 1.0, 3.0, 5.0, 7.0, 9.0]);
        for (index, expected) in [-9.0, -7.0, -5.0, -3.0, -1.0, 1.0, 3.0, 5.0, 7.0, 9.0]
            .into_iter()
            .enumerate()
        {
            assert_eq!(backend.equalizer_band_db(index).unwrap(), expected);
        }
    }

    #[test]
    fn player_stream_info_and_playback_events_update_runtime_fields() {
        let mut player = Player::default();
        player.set_stream_info(Some(192), Some(44_100), Some(2));
        assert_eq!(player.bitrate(), 192);
        assert_eq!(player.frequency(), 44_100);
        assert_eq!(player.channels(), 2);

        player.apply_playback_event(&PlaybackEvent::Tags(PlaybackTags {
            title: Some("Song".to_string()),
            artist: None,
            audio_codec: None,
            bitrate: Some(256),
        }));
        assert_eq!(player.bitrate(), 256);

        player.apply_playback_event(&PlaybackEvent::StreamInfo(StreamInfo {
            bitrate: None,
            frequency: Some(48_000),
            channels: Some(6),
        }));
        assert_eq!(player.frequency(), 48_000);
        assert_eq!(player.channels(), 6);

        player.apply_playback_event(&PlaybackEvent::DurationChanged(Some(12_000)));
        assert_eq!(player.duration_ms(), Some(12_000));

        let mut spectrum = [0.0; SPECTRUM_BANDS];
        spectrum[3] = 0.75;
        player.apply_playback_event(&PlaybackEvent::Spectrum(spectrum));
        assert!(player.visualization_data_valid());
        assert_eq!(player.visualization_data()[3], 0.75);

        player.mark_playing();
        player.apply_playback_event(&PlaybackEvent::EndOfStream);
        assert_eq!(player.state(), PlayerState::Stopped);
    }

    #[test]
    fn stream_info_from_caps_reports_audio_frequency_and_channels() {
        let _guard = gst_test_guard();
        gst::init().expect("GStreamer should initialize");
        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", 44_100i32)
            .field("channels", 2i32)
            .build();

        assert_eq!(
            stream_info_from_caps(&caps),
            StreamInfo {
                bitrate: None,
                frequency: Some(44_100),
                channels: Some(2),
            }
        );

        let video_caps = gst::Caps::builder("video/x-raw")
            .field("rate", 30i32)
            .build();
        assert_eq!(stream_info_from_caps(&video_caps), StreamInfo::default());
    }

    #[test]
    fn gstreamer_bus_events_cover_eos_errors_and_duration_changes() {
        let _guard = gst_test_guard();
        gst::init().expect("GStreamer should initialize");

        assert_eq!(
            event_from_message(&gst::message::Eos::new(), || None),
            Some(PlaybackEvent::EndOfStream)
        );
        assert_eq!(
            event_from_message(
                &gst::message::Error::new(gst::CoreError::Failed, "decode failed"),
                || None
            ),
            Some(PlaybackEvent::Error("decode failed".to_string()))
        );
        assert_eq!(
            event_from_message(&gst::message::DurationChanged::new(), || Some(12_345)),
            Some(PlaybackEvent::DurationChanged(Some(12_345)))
        );
    }

    #[test]
    fn gstreamer_bus_tag_messages_extract_player_metadata() {
        let _guard = gst_test_guard();
        gst::init().expect("GStreamer should initialize");
        let mut tags = gst::TagList::new();
        {
            let tags = tags.get_mut().expect("new tag list should be mutable");
            tags.add::<gst::tags::Title>(&"Song", gst::TagMergeMode::Replace);
            tags.add::<gst::tags::Artist>(&"Artist", gst::TagMergeMode::Replace);
            tags.add::<gst::tags::AudioCodec>(&"Vorbis", gst::TagMergeMode::Replace);
            tags.add::<gst::tags::Bitrate>(&128_000u32, gst::TagMergeMode::Replace);
        }

        assert_eq!(
            event_from_message(&gst::message::Tag::new(tags), || None),
            Some(PlaybackEvent::Tags(PlaybackTags {
                title: Some("Song".to_string()),
                artist: Some("Artist".to_string()),
                audio_codec: Some("Vorbis".to_string()),
                bitrate: Some(128),
            }))
        );
    }

    #[test]
    fn gstreamer_bus_spectrum_messages_extract_visualizer_bands() {
        let _guard = gst_test_guard();
        gst::init().expect("GStreamer should initialize");
        let magnitudes = gst::Array::new([-80.0f64, -40.0, 0.0]);
        let structure = gst::Structure::builder("spectrum")
            .field("magnitude", magnitudes)
            .build();

        let event = event_from_message(&gst::message::Element::new(structure), || None)
            .expect("spectrum structure should produce a visualizer event");
        let PlaybackEvent::Spectrum(bands) = event else {
            panic!("expected spectrum event");
        };

        assert_eq!(bands[0], 0.0);
        assert_eq!(bands[1], 0.5);
        assert_eq!(bands[2], 1.0);
        assert_eq!(bands[3], 0.0);
    }
}
