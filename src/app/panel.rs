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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelPlacement {
    pub(crate) visible: bool,
    pub(crate) detached: bool,
    pub(crate) shaded: bool,
    pub(crate) focused: bool,
    pub(crate) dragging_title: bool,
}

#[allow(dead_code)]
impl PanelPlacement {
    pub(crate) fn from_config(visible: bool, detached: bool, shaded: bool) -> Self {
        Self {
            visible,
            detached,
            shaded,
            focused: false,
            dragging_title: false,
        }
    }

    pub(crate) fn state(self) -> PanelState {
        match (self.visible, self.detached) {
            (false, _) => PanelState::Hidden,
            (true, true) => PanelState::Detached {
                shaded: self.shaded,
            },
            (true, false) => PanelState::Docked {
                shaded: self.shaded,
            },
        }
    }

    pub(crate) fn focused(self) -> bool {
        self.focused || self.dragging_title
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
            PanelPlacement::from_config(false, false, false).state(),
            PanelState::Hidden
        );
        assert_eq!(
            PanelPlacement::from_config(true, false, true).state(),
            PanelState::Docked { shaded: true }
        );
        assert_eq!(
            PanelPlacement::from_config(true, true, false).state(),
            PanelState::Detached { shaded: false }
        );
    }
}
