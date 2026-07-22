//! Frontend-neutral panel placement/state helpers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Equalizer,
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelState {
    Hidden,
    Docked { shaded: bool },
    Detached { shaded: bool },
}

impl PanelState {
    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn from_flags(visible: bool, detached: bool, shaded: bool) -> Self {
        match (visible, detached) {
            (false, _) => Self::Hidden,
            (true, true) => Self::Detached { shaded },
            (true, false) => Self::Docked { shaded },
        }
    }

    pub fn is_detached_visible(self) -> bool {
        matches!(self, PanelState::Detached { .. })
    }

    pub fn is_docked_visible(self) -> bool {
        matches!(self, PanelState::Docked { .. })
    }

    pub fn shaded(self) -> bool {
        match self {
            PanelState::Hidden => false,
            PanelState::Docked { shaded } | PanelState::Detached { shaded } => shaded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelVisibility {
    pub equalizer: bool,
    pub playlist: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_state_maps_visibility_detach_and_shade_flags() {
        assert_eq!(
            PanelState::from_flags(false, false, false),
            PanelState::Hidden
        );
        assert_eq!(
            PanelState::from_flags(true, false, true),
            PanelState::Docked { shaded: true }
        );
        assert_eq!(
            PanelState::from_flags(true, true, false),
            PanelState::Detached { shaded: false }
        );
    }
}
