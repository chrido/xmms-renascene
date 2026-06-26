//! egui equalizer panel/window.

use crate::app::command::EqualizerCommand;
use crate::app::view_model::{equalizer_view_model, EqualizerViewModel};

use super::app::EguiFrontendState;

pub fn equalizer_band_count(view_model: &EqualizerViewModel) -> usize {
    view_model.band_positions.len()
}

pub fn show_equalizer(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = equalizer_view_model(app.controller().state());
    ui.heading("Equalizer");

    let mut active = view_model.active;
    if ui.checkbox(&mut active, "Active").changed() {
        app.dispatch(EqualizerCommand::SetActive(active));
    }
    let mut auto = view_model.auto;
    if ui.checkbox(&mut auto, "Auto").changed() {
        app.dispatch(EqualizerCommand::SetAuto(auto));
    }

    let mut preamp = view_model.preamp_position;
    if ui.add(egui::Slider::new(&mut preamp, 0..=100).text("preamp")).changed() {
        app.dispatch(EqualizerCommand::SetPreamp(preamp));
    }

    ui.horizontal_wrapped(|ui| {
        for (band, original) in view_model.band_positions.into_iter().enumerate() {
            let mut position = original;
            if ui
                .add(egui::Slider::new(&mut position, 0..=100).vertical().text(format!("Band {}", band + 1)))
                .changed()
            {
                app.dispatch(EqualizerCommand::SetBand { band, position });
            }
        }
    });
}

pub fn equalizer_band_command(band: usize, position: i32) -> EqualizerCommand {
    EqualizerCommand::SetBand { band, position }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equalizer_slider_translation_uses_equalizer_command_domain() {
        assert_eq!(
            equalizer_band_command(4, 75),
            EqualizerCommand::SetBand {
                band: 4,
                position: 75,
            }
        );
    }
}
