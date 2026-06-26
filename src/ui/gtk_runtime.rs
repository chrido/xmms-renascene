#![allow(dead_code)]

use gtk::prelude::*;

use crate::app::effect::{AppEffect, RenderTarget};

/// GTK-side interpreter for frontend-neutral application effects.
///
/// This is intentionally small at first: migrated handlers can route effects
/// here while non-migrated GTK code continues to perform its existing work.
pub(crate) struct GtkEffectInterpreter<'a> {
    main_area: Option<&'a gtk::DrawingArea>,
    playlist_area: Option<&'a gtk::DrawingArea>,
    equalizer_area: Option<&'a gtk::DrawingArea>,
}

impl<'a> GtkEffectInterpreter<'a> {
    pub(crate) fn new() -> Self {
        Self {
            main_area: None,
            playlist_area: None,
            equalizer_area: None,
        }
    }

    pub(crate) fn with_render_areas(
        main_area: &'a gtk::DrawingArea,
        playlist_area: Option<&'a gtk::DrawingArea>,
        equalizer_area: Option<&'a gtk::DrawingArea>,
    ) -> Self {
        Self {
            main_area: Some(main_area),
            playlist_area,
            equalizer_area,
        }
    }

    pub(crate) fn apply_effects(&self, effects: impl IntoIterator<Item = AppEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    pub(crate) fn apply_effect(&self, effect: AppEffect) {
        match effect {
            AppEffect::QueueRender(target) => self.queue_render(target),
            AppEffect::ShowError(message) => eprintln!("xmms-rs: {message}"),
            AppEffect::ShowMessage(message) => eprintln!("xmms-rs: {message}"),
            AppEffect::OpenFileDialog(request) => {
                eprintln!("xmms-rs: file dialog effect pending GTK handler: {request:?}");
            }
            AppEffect::OpenPath(path) => {
                eprintln!("xmms-rs: open path effect pending GTK handler: {}", path.display());
            }
            AppEffect::OpenFileInfoDialog
            | AppEffect::OpenPreferences
            | AppEffect::OpenSkinBrowser
            | AppEffect::OpenSkinEditor
            | AppEffect::StartPlayback
            | AppEffect::StartPlaybackFromCurrent
            | AppEffect::StartPlaybackUri { .. }
            | AppEffect::ResumePlayback
            | AppEffect::PausePlayback
            | AppEffect::StopPlayback
            | AppEffect::SeekPlayback(_)
            | AppEffect::SetBackendVolume(_)
            | AppEffect::SetBackendBalance(_)
            | AppEffect::SetBackendEqualizer
            | AppEffect::SaveConfig => {}
        }
    }

    fn queue_render(&self, target: RenderTarget) {
        match target {
            RenderTarget::Main => queue(self.main_area),
            RenderTarget::Playlist => queue(self.playlist_area.or(self.main_area)),
            RenderTarget::Equalizer => queue(self.equalizer_area.or(self.main_area)),
            RenderTarget::DockedPanels | RenderTarget::All => {
                queue(self.main_area);
                queue(self.playlist_area);
                queue(self.equalizer_area);
            }
        }
    }
}

fn queue(area: Option<&gtk::DrawingArea>) {
    if let Some(area) = area {
        area.queue_draw();
    }
}
