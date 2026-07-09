//! Frontend-neutral screenshot scenarios shared by GTK, egui, tests, and tools.

use crate::app_state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotScenario {
    MainPlayerDefault,
    MainPlayerShaded,
    PlaylistDefault,
    PlaylistWithSelection,
    PlaylistSingleSong,
    EqualizerDefault,
    EqualizerNonDefault,
    PreferencesDefault,
}

impl ScreenshotScenario {
    pub fn parse(name: &str) -> Result<Self, String> {
        Self::all()
            .iter()
            .copied()
            .find(|scenario| scenario.name() == name)
            .ok_or_else(|| {
                let known = Self::all()
                    .iter()
                    .map(|scenario| scenario.name())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("unknown screenshot scenario '{name}'. Known scenarios: {known}")
            })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::MainPlayerDefault => "main-player-default",
            Self::MainPlayerShaded => "main-player-shaded",
            Self::PlaylistDefault => "playlist-default",
            Self::PlaylistWithSelection => "playlist-with-selection",
            Self::PlaylistSingleSong => "playlist-single-song",
            Self::EqualizerDefault => "equalizer-default",
            Self::EqualizerNonDefault => "equalizer-non-default",
            Self::PreferencesDefault => "preferences-default",
        }
    }

    pub fn preview_args(self) -> &'static [&'static str] {
        match self {
            Self::MainPlayerDefault => &["--reset", "--screenshot-scenario", "main-player-default"],
            Self::MainPlayerShaded => &[
                "--reset",
                "--shade-main",
                "--screenshot-scenario",
                "main-player-shaded",
            ],
            Self::PlaylistDefault => &[
                "--reset",
                "--playlist",
                "--screenshot-scenario",
                "playlist-default",
            ],
            Self::PlaylistWithSelection => &[
                "--reset",
                "--playlist",
                "--screenshot-scenario",
                "playlist-with-selection",
            ],
            Self::PlaylistSingleSong => &[
                "--reset",
                "--playlist",
                "--screenshot-scenario",
                "playlist-single-song",
            ],
            Self::EqualizerDefault => &[
                "--reset",
                "--equalizer",
                "--screenshot-scenario",
                "equalizer-default",
            ],
            Self::EqualizerNonDefault => &[
                "--reset",
                "--equalizer",
                "--screenshot-scenario",
                "equalizer-non-default",
            ],
            Self::PreferencesDefault => &[
                "--reset",
                "--preferences",
                "--screenshot-scenario",
                "preferences-default",
            ],
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::MainPlayerDefault,
            Self::MainPlayerShaded,
            Self::PlaylistDefault,
            Self::PlaylistWithSelection,
            Self::PlaylistSingleSong,
            Self::EqualizerDefault,
            Self::EqualizerNonDefault,
            Self::PreferencesDefault,
        ]
    }

    pub fn apply_to_app_state(self, state: &mut AppState) {
        match self {
            Self::PlaylistSingleSong => {
                state
                    .playlist
                    .add_timed_uri("file:///music/one-song.wav", "One Song", 15_000);
                state.playlist.set_position(0);
            }
            Self::PlaylistWithSelection => {
                state.playlist.add_timed_uri(
                    "file:///music/first-demo.ogg",
                    "First selected demo",
                    123_000,
                );
                state.playlist.add_timed_uri(
                    "file:///music/current-demo.ogg",
                    "Current demo track",
                    245_000,
                );
                state.playlist.add_timed_uri(
                    "file:///music/third-demo.ogg",
                    "Third demo track",
                    367_000,
                );
                state.playlist.set_position(1);
                if let Some(entry) = state.playlist.entries_mut().get_mut(0) {
                    entry.selected = true;
                }
            }
            Self::EqualizerNonDefault => {
                state.config.equalizer_visible = true;
                state.config.equalizer_active = true;
                state.config.equalizer_auto = true;
                state.config.equalizer_preamp_pos = 25;
                state.config.equalizer_band_pos = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90];
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_scenarios_have_stable_names_and_preview_args() {
        let names: Vec<_> = ScreenshotScenario::all()
            .iter()
            .map(|scenario| scenario.name())
            .collect();
        assert!(names.contains(&"main-player-default"));
        assert_eq!(
            ScreenshotScenario::PlaylistDefault.preview_args(),
            &[
                "--reset",
                "--playlist",
                "--screenshot-scenario",
                "playlist-default"
            ]
        );
    }

    #[test]
    fn scenario_parser_rejects_unknown_names() {
        assert_eq!(
            ScreenshotScenario::parse("equalizer-non-default").unwrap(),
            ScreenshotScenario::EqualizerNonDefault
        );
        assert!(ScreenshotScenario::parse("missing").is_err());
    }

    #[test]
    fn demo_scenarios_mutate_app_state() {
        let mut state = AppState::default();
        ScreenshotScenario::PlaylistWithSelection.apply_to_app_state(&mut state);
        assert_eq!(state.playlist.len(), 3);
        assert_eq!(state.playlist.position(), Some(1));
        assert!(state.playlist.entries()[0].selected);

        let mut state = AppState::default();
        ScreenshotScenario::EqualizerNonDefault.apply_to_app_state(&mut state);
        assert_eq!(state.config.equalizer_preamp_pos, 25);
        assert_eq!(state.config.equalizer_band_pos[9], 90);
    }
}
