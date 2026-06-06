use crate::player::PlayerState;
use crate::skin::widget::VisMode;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub player_x: i32,
    pub player_y: i32,
    pub scale_factor: f64,
    pub skin: Option<String>,
    pub timer_mode: TimerMode,
    pub output_device: Option<String>,
    pub volume: i32,
    pub balance: i32,
    pub no_playlist_advance: bool,
    pub sticky: bool,
    pub doublesize: bool,
    pub easy_move: bool,
    pub playlist_visible: bool,
    pub playlist_detached: bool,
    pub shuffle: bool,
    pub repeat: bool,
    pub playlist_position: i32,
    pub equalizer_visible: bool,
    pub equalizer_detached: bool,
    pub equalizer_active: bool,
    pub equalizer_auto: bool,
    pub equalizer_preamp_pos: i32,
    pub equalizer_band_pos: [i32; 10],
    pub convert_underscore: bool,
    pub convert_twenty: bool,
    pub show_numbers_in_pl: bool,
    pub playlist_font: String,
    pub mainwin_font: String,
    pub title_format: String,
    pub vis_mode: VisMode,
    pub podcast_cache_ttl_days: i32,
    pub podcast_refresh_interval_minutes: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMode {
    Elapsed,
    Remaining,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            player_x: 100,
            player_y: 100,
            scale_factor: 2.0,
            skin: None,
            timer_mode: TimerMode::Elapsed,
            output_device: None,
            volume: 100,
            balance: 0,
            no_playlist_advance: false,
            sticky: false,
            doublesize: true,
            easy_move: false,
            playlist_visible: false,
            playlist_detached: false,
            shuffle: false,
            repeat: false,
            playlist_position: -1,
            equalizer_visible: false,
            equalizer_detached: false,
            equalizer_active: true,
            equalizer_auto: false,
            equalizer_preamp_pos: 50,
            equalizer_band_pos: [50; 10],
            convert_underscore: true,
            convert_twenty: true,
            show_numbers_in_pl: true,
            playlist_font: "Helvetica".to_string(),
            mainwin_font: "Skin bitmap font".to_string(),
            title_format: "%p - %t".to_string(),
            vis_mode: VisMode::Analyzer,
            podcast_cache_ttl_days: 60,
            podcast_refresh_interval_minutes: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    pub player_state: PlayerState,
    pub playlist_position: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_c_application_starting_point() {
        let cfg = Config::default();
        assert_eq!(cfg.player_x, 100);
        assert_eq!(cfg.player_y, 100);
        assert_eq!(cfg.scale_factor, 2.0);
        assert_eq!(cfg.volume, 100);
        assert_eq!(cfg.balance, 0);
        assert_eq!(cfg.equalizer_band_pos, [50; 10]);
        assert_eq!(cfg.playlist_font, "Helvetica");
        assert_eq!(cfg.title_format, "%p - %t");
    }
}
