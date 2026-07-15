//! Frontend-neutral equalizer preset menu/action definitions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerPresetAction {
    Load,
    Save,
}

impl EqualizerPresetAction {
    pub const fn action_name(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Save => "save",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EqualizerPresetMenuItem {
    pub label: &'static str,
    pub action: EqualizerPresetAction,
}

pub const EQUALIZER_PRESET_FILE_ITEMS: &[EqualizerPresetMenuItem] = &[
    EqualizerPresetMenuItem {
        label: "Load",
        action: EqualizerPresetAction::Load,
    },
    EqualizerPresetMenuItem {
        label: "Save",
        action: EqualizerPresetAction::Save,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equalizer_preset_menu_only_exposes_eqf_file_actions() {
        assert_eq!(
            EQUALIZER_PRESET_FILE_ITEMS
                .iter()
                .map(|item| (item.label, item.action.action_name()))
                .collect::<Vec<_>>(),
            vec![("Load", "load"), ("Save", "save")]
        );
    }
}
