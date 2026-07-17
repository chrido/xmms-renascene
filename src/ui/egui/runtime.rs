//! egui-side interpreter for frontend-neutral application effects.

use crate::app::effect::{AppEffect, RenderTarget};

#[derive(Debug, Default)]
pub struct EguiRuntime {
    pub pending_messages: Vec<String>,
    pub repaint_requested: bool,
    pub dirty_targets: Vec<RenderTarget>,
}

impl EguiRuntime {
    pub fn apply_effects(&mut self, effects: impl IntoIterator<Item = AppEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    pub fn apply_effect(&mut self, effect: AppEffect) {
        match effect {
            AppEffect::QueueRender(target) => {
                self.repaint_requested = true;
                self.dirty_targets.push(target);
            }
            AppEffect::ShowError(message) | AppEffect::ShowMessage(message) => {
                self.pending_messages.push(message);
            }
            AppEffect::OpenFileDialog(request) => {
                self.pending_messages.push(format!(
                    "file dialog effect pending egui handler: {request:?}"
                ));
            }
            AppEffect::OpenPath(path) => {
                self.pending_messages.push(format!(
                    "open path effect pending egui handler: {}",
                    path.display()
                ));
            }
            AppEffect::OpenFileInfoDialog => self
                .pending_messages
                .push("file info dialog pending egui handler".to_string()),
            AppEffect::OpenPreferences => self
                .pending_messages
                .push("preferences effect pending egui handler".to_string()),
            AppEffect::OpenSkinBrowser => self
                .pending_messages
                .push("skin browser effect pending egui handler".to_string()),
            AppEffect::OpenSkinEditor => self
                .pending_messages
                .push("skin editor is GTK-only".to_string()),
            AppEffect::StartPlayback
            | AppEffect::StartPlaybackFromCurrent
            | AppEffect::StartPlaybackUri { .. }
            | AppEffect::ResumePlayback
            | AppEffect::PausePlayback
            | AppEffect::StopPlayback
            | AppEffect::BeginStopFade { .. }
            | AppEffect::SeekPlayback(_)
            | AppEffect::SetOutputVolume(_)
            | AppEffect::SetBackendVolume(_)
            | AppEffect::SetBackendBalance(_)
            | AppEffect::SetBackendEqualizer
            | AppEffect::SaveConfig => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egui_runtime_tracks_render_and_message_effects() {
        let mut runtime = EguiRuntime::default();

        runtime.apply_effects([
            AppEffect::QueueRender(RenderTarget::Main),
            AppEffect::ShowMessage("hello".to_string()),
        ]);

        assert!(runtime.repaint_requested);
        assert_eq!(runtime.dirty_targets, vec![RenderTarget::Main]);
        assert_eq!(runtime.pending_messages, vec!["hello"]);
    }
}
