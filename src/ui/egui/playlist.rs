//! egui playlist panel/window.

use crate::app::command::{PlayerCommand, PlaylistCommand};
use crate::app::view_model::{
    format_playlist_footer_duration, playlist_view_model, PlaylistViewModel,
};
use crate::playlist::PlaylistMenuKind;
use crate::render::{
    PlaylistRowRenderEntry, PlaylistRowsRenderState, PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH,
};
use crate::skin::layout::{
    playlist_footer_button_rect, playlist_menu_button_rect, PlaylistFooterButton,
    PlaylistMenuButton, SkinRect,
};

use super::app::EguiFrontendState;
use super::skin_texture::{render_playlist_color_image, upload_color_image};

pub fn playlist_row_count(view_model: &PlaylistViewModel) -> usize {
    view_model.rows.len()
}

pub fn show_playlist(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = playlist_view_model(app.controller().state());
    if !view_model.visible || view_model.detached {
        return;
    }
    let rows = playlist_rows_render_state(app, &view_model);
    let footer_info = playlist_footer_info(app);
    let Ok(image) = render_playlist_color_image(
        &app.active_skin,
        true,
        view_model.shaded,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
        &rows,
        Some(&footer_info),
    ) else {
        ui.label("failed to render skinned playlist");
        return;
    };
    let texture = upload_color_image(ui.ctx(), "xmms-playlist", image);
    let base_height = if view_model.shaded {
        crate::render::MAIN_TITLEBAR_HEIGHT
    } else {
        PLAYLIST_DEFAULT_HEIGHT
    };
    let size = egui::vec2(
        PLAYLIST_DEFAULT_WIDTH as f32 * app.scale_factor,
        base_height as f32 * app.scale_factor,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    add_playlist_hit_regions(ui, app, rect, &view_model);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn playlist_rows_render_state(
    app: &EguiFrontendState,
    view_model: &PlaylistViewModel,
) -> PlaylistRowsRenderState {
    PlaylistRowsRenderState {
        entries: view_model
            .rows
            .iter()
            .map(|row| PlaylistRowRenderEntry {
                title: row.title.clone(),
                length_ms: app
                    .controller()
                    .state()
                    .playlist
                    .entries()
                    .get(row.index)
                    .map(|entry| entry.length_ms)
                    .unwrap_or(-1),
                selected: row.selected,
                current: row.current,
            })
            .collect(),
        scroll_offset: 0,
        scrollbar_dragging: false,
        search_query: None,
        show_numbers: app.controller().state().config.show_numbers_in_pl,
        font_family: app.controller().state().config.playlist_font.clone(),
        width: PLAYLIST_DEFAULT_WIDTH,
        height: PLAYLIST_DEFAULT_HEIGHT,
    }
}

fn playlist_footer_info(app: &EguiFrontendState) -> String {
    let mut selected_ms = 0_i64;
    let mut total_ms = 0_i64;
    let mut selected_more = false;
    let mut total_more = false;
    let current = app.controller().state().playlist.position();
    for (index, entry) in app.controller().state().playlist.entries().iter().enumerate() {
        if entry.length_ms >= 0 {
            total_ms += entry.length_ms;
        } else {
            total_more = true;
        }
        if entry.selected || current == Some(index) {
            if entry.length_ms >= 0 {
                selected_ms += entry.length_ms;
            } else {
                selected_more = true;
            }
        }
    }
    format!(
        "{}/{}",
        format_playlist_footer_duration(selected_ms, selected_more),
        format_playlist_footer_duration(total_ms, total_more)
    )
}

fn add_playlist_hit_regions(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &PlaylistViewModel,
) {
    app.playlist_menu_hover = None;
    if view_model.shaded {
        return;
    }

    add_playlist_rows_hit_region(ui, app, base_rect, view_model);
    for menu in [
        PlaylistMenuButton::Add,
        PlaylistMenuButton::Remove,
        PlaylistMenuButton::Select,
        PlaylistMenuButton::Misc,
        PlaylistMenuButton::List,
    ] {
        let rect = scale_skin_rect(
            base_rect,
            playlist_menu_button_rect(menu, PLAYLIST_DEFAULT_WIDTH, PLAYLIST_DEFAULT_HEIGHT),
            app.scale_factor,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("playlist-menu", menu as u8)),
            egui::Sense::click(),
        );
        if response.clicked() {
            dispatch_playlist_menu_button(app, menu);
        }
    }

    for button in [
        PlaylistFooterButton::Previous,
        PlaylistFooterButton::Play,
        PlaylistFooterButton::Pause,
        PlaylistFooterButton::Stop,
        PlaylistFooterButton::Next,
        PlaylistFooterButton::Eject,
    ] {
        let rect = scale_skin_rect(
            base_rect,
            playlist_footer_button_rect(button, PLAYLIST_DEFAULT_WIDTH, PLAYLIST_DEFAULT_HEIGHT),
            app.scale_factor,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("playlist-footer", button as u8)),
            egui::Sense::click(),
        );
        if response.clicked() {
            dispatch_playlist_footer_button(app, button);
        }
    }
}

fn add_playlist_rows_hit_region(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &PlaylistViewModel,
) {
    let rows_rect = scale_skin_rect(base_rect, SkinRect::new(12, 20, 243, 176), app.scale_factor);
    let response = ui.interact(rows_rect, ui.id().with("playlist-rows"), egui::Sense::click());
    if (response.clicked() || response.double_clicked()) && response.interact_pointer_pos().is_some()
    {
        let pointer = response.interact_pointer_pos().unwrap();
        let row = ((pointer.y - rows_rect.top()) / (11.0 * app.scale_factor)).floor() as usize;
        if let Some(model) = view_model.rows.get(row) {
            if response.double_clicked() {
                app.controller_mut()
                    .state_mut()
                    .playlist
                    .set_position(model.index);
                app.dispatch(PlayerCommand::Play);
            } else if let Some(entry) = app
                .controller_mut()
                .state_mut()
                .playlist
                .entries_mut()
                .get_mut(model.index)
            {
                entry.selected = !entry.selected;
            }
        }
    }
}

fn dispatch_playlist_menu_button(app: &mut EguiFrontendState, menu: PlaylistMenuButton) {
    match menu {
        PlaylistMenuKind::Add => app.dispatch(PlaylistCommand::ExecuteMenu { kind: menu, index: 0 }),
        PlaylistMenuKind::Remove => app.dispatch(PlaylistCommand::RemoveSelectedOrCurrent),
        PlaylistMenuKind::Select => app.dispatch(PlaylistCommand::SelectAll),
        PlaylistMenuKind::Misc => app.dispatch(PlaylistCommand::InvertSelection),
        PlaylistMenuKind::List => app.dispatch(PlaylistCommand::Randomize),
    }
}

fn dispatch_playlist_footer_button(app: &mut EguiFrontendState, button: PlaylistFooterButton) {
    match button {
        PlaylistFooterButton::Previous => app.dispatch(PlayerCommand::PreviousTrack),
        PlaylistFooterButton::Play => app.dispatch(PlayerCommand::Play),
        PlaylistFooterButton::Pause => app.dispatch(PlayerCommand::TogglePause),
        PlaylistFooterButton::Stop => app.dispatch(PlayerCommand::Stop),
        PlaylistFooterButton::Next => app.dispatch(PlayerCommand::NextTrack),
        PlaylistFooterButton::Eject => app.dispatch(PlaylistCommand::ExecuteMenu {
            kind: PlaylistMenuKind::Add,
            index: 0,
        }),
        PlaylistFooterButton::ScrollUp | PlaylistFooterButton::ScrollDown => {}
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

pub fn playlist_menu_command(kind: crate::playlist::PlaylistMenuKind, index: usize) -> PlaylistCommand {
    PlaylistCommand::ExecuteMenu { kind, index }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::PlaylistMenuKind;

    #[test]
    fn playlist_menu_translation_uses_playlist_command_domain() {
        assert_eq!(
            playlist_menu_command(PlaylistMenuKind::Add, 2),
            PlaylistCommand::ExecuteMenu {
                kind: PlaylistMenuKind::Add,
                index: 2,
            }
        );
    }

    #[test]
    fn playlist_buttons_dispatch_to_app_state() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        app.controller_mut()
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/song.ogg", "Song", 12_000);
        dispatch_playlist_menu_button(&mut app, PlaylistMenuKind::Select);
        assert!(app.controller().state().playlist.entries()[0].selected);

        dispatch_playlist_footer_button(&mut app, PlaylistFooterButton::Play);
        assert_eq!(app.controller().state().player.state(), crate::player::PlayerState::Playing);
    }

    #[test]
    fn playlist_footer_info_sums_durations() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        app.controller_mut()
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/song.ogg", "Song", 12_000);
        app.controller_mut().state_mut().playlist.set_position(0);

        assert_eq!(playlist_footer_info(&app), "0:12/0:12");
    }
}
