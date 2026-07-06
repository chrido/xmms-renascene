//! Frontend-neutral equalizer preset menu/action definitions.
//!
//! GTK and egui render these sections differently, but the visible labels and
//! action identifiers live here so the two frontends stay in sync.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerPresetAction {
    LoadPreset,
    LoadAutoPreset,
    LoadDefault,
    LoadZero,
    LoadFromFile,
    LoadFromWinampFile,
    ImportWinampPresets,
    SavePreset,
    SaveAutoPreset,
    SaveDefault,
    SaveToFile,
    SaveToWinampFile,
    DeletePreset,
    DeleteAutoPreset,
    Configure,
}

impl EqualizerPresetAction {
    pub const fn action_name(self) -> &'static str {
        match self {
            Self::LoadPreset => "load-preset",
            Self::LoadAutoPreset => "load-auto-preset",
            Self::LoadDefault => "load-default",
            Self::LoadZero => "load-zero",
            Self::LoadFromFile => "load-from-file",
            Self::LoadFromWinampFile => "load-from-winamp-file",
            Self::ImportWinampPresets => "import-winamp-presets",
            Self::SavePreset => "save-preset",
            Self::SaveAutoPreset => "save-auto-preset",
            Self::SaveDefault => "save-default",
            Self::SaveToFile => "save-to-file",
            Self::SaveToWinampFile => "save-to-winamp-file",
            Self::DeletePreset => "delete-preset",
            Self::DeleteAutoPreset => "delete-auto-preset",
            Self::Configure => "configure",
        }
    }

    pub const fn unsupported_egui_message(self) -> Option<&'static str> {
        match self {
            Self::LoadPreset => Some("named equalizer preset picker pending egui handler"),
            Self::LoadAutoPreset => Some("auto equalizer preset picker pending egui handler"),
            Self::SavePreset => Some("save named equalizer preset pending egui handler"),
            Self::SaveAutoPreset => Some("save auto equalizer preset pending egui handler"),
            Self::SaveDefault => Some("save default equalizer preset pending egui handler"),
            Self::DeletePreset => Some("delete equalizer preset pending egui handler"),
            Self::DeleteAutoPreset => Some("delete auto equalizer preset pending egui handler"),
            Self::Configure => Some("configure equalizer preset paths pending egui handler"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EqualizerPresetMenuItem {
    pub label: &'static str,
    pub action: EqualizerPresetAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EqualizerPresetMenuSection {
    pub label: &'static str,
    pub items: &'static [EqualizerPresetMenuItem],
}

pub const EQUALIZER_LOAD_PRESET_ITEMS: &[EqualizerPresetMenuItem] = &[
    EqualizerPresetMenuItem {
        label: "Preset",
        action: EqualizerPresetAction::LoadPreset,
    },
    EqualizerPresetMenuItem {
        label: "Auto-load preset",
        action: EqualizerPresetAction::LoadAutoPreset,
    },
    EqualizerPresetMenuItem {
        label: "Default",
        action: EqualizerPresetAction::LoadDefault,
    },
    EqualizerPresetMenuItem {
        label: "Zero",
        action: EqualizerPresetAction::LoadZero,
    },
    EqualizerPresetMenuItem {
        label: "From file",
        action: EqualizerPresetAction::LoadFromFile,
    },
    EqualizerPresetMenuItem {
        label: "From WinAMP EQF file",
        action: EqualizerPresetAction::LoadFromWinampFile,
    },
];

pub const EQUALIZER_IMPORT_PRESET_ITEMS: &[EqualizerPresetMenuItem] = &[EqualizerPresetMenuItem {
    label: "WinAMP Presets",
    action: EqualizerPresetAction::ImportWinampPresets,
}];

pub const EQUALIZER_SAVE_PRESET_ITEMS: &[EqualizerPresetMenuItem] = &[
    EqualizerPresetMenuItem {
        label: "Preset",
        action: EqualizerPresetAction::SavePreset,
    },
    EqualizerPresetMenuItem {
        label: "Auto-load preset",
        action: EqualizerPresetAction::SaveAutoPreset,
    },
    EqualizerPresetMenuItem {
        label: "Default",
        action: EqualizerPresetAction::SaveDefault,
    },
    EqualizerPresetMenuItem {
        label: "To file",
        action: EqualizerPresetAction::SaveToFile,
    },
    EqualizerPresetMenuItem {
        label: "To WinAMP EQF file",
        action: EqualizerPresetAction::SaveToWinampFile,
    },
];

pub const EQUALIZER_DELETE_PRESET_ITEMS: &[EqualizerPresetMenuItem] = &[
    EqualizerPresetMenuItem {
        label: "Preset",
        action: EqualizerPresetAction::DeletePreset,
    },
    EqualizerPresetMenuItem {
        label: "Auto-load preset",
        action: EqualizerPresetAction::DeleteAutoPreset,
    },
];

pub const EQUALIZER_PRESET_MENU_SECTIONS: &[EqualizerPresetMenuSection] = &[
    EqualizerPresetMenuSection {
        label: "Load",
        items: EQUALIZER_LOAD_PRESET_ITEMS,
    },
    EqualizerPresetMenuSection {
        label: "Import",
        items: EQUALIZER_IMPORT_PRESET_ITEMS,
    },
    EqualizerPresetMenuSection {
        label: "Save",
        items: EQUALIZER_SAVE_PRESET_ITEMS,
    },
    EqualizerPresetMenuSection {
        label: "Delete",
        items: EQUALIZER_DELETE_PRESET_ITEMS,
    },
];

pub const EQUALIZER_CONFIGURE_PRESET_ITEM: EqualizerPresetMenuItem = EqualizerPresetMenuItem {
    label: "Configure Equalizer",
    action: EqualizerPresetAction::Configure,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equalizer_preset_action_names_are_stable() {
        assert_eq!(EqualizerPresetAction::LoadPreset.action_name(), "load-preset");
        assert_eq!(EqualizerPresetAction::LoadDefault.action_name(), "load-default");
        assert_eq!(EqualizerPresetAction::SaveToFile.action_name(), "save-to-file");
        assert_eq!(EqualizerPresetAction::Configure.action_name(), "configure");
    }

    #[test]
    fn equalizer_preset_sections_cover_expected_labels() {
        let labels: Vec<_> = EQUALIZER_PRESET_MENU_SECTIONS
            .iter()
            .map(|section| section.label)
            .collect();
        assert_eq!(labels, vec!["Load", "Import", "Save", "Delete"]);
        assert_eq!(
            EQUALIZER_LOAD_PRESET_ITEMS
                .iter()
                .map(|item| item.label)
                .collect::<Vec<_>>(),
            vec![
                "Preset",
                "Auto-load preset",
                "Default",
                "Zero",
                "From file",
                "From WinAMP EQF file",
            ]
        );
        assert_eq!(EQUALIZER_CONFIGURE_PRESET_ITEM.label, "Configure Equalizer");
    }

    #[test]
    fn egui_unsupported_actions_are_explicitly_documented() {
        assert_eq!(
            EqualizerPresetAction::LoadPreset.unsupported_egui_message(),
            Some("named equalizer preset picker pending egui handler")
        );
        assert_eq!(EqualizerPresetAction::LoadDefault.unsupported_egui_message(), None);
    }
}
