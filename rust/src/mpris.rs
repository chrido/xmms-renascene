use crate::player::PlayerState;

pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.xmms_resuscitated";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprisRootProperties {
    pub can_quit: bool,
    pub can_raise: bool,
    pub has_track_list: bool,
    pub identity: &'static str,
    pub desktop_entry: &'static str,
    pub supported_uri_schemes: Vec<&'static str>,
    pub supported_mime_types: Vec<&'static str>,
}

impl Default for MprisRootProperties {
    fn default() -> Self {
        Self {
            can_quit: true,
            can_raise: true,
            has_track_list: false,
            identity: "XMMS Resuscitated",
            desktop_entry: "org.xmms.Resuscitated",
            supported_uri_schemes: vec!["file", "http", "https"],
            supported_mime_types: vec![
                "audio/mpeg",
                "audio/ogg",
                "audio/flac",
                "audio/x-wav",
                "audio/mp4",
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprisMetadata {
    pub track_id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub length_us: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MprisPlayerProperties {
    pub playback_status: &'static str,
    pub rate: f64,
    pub metadata: MprisMetadata,
    pub volume: f64,
    pub position_us: i64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
    pub can_control: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisCommand {
    Raise,
    Quit,
    Next,
    Previous,
    Pause,
    PlayPause,
    Stop,
    Play,
    Seek { offset_us: i64 },
    SetPosition { track_id: String, position_us: i64 },
    OpenUri(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisEvent {
    Raised,
    QuitRequested,
    MetadataChanged,
    PlaybackStatusChanged,
    Seeked(i64),
}

pub fn playback_status(state: PlayerState) -> &'static str {
    match state {
        PlayerState::Playing => "Playing",
        PlayerState::Paused => "Paused",
        PlayerState::Stopped => "Stopped",
    }
}
