//! egui equalizer panel/window.

use crate::app::command::{AudioCommand, EqualizerCommand, PanelCommand};
use crate::app::effect::{AppEffect, FileDialogRequest};
use crate::app::equalizer_actions::{
    EqualizerPresetAction, EQUALIZER_CONFIGURE_PRESET_ITEM, EQUALIZER_PRESET_MENU_SECTIONS,
};
use crate::app::view_model::{
    balance_to_eq_shaded_position, eq_shaded_position_to_balance, eq_shaded_position_to_volume,
    eq_slider_pixel_to_position, equalizer_view_model, volume_to_eq_shaded_position,
    EqualizerViewModel,
};
use crate::app_log_info;
use crate::equalizer::{winamp_original_presets, EqualizerPreset};
use crate::render::{
    equalizer_slider_layout, EqualizerControl, EqualizerRenderState, EqualizerSlider,
    EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH,
};
use crate::skin::layout::{
    equalizer_control_rect, panel_title_button_rect, LayoutPanelKind, PanelTitleButton, SkinRect,
};

use super::app::EguiFrontendState;
use super::layout::clamp_popup_to_rect;
use super::skin_texture::{pixel_snapped_rect, render_equalizer_color_image, upload_color_image};

pub fn equalizer_band_count(view_model: &EqualizerViewModel) -> usize {
    view_model.band_positions.len()
}

pub fn show_equalizer(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = equalizer_view_model(app.controller().state());
    if !view_model.visible {
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
        pixel_snapped_rect(ui.ctx(), rect),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    add_equalizer_titlebar_drag_region(ui, app, rect, &view_model);
    add_equalizer_hit_regions(ui, app, rect, &view_model);
    show_equalizer_presets_popover(ui.ctx(), app, rect);
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
    add_equalizer_title_button_hits(ui, app, base_rect);
    if view_model.shaded {
        add_equalizer_slider_hit(ui, app, base_rect, EqualizerSlider::ShadedVolume);
        add_equalizer_slider_hit(ui, app, base_rect, EqualizerSlider::ShadedBalance);
        return;
    }

    for control in [
        EqualizerControl::On,
        EqualizerControl::Auto,
        EqualizerControl::Presets,
    ] {
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

fn add_equalizer_titlebar_drag_region(
    ui: &mut egui::Ui,
    app: &EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &EqualizerViewModel,
) {
    let titlebar = scale_skin_rect(
        base_rect,
        SkinRect::new(
            0,
            0,
            EQUALIZER_WINDOW_WIDTH,
            crate::render::MAIN_TITLEBAR_HEIGHT,
        ),
        app.scale_factor,
    );
    let response = ui.interact(
        titlebar,
        ui.id().with("eq-titlebar-drag"),
        egui::Sense::click_and_drag(),
    );
    if response.drag_started() {
        let Some(pointer) = response.interact_pointer_pos() else {
            return;
        };
        let x = ((pointer.x - base_rect.left()) / app.scale_factor).floor() as i32;
        let y = ((pointer.y - base_rect.top()) / app.scale_factor).floor() as i32;
        if equalizer_titlebar_drag_excluded(x, y, view_model.shaded) {
            return;
        }
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

fn equalizer_titlebar_drag_excluded(x: i32, y: i32, shaded: bool) -> bool {
    [PanelTitleButton::Shade, PanelTitleButton::Close]
        .into_iter()
        .any(|button| {
            panel_title_button_rect(LayoutPanelKind::Equalizer, button, EQUALIZER_WINDOW_WIDTH)
                .contains(x, y)
        })
        || (shaded
            && [
                EqualizerSlider::ShadedVolume,
                EqualizerSlider::ShadedBalance,
            ]
            .into_iter()
            .any(|slider| equalizer_slider_layout(slider).rect.contains(x, y)))
}

fn add_equalizer_title_button_hits(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
) {
    for button in [PanelTitleButton::Shade, PanelTitleButton::Close] {
        let rect = scale_skin_rect(
            base_rect,
            panel_title_button_rect(LayoutPanelKind::Equalizer, button, EQUALIZER_WINDOW_WIDTH),
            app.scale_factor,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("eq-title-button", button as u8)),
            egui::Sense::click(),
        );
        if response.clicked() {
            let button_name = format!("{button:?}");
            app_log_info!(equalizer, "title button", button_name);
            match button {
                PanelTitleButton::Shade => app.dispatch(PanelCommand::ToggleEqualizerShade),
                PanelTitleButton::Close => {
                    app.dispatch(PanelCommand::SetEqualizerVisibility(false));
                }
            }
        }
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

pub(crate) fn dispatch_equalizer_control(app: &mut EguiFrontendState, control: EqualizerControl) {
    let control_name = format!("{control:?}");
    app_log_info!(equalizer, "control activated", control_name);
    match control {
        EqualizerControl::On => app.dispatch(EqualizerCommand::ToggleActive),
        EqualizerControl::Auto => app.dispatch(EqualizerCommand::ToggleAuto),
        EqualizerControl::Presets => app.equalizer_presets_open = true,
    }
}

pub(crate) fn show_equalizer_presets_popover(
    ctx: &egui::Context,
    app: &mut EguiFrontendState,
    equalizer_rect: egui::Rect,
) {
    if !app.equalizer_presets_open {
        return;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.equalizer_presets_open = false;
        return;
    }

    let presets_button = equalizer_control_rect(EqualizerControl::Presets);
    let estimated_popup_width = 240.0;
    // egui Areas are clipped to the OS window. The Presets button sits near the
    // right edge of the 275px equalizer, while this popup is much wider, so an
    // unadjusted right-opening popup is clipped. Move it left as needed so the
    // full menu remains visible inside the equalizer/window bounds.
    let popup_pos = clamp_popup_to_rect(
        egui::pos2(
            equalizer_rect.left() + presets_button.x as f32 * app.scale_factor,
            equalizer_rect.top() + presets_button.bottom() as f32 * app.scale_factor,
        ),
        equalizer_rect,
        egui::vec2(estimated_popup_width, 0.0),
    );
    let mut close_after_click = false;
    let response = egui::Area::new(egui::Id::new("xmms-egui-equalizer-presets-popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(popup_pos)
        .constrain(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(220.0);
                for section in EQUALIZER_PRESET_MENU_SECTIONS {
                    ui.menu_button(section.label, |ui| {
                        for item in section.items {
                            if ui.button(item.label).clicked() {
                                dispatch_equalizer_preset_action(app, item.action);
                                close_after_click = true;
                                ui.close();
                            }
                        }
                        if section.label == "Load" {
                            ui.separator();
                            ui.label("Winamp original presets");
                            for preset in winamp_original_presets().into_iter().take(12) {
                                if ui.button(&preset.name).clicked() {
                                    apply_equalizer_preset(app, &preset);
                                    close_after_click = true;
                                    ui.close();
                                }
                            }
                        }
                    });
                }
                if ui.button(EQUALIZER_CONFIGURE_PRESET_ITEM.label).clicked() {
                    dispatch_equalizer_preset_action(app, EQUALIZER_CONFIGURE_PRESET_ITEM.action);
                    close_after_click = true;
                }
            });
        });

    let clicked_outside = ctx.input(|input| {
        input.pointer.any_released()
            && input.pointer.latest_pos().is_some_and(|pos| {
                let presets_rect =
                    scale_skin_rect(equalizer_rect, presets_button, app.scale_factor);
                !response.response.rect.contains(pos) && !presets_rect.contains(pos)
            })
    });
    if close_after_click || clicked_outside {
        app.equalizer_presets_open = false;
    }
}

fn dispatch_equalizer_preset_action(app: &mut EguiFrontendState, action: EqualizerPresetAction) {
    match action {
        EqualizerPresetAction::LoadDefault => {
            apply_equalizer_preset(app, &EqualizerPreset::zero("Default"));
        }
        EqualizerPresetAction::LoadZero => {
            apply_equalizer_preset(app, &EqualizerPreset::zero("Zero"));
        }
        EqualizerPresetAction::LoadFromFile | EqualizerPresetAction::LoadFromWinampFile => {
            app.apply_effect(AppEffect::OpenFileDialog(
                FileDialogRequest::LoadEqualizerPreset,
            ));
        }
        EqualizerPresetAction::ImportWinampPresets => {
            app.apply_effect(AppEffect::OpenFileDialog(
                FileDialogRequest::LoadEqualizerPreset,
            ));
        }
        EqualizerPresetAction::SaveToFile | EqualizerPresetAction::SaveToWinampFile => {
            app.apply_effect(AppEffect::OpenFileDialog(
                FileDialogRequest::SaveEqualizerPreset,
            ));
        }
        other => {
            if let Some(message) = other.unsupported_egui_message() {
                app.runtime.pending_messages.push(message.to_string());
            }
        }
    }
}

fn apply_equalizer_preset(app: &mut EguiFrontendState, preset: &EqualizerPreset) {
    app.dispatch(EqualizerCommand::SetPreamp(preset.preamp_position()));
    for (band, position) in preset.band_positions().into_iter().enumerate() {
        app.dispatch(EqualizerCommand::SetBand { band, position });
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
            app.dispatch(EqualizerCommand::SetPreamp(eq_slider_pixel_to_position(
                pixel,
            )));
        }
        EqualizerSlider::Band(band) => {
            let pixel = ((pointer.y - rect.top()) / app.scale_factor).round() as i32;
            app.dispatch(EqualizerCommand::SetBand {
                band,
                position: eq_slider_pixel_to_position(pixel),
            });
        }
        EqualizerSlider::ShadedVolume => {
            let layout = equalizer_slider_layout(slider);
            let position = ((pointer.x - rect.left()) / app.scale_factor).round() as i32;
            app.dispatch(AudioCommand::SetVolume(eq_shaded_position_to_volume(
                position.clamp(layout.min, layout.max),
            )));
        }
        EqualizerSlider::ShadedBalance => {
            let layout = equalizer_slider_layout(slider);
            let position = ((pointer.x - rect.left()) / app.scale_factor).round() as i32;
            app.dispatch(AudioCommand::SetBalance(eq_shaded_position_to_balance(
                position.clamp(layout.min, layout.max),
            )));
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn equalizer_presets_popover_stays_inside_equalizer_window() {
        let equalizer_rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(
                EQUALIZER_WINDOW_WIDTH as f32,
                EQUALIZER_WINDOW_HEIGHT as f32,
            ),
        );
        let presets_button = equalizer_control_rect(EqualizerControl::Presets);
        let popup_width = 240.0;
        let pos = clamp_popup_to_rect(
            egui::pos2(presets_button.x as f32, presets_button.bottom() as f32),
            equalizer_rect,
            egui::vec2(popup_width, 0.0),
        );
        assert!(pos.x >= equalizer_rect.left());
        assert!(
            pos.x + popup_width <= equalizer_rect.right() + 0.5,
            "preset popup right edge {} overflows equalizer right {}",
            pos.x + popup_width,
            equalizer_rect.right()
        );
        assert!(pos.x < presets_button.x as f32);
    }

    #[test]
    fn shaded_equalizer_sliders_update_audio_state() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();

        let scale = app.scale_factor;
        dispatch_equalizer_slider(
            &mut app,
            EqualizerSlider::ShadedVolume,
            egui::pos2(97.0 * scale, 0.0),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(97.0 * scale, 8.0 * scale)),
        );
        assert_eq!(app.controller().state().player.volume(), 100);

        dispatch_equalizer_slider(
            &mut app,
            EqualizerSlider::ShadedBalance,
            egui::pos2(0.0, 0.0),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(42.0 * scale, 8.0 * scale)),
        );
        assert_eq!(app.controller().state().player.balance(), -100);
    }
}
