//! egui input translation helpers.

use crate::app::command::{AppCommand, PlayerCommand};

pub fn play_command() -> AppCommand {
    PlayerCommand::Play.into()
}
