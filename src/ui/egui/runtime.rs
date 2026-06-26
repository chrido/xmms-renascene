//! egui-side interpreter for frontend-neutral application effects.

use crate::app::effect::AppEffect;

#[derive(Debug, Default)]
pub struct EguiRuntime {
    pub pending_messages: Vec<String>,
}

impl EguiRuntime {
    pub fn apply_effects(&mut self, effects: impl IntoIterator<Item = AppEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    pub fn apply_effect(&mut self, effect: AppEffect) {
        match effect {
            AppEffect::ShowError(message) | AppEffect::ShowMessage(message) => {
                self.pending_messages.push(message);
            }
            _ => {}
        }
    }
}
