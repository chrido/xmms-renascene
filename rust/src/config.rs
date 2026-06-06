use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::skin::widget::{
    VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode, VisScopeMode, VisVuMode,
};

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
    pub playback_position_ms: i64,
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
    pub vis_analyzer_mode: VisAnalyzerMode,
    pub vis_analyzer_style: VisAnalyzerStyle,
    pub vis_scope_mode: VisScopeMode,
    pub vis_peaks_enabled: bool,
    pub vis_falloff: f64,
    pub vis_analyzer_falloff: VisFalloffSpeed,
    pub vis_peaks_falloff: VisFalloffSpeed,
    pub vis_vu_mode: VisVuMode,
    pub vis_refresh_divisor: i32,
    pub podcast_cache_ttl_days: i32,
    pub podcast_refresh_interval_minutes: i32,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMode {
    Elapsed = 0,
    Remaining = 1,
}

impl TimerMode {
    fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Remaining,
            _ => Self::Elapsed,
        }
    }
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
            playback_position_ms: 0,
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
            vis_analyzer_mode: VisAnalyzerMode::Normal,
            vis_analyzer_style: VisAnalyzerStyle::Bars,
            vis_scope_mode: VisScopeMode::Line,
            vis_peaks_enabled: true,
            vis_falloff: 0.04,
            vis_analyzer_falloff: VisFalloffSpeed::Medium,
            vis_peaks_falloff: VisFalloffSpeed::Slow,
            vis_vu_mode: VisVuMode::Normal,
            vis_refresh_divisor: 1,
            podcast_cache_ttl_days: 60,
            podcast_refresh_interval_minutes: 60,
        }
    }
}

impl Config {
    pub fn load_from_file(path: &Path) -> io::Result<Self> {
        let contents = fs::read_to_string(path)?;
        Ok(Self::from_key_file_str(&contents))
    }

    pub fn save_to_file(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_key_file_string())
    }

    pub fn from_key_file_str(contents: &str) -> Self {
        let mut cfg = Self::default();
        let keys = parse_xmms_section(contents);

        cfg.player_x = get_i32(&keys, "player_x").unwrap_or(cfg.player_x);
        cfg.player_y = get_i32(&keys, "player_y").unwrap_or(cfg.player_y);
        cfg.scale_factor = get_f64(&keys, "scale_factor")
            .unwrap_or_else(|| {
                if get_bool(&keys, "doublesize").unwrap_or(cfg.doublesize) {
                    2.0
                } else {
                    1.0
                }
            })
            .clamp(1.0, 5.0);
        cfg.doublesize = cfg.scale_factor > 1.0;
        cfg.skin = get_non_empty_string(&keys, "skin");
        cfg.output_device = get_non_empty_string(&keys, "output_device");
        cfg.timer_mode = TimerMode::from_i32(get_i32(&keys, "timer_mode").unwrap_or(0));
        cfg.volume = get_i32(&keys, "volume").unwrap_or(cfg.volume).clamp(0, 100);
        cfg.balance = get_i32(&keys, "balance")
            .unwrap_or(cfg.balance)
            .clamp(-100, 100);
        cfg.no_playlist_advance =
            get_bool(&keys, "no_playlist_advance").unwrap_or(cfg.no_playlist_advance);
        cfg.sticky = get_bool(&keys, "sticky").unwrap_or(cfg.sticky);
        cfg.easy_move = get_bool(&keys, "easy_move").unwrap_or(cfg.easy_move);
        cfg.playlist_visible = get_bool(&keys, "playlist_visible").unwrap_or(cfg.playlist_visible);
        cfg.playlist_detached =
            get_bool(&keys, "playlist_detached").unwrap_or(cfg.playlist_detached);
        cfg.shuffle = get_bool(&keys, "shuffle").unwrap_or(cfg.shuffle);
        cfg.repeat = get_bool(&keys, "repeat").unwrap_or(cfg.repeat);
        cfg.playlist_position =
            get_i32(&keys, "playlist_position").unwrap_or(cfg.playlist_position);
        cfg.playback_position_ms = get_i64(&keys, "playback_position_ms")
            .unwrap_or(cfg.playback_position_ms)
            .max(0);
        cfg.equalizer_visible =
            get_bool(&keys, "equalizer_visible").unwrap_or(cfg.equalizer_visible);
        cfg.equalizer_detached =
            get_bool(&keys, "equalizer_detached").unwrap_or(cfg.equalizer_detached);
        cfg.equalizer_active = get_bool(&keys, "equalizer_active").unwrap_or(cfg.equalizer_active);
        cfg.equalizer_auto = get_bool(&keys, "equalizer_auto").unwrap_or(cfg.equalizer_auto);
        cfg.equalizer_preamp_pos = get_i32(&keys, "equalizer_preamp_pos")
            .unwrap_or(cfg.equalizer_preamp_pos)
            .clamp(0, 100);
        for i in 0..10 {
            let key = format!("equalizer_band_{i}_pos");
            cfg.equalizer_band_pos[i] = get_i32(&keys, &key)
                .unwrap_or(cfg.equalizer_band_pos[i])
                .clamp(0, 100);
        }
        cfg.convert_underscore =
            get_bool(&keys, "convert_underscore").unwrap_or(cfg.convert_underscore);
        cfg.convert_twenty = get_bool(&keys, "convert_twenty").unwrap_or(cfg.convert_twenty);
        cfg.show_numbers_in_pl =
            get_bool(&keys, "show_numbers_in_pl").unwrap_or(cfg.show_numbers_in_pl);
        if let Some(value) = get_non_empty_string(&keys, "playlist_font") {
            cfg.playlist_font = value;
        }
        if let Some(value) = get_non_empty_string(&keys, "mainwin_font") {
            cfg.mainwin_font = value;
        }
        if let Some(value) = get_non_empty_string(&keys, "title_format") {
            cfg.title_format = value;
        }
        cfg.vis_mode = VisMode::from_i32(get_i32(&keys, "vis_mode").unwrap_or(cfg.vis_mode as i32));
        cfg.vis_analyzer_mode = VisAnalyzerMode::from_i32(
            get_i32(&keys, "vis_analyzer_mode").unwrap_or(cfg.vis_analyzer_mode as i32),
        );
        cfg.vis_analyzer_style = VisAnalyzerStyle::from_i32(
            get_i32(&keys, "vis_analyzer_style").unwrap_or(cfg.vis_analyzer_style as i32),
        );
        cfg.vis_scope_mode = VisScopeMode::from_i32(
            get_i32(&keys, "vis_scope_mode").unwrap_or(cfg.vis_scope_mode as i32),
        );
        cfg.vis_peaks_enabled =
            get_bool(&keys, "vis_peaks_enabled").unwrap_or(cfg.vis_peaks_enabled);
        cfg.vis_falloff = get_f64(&keys, "vis_falloff")
            .unwrap_or(cfg.vis_falloff)
            .clamp(0.001, 0.25);
        cfg.vis_analyzer_falloff = VisFalloffSpeed::from_i32(
            get_i32(&keys, "vis_analyzer_falloff").unwrap_or(cfg.vis_analyzer_falloff as i32),
        );
        cfg.vis_peaks_falloff = VisFalloffSpeed::from_i32(
            get_i32(&keys, "vis_peaks_falloff").unwrap_or(cfg.vis_peaks_falloff as i32),
        );
        cfg.vis_vu_mode =
            VisVuMode::from_i32(get_i32(&keys, "vis_vu_mode").unwrap_or(cfg.vis_vu_mode as i32));
        cfg.vis_refresh_divisor = get_i32(&keys, "vis_refresh_divisor")
            .unwrap_or(cfg.vis_refresh_divisor)
            .clamp(1, 8);
        cfg.podcast_cache_ttl_days = get_i32(&keys, "podcast_cache_ttl_days")
            .unwrap_or(cfg.podcast_cache_ttl_days)
            .clamp(1, 3650);
        cfg.podcast_refresh_interval_minutes = get_i32(&keys, "podcast_refresh_interval_minutes")
            .unwrap_or(cfg.podcast_refresh_interval_minutes)
            .clamp(1, 10080);
        cfg
    }

    pub fn to_key_file_string(&self) -> String {
        let mut out = String::from("[xmms]\n");
        push_i32(&mut out, "player_x", self.player_x);
        push_i32(&mut out, "player_y", self.player_y);
        push_f64(&mut out, "scale_factor", self.scale_factor);
        push_i32(&mut out, "timer_mode", self.timer_mode as i32);
        push_i32(&mut out, "volume", self.volume);
        push_i32(&mut out, "balance", self.balance);
        push_bool(&mut out, "no_playlist_advance", self.no_playlist_advance);
        push_bool(&mut out, "sticky", self.sticky);
        push_bool(&mut out, "doublesize", self.scale_factor > 1.0);
        push_bool(&mut out, "easy_move", self.easy_move);
        push_bool(&mut out, "playlist_visible", self.playlist_visible);
        push_bool(&mut out, "playlist_detached", self.playlist_detached);
        push_bool(&mut out, "shuffle", self.shuffle);
        push_bool(&mut out, "repeat", self.repeat);
        push_i32(&mut out, "playlist_position", self.playlist_position);
        push_i64(&mut out, "playback_position_ms", self.playback_position_ms);
        push_bool(&mut out, "equalizer_visible", self.equalizer_visible);
        push_bool(&mut out, "equalizer_detached", self.equalizer_detached);
        push_bool(&mut out, "equalizer_active", self.equalizer_active);
        push_bool(&mut out, "equalizer_auto", self.equalizer_auto);
        push_i32(&mut out, "equalizer_preamp_pos", self.equalizer_preamp_pos);
        for i in 0..10 {
            push_i32(
                &mut out,
                &format!("equalizer_band_{i}_pos"),
                self.equalizer_band_pos[i],
            );
        }
        if let Some(skin) = &self.skin {
            push_string(&mut out, "skin", skin);
        }
        push_bool(&mut out, "convert_underscore", self.convert_underscore);
        push_bool(&mut out, "convert_twenty", self.convert_twenty);
        push_bool(&mut out, "show_numbers_in_pl", self.show_numbers_in_pl);
        push_string(&mut out, "playlist_font", &self.playlist_font);
        push_string(&mut out, "mainwin_font", &self.mainwin_font);
        push_string(&mut out, "title_format", &self.title_format);
        push_i32(&mut out, "vis_mode", self.vis_mode as i32);
        push_i32(&mut out, "vis_analyzer_mode", self.vis_analyzer_mode as i32);
        push_i32(
            &mut out,
            "vis_analyzer_style",
            self.vis_analyzer_style as i32,
        );
        push_i32(&mut out, "vis_scope_mode", self.vis_scope_mode as i32);
        push_bool(&mut out, "vis_peaks_enabled", self.vis_peaks_enabled);
        push_f64(&mut out, "vis_falloff", self.vis_falloff);
        push_i32(
            &mut out,
            "vis_analyzer_falloff",
            self.vis_analyzer_falloff as i32,
        );
        push_i32(&mut out, "vis_peaks_falloff", self.vis_peaks_falloff as i32);
        push_i32(&mut out, "vis_vu_mode", self.vis_vu_mode as i32);
        push_i32(&mut out, "vis_refresh_divisor", self.vis_refresh_divisor);
        push_i32(
            &mut out,
            "podcast_cache_ttl_days",
            self.podcast_cache_ttl_days,
        );
        push_i32(
            &mut out,
            "podcast_refresh_interval_minutes",
            self.podcast_refresh_interval_minutes,
        );
        if let Some(output_device) = &self.output_device {
            push_string(&mut out, "output_device", output_device);
        }
        out
    }
}

fn parse_xmms_section(contents: &str) -> BTreeMap<String, String> {
    let mut in_xmms = false;
    let mut keys = BTreeMap::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_xmms = &line[1..line.len() - 1] == "xmms";
            continue;
        }
        if in_xmms {
            if let Some((key, value)) = line.split_once('=') {
                keys.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    keys
}

fn get_non_empty_string(keys: &BTreeMap<String, String>, key: &str) -> Option<String> {
    keys.get(key).filter(|value| !value.is_empty()).cloned()
}

fn get_i32(keys: &BTreeMap<String, String>, key: &str) -> Option<i32> {
    keys.get(key)?.parse().ok()
}

fn get_i64(keys: &BTreeMap<String, String>, key: &str) -> Option<i64> {
    keys.get(key)?.parse().ok()
}

fn get_f64(keys: &BTreeMap<String, String>, key: &str) -> Option<f64> {
    keys.get(key)?.parse().ok()
}

fn get_bool(keys: &BTreeMap<String, String>, key: &str) -> Option<bool> {
    match keys.get(key)?.as_str() {
        "true" | "True" | "TRUE" | "1" => Some(true),
        "false" | "False" | "FALSE" | "0" => Some(false),
        _ => None,
    }
}

fn push_i32(out: &mut String, key: &str, value: i32) {
    out.push_str(&format!("{key}={value}\n"));
}

fn push_i64(out: &mut String, key: &str, value: i64) {
    out.push_str(&format!("{key}={value}\n"));
}

fn push_f64(out: &mut String, key: &str, value: f64) {
    out.push_str(&format!("{key}={value}\n"));
}

fn push_bool(out: &mut String, key: &str, value: bool) {
    out.push_str(&format!("{key}={}\n", if value { "true" } else { "false" }));
}

fn push_string(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
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
        assert_eq!(cfg.vis_refresh_divisor, 1);
        assert_eq!(cfg.podcast_cache_ttl_days, 60);
    }

    #[test]
    fn loads_c_keyfile_values_with_clamping_and_legacy_doublesize() {
        let cfg = Config::from_key_file_str(
            "[xmms]\n\
             player_x=12\n\
             doublesize=false\n\
             volume=250\n\
             balance=-250\n\
             equalizer_band_3_pos=75\n\
             playlist_font=Monospace\n\
             vis_mode=1\n\
             vis_refresh_divisor=99\n\
             podcast_cache_ttl_days=0\n",
        );

        assert_eq!(cfg.player_x, 12);
        assert_eq!(cfg.scale_factor, 1.0);
        assert!(!cfg.doublesize);
        assert_eq!(cfg.volume, 100);
        assert_eq!(cfg.balance, -100);
        assert_eq!(cfg.equalizer_band_pos[3], 75);
        assert_eq!(cfg.playlist_font, "Monospace");
        assert_eq!(cfg.vis_mode, VisMode::Scope);
        assert_eq!(cfg.vis_refresh_divisor, 8);
        assert_eq!(cfg.podcast_cache_ttl_days, 1);
    }

    #[test]
    fn saves_and_reloads_known_config_keys() {
        let mut cfg = Config {
            skin: Some("/skins/classic".to_string()),
            output_device: Some("pipewire.node".to_string()),
            playlist_visible: true,
            equalizer_band_pos: [10; 10],
            ..Config::default()
        };
        cfg.vis_mode = VisMode::Off;

        let serialized = cfg.to_key_file_string();
        assert!(serialized.contains("[xmms]\n"));
        assert!(serialized.contains("skin=/skins/classic\n"));
        assert!(serialized.contains("output_device=pipewire.node\n"));
        assert!(serialized.contains("equalizer_band_9_pos=10\n"));

        let reparsed = Config::from_key_file_str(&serialized);
        assert_eq!(reparsed.skin, cfg.skin);
        assert_eq!(reparsed.output_device, cfg.output_device);
        assert!(reparsed.playlist_visible);
        assert_eq!(reparsed.equalizer_band_pos, [10; 10]);
        assert_eq!(reparsed.vis_mode, VisMode::Off);
    }
}
