//! Frontend-neutral screenshot scenarios shared by GTK, egui, tests, and tools.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotScenario {
    MainPlayerDefault,
    MainPlayerShaded,
    PlaylistDefault,
    PlaylistWithSelection,
    EqualizerDefault,
    EqualizerNonDefault,
    PreferencesDefault,
}

impl ScreenshotScenario {
    pub fn name(self) -> &'static str {
        match self {
            Self::MainPlayerDefault => "main-player-default",
            Self::MainPlayerShaded => "main-player-shaded",
            Self::PlaylistDefault => "playlist-default",
            Self::PlaylistWithSelection => "playlist-with-selection",
            Self::EqualizerDefault => "equalizer-default",
            Self::EqualizerNonDefault => "equalizer-non-default",
            Self::PreferencesDefault => "preferences-default",
        }
    }

    pub fn preview_args(self) -> &'static [&'static str] {
        match self {
            Self::MainPlayerDefault => &["--reset"],
            Self::MainPlayerShaded => &["--reset", "--shade-main"],
            Self::PlaylistDefault | Self::PlaylistWithSelection => &["--reset", "--playlist"],
            Self::EqualizerDefault | Self::EqualizerNonDefault => &["--reset", "--equalizer"],
            Self::PreferencesDefault => &["--reset", "--preferences"],
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::MainPlayerDefault,
            Self::MainPlayerShaded,
            Self::PlaylistDefault,
            Self::PlaylistWithSelection,
            Self::EqualizerDefault,
            Self::EqualizerNonDefault,
            Self::PreferencesDefault,
        ]
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
        assert_eq!(ScreenshotScenario::PlaylistDefault.preview_args(), &["--reset", "--playlist"]);
    }
}
