use gst::prelude::*;
use gstreamer as gst;

const SPECTRUM_BANDS: usize = 75;

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
    audio_chain: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub audio_codec: Option<String>,
    pub bitrate: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackEvent {
    EndOfStream,
    Error(String),
    DurationChanged(Option<i64>),
    Tags(PlaybackTags),
    Spectrum([f32; SPECTRUM_BANDS]),
}

impl GStreamerBackend {
    pub fn new() -> Result<Self, String> {
        gst::init().map_err(|err| format!("failed to initialize GStreamer: {err}"))?;

        let pipeline = make_element("playbin", "player")?;
        let fake_video = make_element("fakesink", "fakevideo")?;
        pipeline.set_property("video-sink", &fake_video);

        let (audio_sink_bin, audio_chain) = build_audio_sink_bin()?;
        pipeline.set_property("audio-sink", &audio_sink_bin);

        Ok(Self {
            pipeline,
            audio_sink_bin,
            audio_chain,
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

    pub fn audio_chain(&self) -> &[&'static str] {
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

    fn event_from_message(&self, message: &gst::Message) -> Option<PlaybackEvent> {
        event_from_message(message, || query_duration_ms(&self.pipeline))
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
}

fn build_audio_sink_bin() -> Result<(gst::Bin, Vec<&'static str>), String> {
    let bin = gst::Bin::builder().name("audio-sink-bin").build();
    let convert = make_element("audioconvert", "convert")?;
    let panorama = make_element("audiopanorama", "panorama")?;
    let equalizer = make_element("equalizer-10bands", "eq")?;
    let spectrum = make_element("spectrum", "spectrum")?;
    let sink = make_element("autoaudiosink", "sink")?;

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
        .map(|name| match name.as_str() {
            "audioconvert" => "audioconvert",
            "audiopanorama" => "audiopanorama",
            "equalizer-10bands" => "equalizer-10bands",
            "spectrum" => "spectrum",
            "autoaudiosink" => "autoaudiosink",
            _ => "unknown",
        })
        .collect();
    Ok((bin, names))
}

fn make_element(factory: &str, name: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .map_err(|err| format!("failed to create GStreamer {factory}: {err}"))
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
    use std::sync::Mutex;

    static GST_TEST_LOCK: Mutex<()> = Mutex::new(());

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
        let _guard = GST_TEST_LOCK.lock().expect("GStreamer test lock poisoned");
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");

        assert_eq!(backend.pipeline_factory_name().as_deref(), Some("playbin"));
        assert_eq!(
            backend.video_sink_factory_name().as_deref(),
            Some("fakesink")
        );
    }

    #[test]
    fn gstreamer_backend_builds_audio_output_chain() {
        let _guard = GST_TEST_LOCK.lock().expect("GStreamer test lock poisoned");
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");
        let chain = backend.audio_chain();

        assert_eq!(backend.audio_sink_bin_name(), "audio-sink-bin");
        assert_eq!(
            chain,
            &[
                "audioconvert",
                "audiopanorama",
                "equalizer-10bands",
                "spectrum",
                "autoaudiosink"
            ]
        );
    }

    #[test]
    fn gstreamer_bus_events_cover_eos_errors_and_duration_changes() {
        let _guard = GST_TEST_LOCK.lock().expect("GStreamer test lock poisoned");
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
        let _guard = GST_TEST_LOCK.lock().expect("GStreamer test lock poisoned");
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
        let _guard = GST_TEST_LOCK.lock().expect("GStreamer test lock poisoned");
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
