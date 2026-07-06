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

const fn item(label: &'static str, action: EqualizerPresetAction) -> EqualizerPresetMenuItem {
    EqualizerPresetMenuItem { label, action }
}

const fn section(
    label: &'static str,
    items: &'static [EqualizerPresetMenuItem],
) -> EqualizerPresetMenuSection {
    EqualizerPresetMenuSection { label, items }
}

pub const EQUALIZER_LOAD_PRESET_ITEMS: &[EqualizerPresetMenuItem] = &[
    item("Preset", EqualizerPresetAction::LoadPreset),
    item("Auto-load preset", EqualizerPresetAction::LoadAutoPreset),
    item("Default", EqualizerPresetAction::LoadDefault),
    item("Zero", EqualizerPresetAction::LoadZero),
    item("From file", EqualizerPresetAction::LoadFromFile),
    item("From WinAMP EQF file", EqualizerPresetAction::LoadFromWinampFile),
];

pub const EQUALIZER_IMPORT_PRESET_ITEMS: &[EqualizerPresetMenuItem] =
    &[item("WinAMP Presets", EqualizerPresetAction::ImportWinampPresets)];

pub const EQUALIZER_SAVE_PRESET_ITEMS: &[EqualizerPresetMenuItem] = &[
    item("Preset", EqualizerPresetAction::SavePreset),
    item("Auto-load preset", EqualizerPresetAction::SaveAutoPreset),
    item("Default", EqualizerPresetAction::SaveDefault),
    item("To file", EqualizerPresetAction::SaveToFile),
    item("To WinAMP EQF file", EqualizerPresetAction::SaveToWinampFile),
];

pub const EQUALIZER_DELETE_PRESET_ITEMS: &[EqualizerPresetMenuItem] = &[
    item("Preset", EqualizerPresetAction::DeletePreset),
    item("Auto-load preset", EqualizerPresetAction::DeleteAutoPreset),
];

pub const EQUALIZER_PRESET_MENU_SECTIONS: &[EqualizerPresetMenuSection] = &[
    section("Load", EQUALIZER_LOAD_PRESET_ITEMS),
    section("Import", EQUALIZER_IMPORT_PRESET_ITEMS),
    section("Save", EQUALIZER_SAVE_PRESET_ITEMS),
    section("Delete", EQUALIZER_DELETE_PRESET_ITEMS),
];

pub const EQUALIZER_CONFIGURE_PRESET_ITEM: EqualizerPresetMenuItem =
    item("Configure Equalizer", EqualizerPresetAction::Configure);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equalizer_preset_action_names_are_stable() {
        for (action, name) in [
            (EqualizerPresetAction::LoadPreset, "load-preset"),
            (EqualizerPresetAction::LoadDefault, "load-default"),
            (EqualizerPresetAction::SaveToFile, "save-to-file"),
            (EqualizerPresetAction::Configure, "configure"),
        ] {
            assert_eq!(action.action_name(), name);
        }
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
            [
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
    fn equalizer_preset_menu_items_all_have_stable_action_names() {
        let mut names: Vec<_> = EQUALIZER_PRESET_MENU_SECTIONS
            .iter()
            .flat_map(|section| {
                assert!(!section.items.is_empty(), "{} section is empty", section.label);
                section.items.iter().map(|item| {
                    assert!(!item.label.is_empty());
                    item.action.action_name()
                })
            })
            .chain([EQUALIZER_CONFIGURE_PRESET_ITEM.action.action_name()])
            .collect();
        let expected_count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), expected_count);
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
