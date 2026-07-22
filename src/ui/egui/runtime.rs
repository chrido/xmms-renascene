//! egui-side interpreter for frontend-neutral application effects.

use std::time::Duration;

use crate::app::effect::RenderTarget;
use crate::app::store::StateChangeSet;

use super::effect_executor::UiEffect;

const ANDROID_LAYOUT_REPAINT_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Default)]
pub struct EguiRuntime {
    pub pending_messages: Vec<String>,
    repaint: RepaintPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AndroidLayoutRepaint {
    AwaitingReadiness,
    Stabilizing,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RepaintSchedule {
    pub immediate: bool,
    pub after: Option<Duration>,
}

#[derive(Debug, Default)]
struct RepaintPolicy {
    state_change_pending: bool,
    android_layout: Option<AndroidLayoutRepaint>,
    dirty_targets: Vec<RenderTarget>,
}

impl RepaintPolicy {
    fn queue_render(&mut self, target: RenderTarget) {
        self.state_change_pending = true;
        if !self.dirty_targets.contains(&target) {
            self.dirty_targets.push(target);
        }
    }

    fn request_android_layout(&mut self, repaint: AndroidLayoutRepaint) {
        self.android_layout = match (self.android_layout, repaint) {
            (Some(AndroidLayoutRepaint::AwaitingReadiness), _)
            | (_, AndroidLayoutRepaint::AwaitingReadiness) => {
                Some(AndroidLayoutRepaint::AwaitingReadiness)
            }
            _ => Some(AndroidLayoutRepaint::Stabilizing),
        };
    }

    fn take_schedule(&mut self) -> RepaintSchedule {
        let layout = self.android_layout.take();
        let schedule = RepaintSchedule {
            immediate: self.state_change_pending
                || layout == Some(AndroidLayoutRepaint::AwaitingReadiness),
            after: layout.map(|_| ANDROID_LAYOUT_REPAINT_INTERVAL),
        };
        self.state_change_pending = false;
        self.dirty_targets.clear();
        schedule
    }
}

impl EguiRuntime {
    #[cfg(test)]
    pub(crate) fn apply_effects(&mut self, effects: impl IntoIterator<Item = UiEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    pub(crate) fn apply_effect(&mut self, effect: UiEffect) {
        match effect {
            UiEffect::QueueRender(target) => self.repaint.queue_render(target),
            UiEffect::ShowError(message) | UiEffect::ShowMessage(message) => {
                self.pending_messages.push(message);
            }
            UiEffect::OpenFileDialog(request) => {
                self.pending_messages.push(format!(
                    "file dialog effect pending egui handler: {request:?}"
                ));
            }
            UiEffect::OpenPath(path) => {
                self.pending_messages.push(format!(
                    "open path effect pending egui handler: {}",
                    path.display()
                ));
            }
            UiEffect::OpenFileInfoDialog => self
                .pending_messages
                .push("file info dialog pending egui handler".to_string()),
            UiEffect::OpenPreferences => self
                .pending_messages
                .push("preferences effect pending egui handler".to_string()),
            UiEffect::OpenSkinBrowser => self
                .pending_messages
                .push("skin browser effect pending egui handler".to_string()),
            UiEffect::OpenSkinEditor => self
                .pending_messages
                .push("skin editor is GTK-only".to_string()),
        }
    }

    pub fn invalidate_changes(&mut self, changes: StateChangeSet) {
        if changes.contains(StateChangeSet::RENDER_ALL) {
            self.repaint.queue_render(RenderTarget::All);
            return;
        }
        if changes.intersects(StateChangeSet::RENDER_MAIN) {
            self.repaint.queue_render(RenderTarget::Main);
        }
        if changes.intersects(StateChangeSet::RENDER_PLAYLIST) {
            self.repaint.queue_render(RenderTarget::Playlist);
        }
        if changes.intersects(StateChangeSet::RENDER_EQUALIZER) {
            self.repaint.queue_render(RenderTarget::Equalizer);
        }
    }

    pub(crate) fn request_android_layout_repaint(&mut self, repaint: AndroidLayoutRepaint) {
        self.repaint.request_android_layout(repaint);
    }

    pub(crate) fn take_repaint_schedule(&mut self) -> RepaintSchedule {
        self.repaint.take_schedule()
    }

    #[cfg(test)]
    pub(crate) fn repaint_requested(&self) -> bool {
        self.repaint.state_change_pending || self.repaint.android_layout.is_some()
    }

    #[cfg(test)]
    pub(crate) fn dirty_targets(&self) -> &[RenderTarget] {
        &self.repaint.dirty_targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egui_runtime_tracks_render_and_message_effects() {
        let mut runtime = EguiRuntime::default();

        runtime.apply_effects([
            UiEffect::QueueRender(RenderTarget::Main),
            UiEffect::QueueRender(RenderTarget::Main),
            UiEffect::ShowMessage("hello".to_string()),
        ]);

        assert!(runtime.repaint_requested());
        assert_eq!(runtime.dirty_targets(), &[RenderTarget::Main]);
        assert_eq!(runtime.pending_messages, vec!["hello"]);
    }

    #[test]
    fn state_changes_share_the_render_invalidation_policy() {
        let mut runtime = EguiRuntime::default();

        runtime.invalidate_changes(StateChangeSet::RENDER_MAIN | StateChangeSet::RENDER_PLAYLIST);

        assert_eq!(
            runtime.dirty_targets(),
            &[RenderTarget::Main, RenderTarget::Playlist]
        );
    }

    #[test]
    fn state_change_and_layout_repaints_remain_distinct() {
        let mut runtime = EguiRuntime::default();

        runtime.invalidate_changes(StateChangeSet::RENDER_MAIN);
        assert_eq!(
            runtime.take_repaint_schedule(),
            RepaintSchedule {
                immediate: true,
                after: None,
            }
        );

        runtime.request_android_layout_repaint(AndroidLayoutRepaint::Stabilizing);
        assert_eq!(
            runtime.take_repaint_schedule(),
            RepaintSchedule {
                immediate: false,
                after: Some(ANDROID_LAYOUT_REPAINT_INTERVAL),
            }
        );

        runtime.request_android_layout_repaint(AndroidLayoutRepaint::AwaitingReadiness);
        assert_eq!(
            runtime.take_repaint_schedule(),
            RepaintSchedule {
                immediate: true,
                after: Some(ANDROID_LAYOUT_REPAINT_INTERVAL),
            }
        );
    }

    #[test]
    fn awaiting_layout_readiness_takes_priority_over_stabilizing() {
        let mut runtime = EguiRuntime::default();

        runtime.request_android_layout_repaint(AndroidLayoutRepaint::AwaitingReadiness);
        runtime.request_android_layout_repaint(AndroidLayoutRepaint::Stabilizing);

        assert_eq!(
            runtime.take_repaint_schedule(),
            RepaintSchedule {
                immediate: true,
                after: Some(ANDROID_LAYOUT_REPAINT_INTERVAL),
            }
        );
    }
}
