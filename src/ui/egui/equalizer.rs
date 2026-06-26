//! egui equalizer panel/window.

use crate::app::view_model::EqualizerViewModel;

pub fn equalizer_band_count(view_model: &EqualizerViewModel) -> usize {
    view_model.band_positions.len()
}
