use gst::prelude::*;
use gstreamer as gst;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        let backend = GStreamerBackend::new().expect("GStreamer backend should construct");

        assert_eq!(backend.pipeline_factory_name().as_deref(), Some("playbin"));
        assert_eq!(
            backend.video_sink_factory_name().as_deref(),
            Some("fakesink")
        );
    }

    #[test]
    fn gstreamer_backend_builds_audio_output_chain() {
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
}
