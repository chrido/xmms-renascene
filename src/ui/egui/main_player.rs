//! egui main player panel/window.

use crate::app::command::{AudioCommand, PanelCommand, PlayerCommand, PlaylistCommand};
use crate::app::view_model::{
    balance_to_position, main_player_view_model, position_to_balance, position_to_volume,
    volume_to_position, MainPlayerViewModel,
};
use crate::player::PlayerState;
use crate::render::{
    MainPushButton, MainSlider, MainToggleButton, MainWindowRenderState, MAIN_TITLEBAR_HEIGHT,
    MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
};
use crate::skin::layout::{
    main_push_button_rect, main_slider_layout, main_toggle_button_rect, SkinRect,
};
use crate::skin::widget::{NumberDisplay, PlayStatusValue};

use super::app::EguiFrontendState;
use super::skin_texture::{render_main_player_color_image, upload_color_image};

pub fn main_player_title(view_model: &MainPlayerViewModel) -> &str {
    &view_model.title
}

pub fn show_main_player(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = main_player_view_model(app.controller().state());
    let render_state = main_render_state(&view_model);
    let Ok(image) = render_main_player_color_image(&app.active_skin, &render_state) else {
        ui.label("failed to render skinned main player");
        return;
    };
    let texture = upload_color_image(ui.ctx(), "xmms-main-player", image);
    let base_height = if view_model.shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        MAIN_WINDOW_HEIGHT
    };
    let size = egui::vec2(
        MAIN_WINDOW_WIDTH as f32 * app.scale_factor,
        base_height as f32 * app.scale_factor,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    add_main_hit_regions(ui, app, rect, &view_model);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn main_render_state(view_model: &MainPlayerViewModel) -> MainWindowRenderState {
    MainWindowRenderState {
        title: if view_model.title.is_empty() {
            "XMMS Renascene".to_string()
        } else {
            view_model.title.clone()
        },
        shaded: view_model.shaded,
        bitrate_text: blank_zero(&view_model.bitrate_text),
        frequency_text: blank_zero(&view_model.frequency_text),
        channels: view_model.channels_text.parse().unwrap_or(0),
        volume_position: volume_to_position(view_model.volume),
        balance_position: balance_to_position(view_model.balance),
        shuffle_selected: view_model.shuffle,
        repeat_selected: view_model.repeat,
        equalizer_selected: false,
        playlist_selected: false,
        play_status: match view_model.player_state {
            PlayerState::Playing => PlayStatusValue::Playing,
            PlayerState::Paused => PlayStatusValue::Paused,
            PlayerState::Stopped => PlayStatusValue::Stopped,
        },
        time_digits: [NumberDisplay::BLANK; 5],
        ..MainWindowRenderState::default()
    }
}

fn blank_zero(text: &str) -> String {
    if text == "0" {
        String::new()
    } else {
        text.to_string()
    }
}

fn add_main_hit_regions(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &MainPlayerViewModel,
) {
    for &button in main_push_buttons(view_model.shaded) {
        let rect = scale_skin_rect(
            base_rect,
            main_push_button_rect(button, view_model.shaded),
            app.scale_factor,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("main-push", button as u8)),
            egui::Sense::click(),
        );
        if response.clicked() {
            dispatch_push(app, button);
        }
    }

    if !view_model.shaded {
        for toggle in [
            MainToggleButton::Shuffle,
            MainToggleButton::Repeat,
            MainToggleButton::Equalizer,
            MainToggleButton::Playlist,
        ] {
            let rect =
                scale_skin_rect(base_rect, main_toggle_button_rect(toggle), app.scale_factor);
            let response = ui.interact(
                rect,
                ui.id().with(("main-toggle", toggle as u8)),
                egui::Sense::click(),
            );
            if response.clicked() {
                dispatch_toggle(app, toggle);
            }
        }
    }

    for &slider in main_sliders(view_model.shaded) {
        let layout = main_slider_layout(slider, view_model.shaded);
        let rect = scale_skin_rect(base_rect, layout.rect, app.scale_factor);
        let response = ui.interact(
            rect,
            ui.id().with(("main-slider", slider as u8)),
            egui::Sense::click_and_drag(),
        );
        if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
            let pointer = response.interact_pointer_pos().unwrap();
            let normalized = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let position =
                layout.min + ((layout.max - layout.min) as f32 * normalized).round() as i32;
            dispatch_slider(app, slider, position);
        }
    }
}

fn main_push_buttons(shaded: bool) -> &'static [MainPushButton] {
    if shaded {
        &[
            MainPushButton::Menu,
            MainPushButton::Minimize,
            MainPushButton::Shade,
            MainPushButton::Close,
            MainPushButton::Previous,
            MainPushButton::Play,
            MainPushButton::Pause,
            MainPushButton::Stop,
            MainPushButton::Next,
            MainPushButton::Eject,
        ]
    } else {
        &[
            MainPushButton::Menu,
            MainPushButton::Minimize,
            MainPushButton::Shade,
            MainPushButton::Close,
            MainPushButton::Previous,
            MainPushButton::Play,
            MainPushButton::Pause,
            MainPushButton::Stop,
            MainPushButton::Next,
            MainPushButton::Eject,
        ]
    }
}

fn main_sliders(shaded: bool) -> &'static [MainSlider] {
    if shaded {
        &[MainSlider::Position]
    } else {
        &[
            MainSlider::Volume,
            MainSlider::Balance,
            MainSlider::Position,
        ]
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

fn dispatch_push(app: &mut EguiFrontendState, button: MainPushButton) {
    match button {
        MainPushButton::Previous => app.dispatch(PlayerCommand::PreviousTrack),
        MainPushButton::Play => app.dispatch(PlayerCommand::Play),
        MainPushButton::Pause => app.dispatch(PlayerCommand::TogglePause),
        MainPushButton::Stop => app.dispatch(PlayerCommand::Stop),
        MainPushButton::Next => app.dispatch(PlayerCommand::NextTrack),
        MainPushButton::Eject => app.dispatch(PlaylistCommand::ExecuteMenu {
            kind: crate::playlist::PlaylistMenuKind::Add,
            index: 0,
        }),
        MainPushButton::Shade => app.dispatch(PanelCommand::ToggleMainShade),
        MainPushButton::Menu | MainPushButton::Minimize | MainPushButton::Close => {}
    }
}

fn dispatch_toggle(app: &mut EguiFrontendState, toggle: MainToggleButton) {
    match toggle {
        MainToggleButton::Shuffle => app.dispatch(PlaylistCommand::ToggleShuffle),
        MainToggleButton::Repeat => app.dispatch(PlaylistCommand::ToggleRepeat),
        MainToggleButton::Equalizer => app.dispatch(PanelCommand::ToggleEqualizerVisibility),
        MainToggleButton::Playlist => app.dispatch(PanelCommand::TogglePlaylistVisibility),
    }
}

fn dispatch_slider(app: &mut EguiFrontendState, slider: MainSlider, position: i32) {
    match slider {
        MainSlider::Volume => app.dispatch(AudioCommand::SetVolume(position_to_volume(position))),
        MainSlider::Balance => {
            app.dispatch(AudioCommand::SetBalance(position_to_balance(position)))
        }
        MainSlider::Position => app.dispatch(PlayerCommand::SeekToMs(0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::AppCommand;

    #[test]
    fn main_player_commands_use_hierarchical_domains() {
        assert_eq!(
            AppCommand::from(PlayerCommand::Play),
            AppCommand::Player(PlayerCommand::Play)
        );
        assert_eq!(
            AppCommand::from(AudioCommand::SetVolume(50)),
            AppCommand::Audio(AudioCommand::SetVolume(50))
        );
        assert_eq!(
            AppCommand::from(PanelCommand::TogglePlaylistVisibility),
            AppCommand::Panel(PanelCommand::TogglePlaylistVisibility)
        );
    }

    #[test]
    fn main_render_state_uses_skin_slider_positions() {
        let state = main_render_state(&MainPlayerViewModel {
            title: String::new(),
            player_state: PlayerState::Stopped,
            volume: 100,
            balance: 0,
            shuffle: false,
            repeat: false,
            shaded: false,
            bitrate_text: "0".to_string(),
            frequency_text: "0".to_string(),
            channels_text: "0".to_string(),
        });

        assert_eq!(state.volume_position, 51);
        assert_eq!(state.balance_position, 12);
        assert_eq!(state.title, "XMMS Renascene");
        assert!(state.bitrate_text.is_empty());
    }
}
