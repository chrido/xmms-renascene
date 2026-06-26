//! egui playlist panel/window.

use crate::app::view_model::PlaylistViewModel;

pub fn playlist_row_count(view_model: &PlaylistViewModel) -> usize {
    view_model.rows.len()
}
