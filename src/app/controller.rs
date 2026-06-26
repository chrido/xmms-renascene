//! Frontend-neutral application controller.
//!
//! The controller owns application state transitions. It remains free of GTK
//! widgets, platform windows, and concrete backend objects.

use crate::app::command::AppCommand;
use crate::app::effect::AppEffect;
use crate::app_state::AppState;
use crate::player::PlaybackEvent;

#[derive(Debug, Clone)]
pub struct AppController {
    state: AppState,
}

impl AppController {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn into_state(self) -> AppState {
        self.state
    }

    pub fn handle_command(&mut self, command: AppCommand) -> Vec<AppEffect> {
        match command {
            AppCommand::SetVolume(volume) => {
                self.state.player.set_volume(volume);
                vec![
                    AppEffect::SetBackendVolume(self.state.player.volume()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(crate::app::effect::RenderTarget::All),
                ]
            }
            AppCommand::SetBalance(balance) => {
                self.state.player.set_balance(balance);
                vec![
                    AppEffect::SetBackendBalance(self.state.player.balance()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(crate::app::effect::RenderTarget::All),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub fn handle_playback_event(&mut self, _event: PlaybackEvent) -> Vec<AppEffect> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::RenderTarget;

    #[test]
    fn controller_volume_command_clamps_and_returns_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AppCommand::SetVolume(150));

        assert_eq!(controller.state().player.volume(), 100);
        assert_eq!(effects[0], AppEffect::SetBackendVolume(100));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn controller_balance_command_clamps_and_returns_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AppCommand::SetBalance(-150));

        assert_eq!(controller.state().player.balance(), -100);
        assert_eq!(effects[0], AppEffect::SetBackendBalance(-100));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }
}
