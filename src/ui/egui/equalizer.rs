//! egui equalizer panel/window.

use crate::app::command::EqualizerCommand;
use crate::app::view_model::{
    balance_to_eq_shaded_position, equalizer_view_model, eq_slider_pixel_to_position,
    volume_to_eq_shaded_position, EqualizerViewModel,
};
use crate::render::{
    equalizer_slider_layout, EqualizerControl, EqualizerRenderState, EqualizerSlider,
    EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH,
};
use crate::skin::layout::{equalizer_control_rect, SkinRect};

use super::app::EguiFrontendState;
use super::skin_texture::{render_equalizer_color_image, upload_color_image};

pub fn equalizer_band_count(view_model: &EqualizerViewModel) -> usize {
    view_model.band_positions.len()
}

pub fn show_equalizer(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = equalizer_view_model(app.controller().state());
    if !view_model.visible || view_model.detached {
        return;
    }
    let render_state = equalizer_render_state(app, &view_model);
    let Ok(image) = render_equalizer_color_image(&app.active_skin, &render_state) else {
        ui.label("failed to render skinned equalizer");
        return;
    };
    let texture = upload_color_image(ui.ctx(), "xmms-equalizer", image);
    let base_height = if view_model.shaded {
        crate::render::MAIN_TITLEBAR_HEIGHT
    } else {
        EQUALIZER_WINDOW_HEIGHT
    };
    let size = egui::vec2(
        EQUALIZER_WINDOW_WIDTH as f32 * app.scale_factor,
        base_height as f32 * app.scale_factor,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    add_equalizer_hit_regions(ui, app, rect, &view_model);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn equalizer_render_state(
    app: &EguiFrontendState,
    view_model: &EqualizerViewModel,
) -> EqualizerRenderState {
    EqualizerRenderState {
        focused: true,
        shaded: view_model.shaded,
        active: view_model.active,
        automatic: view_model.auto,
        pressed_control: app.equalizer_pressed_control,
        pressed_slider: app.equalizer_pressed_slider,
        preamp_position: view_model.preamp_position,
        band_positions: view_model.band_positions,
        volume_position: volume_to_eq_shaded_position(app.controller().state().player.volume()),
        balance_position: balance_to_eq_shaded_position(app.controller().state().player.balance()),
    }
}

fn add_equalizer_hit_regions(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &EqualizerViewModel,
) {
    app.equalizer_pressed_control = None;
    app.equalizer_pressed_slider = None;
    if view_model.shaded {
        add_equalizer_slider_hit(ui, app, base_rect, EqualizerSlider::ShadedVolume);
        add_equalizer_slider_hit(ui, app, base_rect, EqualizerSlider::ShadedBalance);
        return;
    }

    for control in [EqualizerControl::On, EqualizerControl::Auto, EqualizerControl::Presets] {
        let rect = scale_skin_rect(base_rect, equalizer_control_rect(control), app.scale_factor);
        let response = ui.interact(
            rect,
            ui.id().with(("eq-control", control as u8)),
            egui::Sense::click(),
        );
        if response.is_pointer_button_down_on() {
            app.equalizer_pressed_control = Some(control);
            ui.ctx().request_repaint();
        }
        if response.clicked() {
            dispatch_equalizer_control(app, control);
        }
    }

    add_equalizer_slider_hit(ui, app, base_rect, EqualizerSlider::Preamp);
    for band in 0..crate::audio_model::EQUALIZER_BANDS {
        add_equalizer_slider_hit(ui, app, base_rect, EqualizerSlider::Band(band));
    }
}

fn add_equalizer_slider_hit(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
    slider: EqualizerSlider,
) {
    let layout = equalizer_slider_layout(slider);
    let rect = scale_skin_rect(base_rect, layout.rect, app.scale_factor);
    let response = ui.interact(
        rect,
        ui.id().with(("eq-slider", equalizer_slider_id(slider))),
        egui::Sense::click_and_drag(),
    );
    if response.is_pointer_button_down_on() || response.dragged() {
        app.equalizer_pressed_slider = Some(slider);
        ui.ctx().request_repaint();
    }
    if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
        let pointer = response.interact_pointer_pos().unwrap();
        dispatch_equalizer_slider(app, slider, pointer, rect);
    }
}

fn dispatch_equalizer_control(app: &mut EguiFrontendState, control: EqualizerControl) {
    match control {
        EqualizerControl::On => app.dispatch(EqualizerCommand::ToggleActive),
        EqualizerControl::Auto => app.dispatch(EqualizerCommand::ToggleAuto),
        EqualizerControl::Presets => app
            .runtime
            .pending_messages
            .push("equalizer preset menu pending egui handler".to_string()),
    }
}

fn dispatch_equalizer_slider(
    app: &mut EguiFrontendState,
    slider: EqualizerSlider,
    pointer: egui::Pos2,
    rect: egui::Rect,
) {
    match slider {
        EqualizerSlider::Preamp => {
            let pixel = ((pointer.y - rect.top()) / app.scale_factor).round() as i32;
            app.dispatch(EqualizerCommand::SetPreamp(eq_slider_pixel_to_position(pixel)));
        }
        EqualizerSlider::Band(band) => {
            let pixel = ((pointer.y - rect.top()) / app.scale_factor).round() as i32;
            app.dispatch(EqualizerCommand::SetBand {
                band,
                position: eq_slider_pixel_to_position(pixel),
            });
        }
        EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => {
            app.runtime
                .pending_messages
                .push("shaded equalizer slider pending egui handler".to_string());
        }
    }
}

fn equalizer_slider_id(slider: EqualizerSlider) -> u16 {
    match slider {
        EqualizerSlider::Preamp => 0,
        EqualizerSlider::Band(band) => 1 + band as u16,
        EqualizerSlider::ShadedVolume => 100,
        EqualizerSlider::ShadedBalance => 101,
    }
}

fn scale_skin_rect(base: egui::Rect, rect: SkinRect, scale: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(
            base.left() + rect.x as f32 * scale,
            base.top() + rect.y as f32 * scale,
        ),
        egui::vec2(rect.width as f32 * scale, rect.height as f32 * scale),
    )
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

    #[test]
    fn equalizer_controls_dispatch_to_app_state() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();

        dispatch_equalizer_control(&mut app, EqualizerControl::On);
        assert!(!app.controller().state().config.equalizer_active);

        dispatch_equalizer_control(&mut app, EqualizerControl::Auto);
        assert!(app.controller().state().config.equalizer_auto);

        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(14.0, 63.0));
        dispatch_equalizer_slider(
            &mut app,
            EqualizerSlider::Band(2),
            egui::pos2(0.0, 0.0),
            rect,
        );
        assert_eq!(app.controller().state().config.equalizer_band_pos[2], 0);
    }
}
