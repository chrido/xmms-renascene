//! egui main player panel/window.

use crate::app::view_model::MainPlayerViewModel;

pub fn main_player_title(view_model: &MainPlayerViewModel) -> &str {
    &view_model.title
}
