//! Frontend/platform service boundary documentation.
//!
//! The app layer prefers explicit `AppEffect` values over injecting many
//! service traits into the controller. This module records the platform surface
//! that each frontend must eventually provide.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendServiceKind {
    FileDialog,
    DirectoryDialog,
    MessageDialog,
    Clipboard,
    Timer,
    StoragePath,
    SkinImportExportPath,
    ExternalUrl,
    WindowAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontendServiceBoundary {
    pub kind: FrontendServiceKind,
    pub description: &'static str,
}

pub const FRONTEND_SERVICE_BOUNDARIES: &[FrontendServiceBoundary] = &[
    FrontendServiceBoundary {
        kind: FrontendServiceKind::FileDialog,
        description: "open/save audio, playlist, equalizer preset, and skin archive files",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::DirectoryDialog,
        description: "choose audio directories and skin directories",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::MessageDialog,
        description: "present errors, confirmations, and informational messages",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::Clipboard,
        description: "copy/paste text or paths where supported by the frontend",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::Timer,
        description: "schedule UI ticks, delayed playback commands, and animation updates",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::StoragePath,
        description: "resolve config, cache, playlist, podcast, and session storage paths",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::SkinImportExportPath,
        description: "resolve user skin import/export destinations",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::ExternalUrl,
        description: "open remote URLs or platform web views",
    },
    FrontendServiceBoundary {
        kind: FrontendServiceKind::WindowAction,
        description: "present, hide, resize, dock, shade, and focus frontend windows or views",
    },
];

pub fn service_boundaries() -> &'static [FrontendServiceBoundary] {
    FRONTEND_SERVICE_BOUNDARIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_service_boundary_lists_mobile_relevant_services() {
        let kinds: Vec<_> = service_boundaries()
            .iter()
            .map(|boundary| boundary.kind)
            .collect();

        assert!(kinds.contains(&FrontendServiceKind::FileDialog));
        assert!(kinds.contains(&FrontendServiceKind::DirectoryDialog));
        assert!(kinds.contains(&FrontendServiceKind::MessageDialog));
        assert!(kinds.contains(&FrontendServiceKind::Timer));
        assert!(kinds.contains(&FrontendServiceKind::StoragePath));
        assert!(kinds.contains(&FrontendServiceKind::WindowAction));
    }
}
