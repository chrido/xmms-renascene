//! egui playlist panel/window.

use crate::app::command::{PanelCommand, PlayerCommand, PlaylistCommand};
use crate::app::effect::{AppEffect, FileDialogRequest};
use crate::app::view_model::{
    format_playlist_footer_duration, playlist_view_model, PlaylistViewModel,
};
use crate::player::PlayerState;
use crate::playlist::{PlaylistMenuKind, PlaylistSortKey};
use crate::render::{
    PlaylistMenuRenderState, PlaylistRowRenderEntry, PlaylistRowsRenderState,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH,
};
use crate::skin::layout::{
    panel_title_button_rect, playlist_footer_button_rect, playlist_menu_button_rect,
    playlist_menu_popup_rect, LayoutPanelKind, PanelTitleButton, PlaylistFooterButton,
    PlaylistMenuButton, SkinRect,
};

use super::app::EguiFrontendState;
use super::skin_texture::{
    render_playlist_color_image, render_playlist_menu_color_image, upload_color_image,
};

pub fn playlist_row_count(view_model: &PlaylistViewModel) -> usize {
    view_model.rows.len()
}

pub fn show_playlist(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = playlist_view_model(app.controller().state());
    if !view_model.visible {
        return;
    }
    let rows = playlist_rows_render_state(app, &view_model);
    let footer_info = playlist_footer_info(app);
    let (footer_time_minutes, footer_time_seconds) = playlist_footer_time_parts(app);
    let Ok(image) = render_playlist_color_image(
        &app.active_skin,
        true,
        view_model.shaded,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
        &rows,
        Some(&footer_info),
        Some(&footer_time_minutes),
        Some(&footer_time_seconds),
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
    add_playlist_menu_popover(ui, app, rect);
    show_playlist_sort_popover(ui.ctx(), app);
    show_physical_delete_confirmation(ui.ctx(), app);
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
        scroll_offset: app.playlist_scroll_offset,
        scrollbar_dragging: false,
        search_query: None,
        show_numbers: app.controller().state().config.show_numbers_in_pl,
        font_family: app.controller().state().config.playlist_font.clone(),
        width: PLAYLIST_DEFAULT_WIDTH,
        height: PLAYLIST_DEFAULT_HEIGHT,
    }
}

fn playlist_footer_time_parts(app: &EguiFrontendState) -> (String, String) {
    if app.controller().state().player.state() == PlayerState::Stopped {
        return ("   ".to_string(), "  ".to_string());
    }
    let total_seconds = app.controller().state().config.playback_position_ms.max(0) / 1_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    (format!("{minutes:>3}"), format!("{seconds:02}"))
}

fn playlist_footer_info(app: &EguiFrontendState) -> String {
    let mut selected_ms = 0_i64;
    let mut total_ms = 0_i64;
    let mut selected_more = false;
    let mut total_more = false;
    let current = app.controller().state().playlist.position();
    for (index, entry) in app
        .controller()
        .state()
        .playlist
        .entries()
        .iter()
        .enumerate()
    {
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
    add_playlist_title_button_hits(ui, app, base_rect);
    if view_model.shaded {
        return;
    }

    if app.playlist_menu_open.is_none() {
        add_playlist_rows_hit_region(ui, app, base_rect, view_model);
    }
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
        PlaylistFooterButton::ScrollUp,
        PlaylistFooterButton::ScrollDown,
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

fn add_playlist_title_button_hits(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
) {
    for button in [PanelTitleButton::Shade, PanelTitleButton::Close] {
        let rect = scale_skin_rect(
            base_rect,
            panel_title_button_rect(LayoutPanelKind::Playlist, button, PLAYLIST_DEFAULT_WIDTH),
            app.scale_factor,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("playlist-title-button", button as u8)),
            egui::Sense::click(),
        );
        if response.clicked() {
            match button {
                PanelTitleButton::Shade => app.dispatch(PanelCommand::TogglePlaylistShade),
                PanelTitleButton::Close => {
                    app.dispatch(PanelCommand::SetPlaylistVisibility(false));
                }
            }
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
    let response = ui.interact(
        rows_rect,
        ui.id().with("playlist-rows"),
        egui::Sense::click(),
    );
    response.context_menu(|ui| {
        if ui.button("Remove Selected").clicked() {
            app.dispatch(PlaylistCommand::RemoveSelectedOrCurrent);
            ui.close();
        }
        if ui.button("Remove Dead Files").clicked() {
            app.dispatch(PlaylistCommand::RemoveDead);
            ui.close();
        }
        if ui.button("Physically Delete").clicked() {
            app.confirm_physical_delete_open = true;
            ui.close();
        }
        ui.separator();
        if ui.button("Select All").clicked() {
            app.dispatch(PlaylistCommand::SelectAll);
            ui.close();
        }
        if ui.button("Select None").clicked() {
            app.dispatch(PlaylistCommand::SelectNone);
            ui.close();
        }
        if ui.button("Invert Selection").clicked() {
            app.dispatch(PlaylistCommand::InvertSelection);
            ui.close();
        }
    });
    if (response.clicked() || response.double_clicked())
        && response.interact_pointer_pos().is_some()
    {
        let pointer = response.interact_pointer_pos().unwrap();
        let row = ((pointer.y - rows_rect.top()) / (11.0 * app.scale_factor)).floor() as usize;
        let index = app.playlist_scroll_offset.saturating_add(row);
        if let Some(model) = view_model.rows.get(index) {
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
    app.playlist_menu_open = Some(menu);
}

fn show_physical_delete_confirmation(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if !app.confirm_physical_delete_open {
        return;
    }
    let selected_count = app
        .controller()
        .state()
        .playlist
        .entries()
        .iter()
        .filter(|entry| entry.selected)
        .count();
    let mut open = true;
    egui::Window::new("Delete selected files?")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!(
                "Permanently delete {selected_count} selected local file(s) from disk?"
            ));
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    app.confirm_physical_delete_open = false;
                }
                if ui.button("Delete").clicked() {
                    app.dispatch(PlaylistCommand::PhysicallyDeleteSelected);
                    app.confirm_physical_delete_open = false;
                }
            });
        });
    if !open {
        app.confirm_physical_delete_open = false;
    }
}

fn add_playlist_menu_popover(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
) {
    let Some(kind) = app.playlist_menu_open else {
        return;
    };
    let popup = playlist_menu_popup_rect(kind, PLAYLIST_DEFAULT_WIDTH, PLAYLIST_DEFAULT_HEIGHT);
    let popup_rect = scale_skin_rect(base_rect, popup, app.scale_factor);
    let item_height = 18.0 * app.scale_factor;
    app.playlist_menu_hover = None;
    let mut clicked_item = None;
    for index in 0..kind.item_count() {
        let item_rect = egui::Rect::from_min_size(
            egui::pos2(popup_rect.left(), popup_rect.top() + index as f32 * item_height),
            egui::vec2(popup_rect.width(), item_height),
        );
        let response = ui.interact(
            item_rect,
            ui.id().with(("playlist-skinned-menu", kind as u8, index)),
            egui::Sense::click(),
        );
        if response.hovered() {
            app.playlist_menu_hover = Some((kind, index));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            clicked_item = Some(index);
        }
    }

    let hover = app
        .playlist_menu_hover
        .and_then(|(hover_kind, index)| (hover_kind == kind).then_some(index));
    let render_state = PlaylistMenuRenderState { kind, hover };
    match render_playlist_menu_color_image(
        &app.active_skin,
        render_state,
        popup.width,
        popup.height,
    ) {
        Ok(image) => {
            let texture = upload_color_image(
                ui.ctx(),
                format!("xmms-playlist-menu-{kind:?}-{hover:?}"),
                image,
            );
            ui.painter().image(
                texture.id(),
                popup_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        Err(err) => {
            ui.painter().text(
                popup_rect.left_top(),
                egui::Align2::LEFT_TOP,
                format!("menu render error: {err}"),
                egui::FontId::monospace(8.0 * app.scale_factor),
                egui::Color32::WHITE,
            );
        }
    }

    if let Some(index) = clicked_item {
        dispatch_playlist_menu_item(app, kind, index);
        app.playlist_menu_open = None;
    }
}

fn dispatch_playlist_menu_item(app: &mut EguiFrontendState, kind: PlaylistMenuKind, index: usize) {
    match (kind, index) {
        (PlaylistMenuKind::Add, 0) => {
            app.prompt_open = Some(super::menu::EguiPrompt::OpenLocation);
            app.prompt_text.clear();
        }
        (PlaylistMenuKind::Add, 1) => app.apply_effect(AppEffect::OpenFileDialog(
            FileDialogRequest::AddAudioDirectory,
        )),
        (PlaylistMenuKind::Add, 2) => {
            app.apply_effect(AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles));
        }
        (PlaylistMenuKind::Misc, 0) => app.playlist_sort_menu_open = true,
        _ => app.dispatch(PlaylistCommand::ExecuteMenu { kind, index }),
    }
}

fn show_playlist_sort_popover(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if !app.playlist_sort_menu_open {
        return;
    }
    let mut open = true;
    let mut close_after_click = false;
    egui::Window::new("Playlist Sort")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Sort List");
            close_after_click |=
                playlist_sort_item(ui, app, false, PlaylistSortKey::Title, "By Title");
            close_after_click |=
                playlist_sort_item(ui, app, false, PlaylistSortKey::Filename, "By Filename");
            close_after_click |=
                playlist_sort_item(ui, app, false, PlaylistSortKey::Path, "By Path + Filename");
            close_after_click |=
                playlist_sort_item(ui, app, false, PlaylistSortKey::Date, "By Date");
            ui.separator();
            ui.label("Sort Selection");
            close_after_click |=
                playlist_sort_item(ui, app, true, PlaylistSortKey::Title, "By Title");
            close_after_click |=
                playlist_sort_item(ui, app, true, PlaylistSortKey::Filename, "By Filename");
            close_after_click |=
                playlist_sort_item(ui, app, true, PlaylistSortKey::Path, "By Path + Filename");
            close_after_click |=
                playlist_sort_item(ui, app, true, PlaylistSortKey::Date, "By Date");
            ui.separator();
            if ui.button("Randomize List").clicked() {
                app.dispatch(PlaylistCommand::Randomize);
                close_after_click = true;
            }
            if ui.button("Reverse List").clicked() {
                app.dispatch(PlaylistCommand::Reverse);
                close_after_click = true;
            }
        });
    if !open || close_after_click {
        app.playlist_sort_menu_open = false;
    }
}

fn playlist_sort_item(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    selected_only: bool,
    key: PlaylistSortKey,
    label: &str,
) -> bool {
    if !ui.button(label).clicked() {
        return false;
    }
    if selected_only {
        app.dispatch(PlaylistCommand::SortSelected(key));
    } else {
        app.dispatch(PlaylistCommand::Sort(key));
    }
    true
}

fn dispatch_playlist_footer_button(app: &mut EguiFrontendState, button: PlaylistFooterButton) {
    match button {
        PlaylistFooterButton::Previous => app.dispatch(PlayerCommand::PreviousTrack),
        PlaylistFooterButton::Play => app.dispatch(PlayerCommand::Play),
        PlaylistFooterButton::Pause => app.dispatch(PlayerCommand::TogglePause),
        PlaylistFooterButton::Stop => app.dispatch(PlayerCommand::Stop),
        PlaylistFooterButton::Next => app.dispatch(PlayerCommand::NextTrack),
        PlaylistFooterButton::Eject => {
            app.apply_effect(AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles))
        }
        PlaylistFooterButton::ScrollUp => {
            app.playlist_scroll_offset = app.playlist_scroll_offset.saturating_sub(1);
        }
        PlaylistFooterButton::ScrollDown => {
            let visible_rows = ((PLAYLIST_DEFAULT_HEIGHT - 58) / 11).max(1) as usize;
            let max_offset = app
                .controller()
                .state()
                .playlist
                .len()
                .saturating_sub(visible_rows);
            app.playlist_scroll_offset = (app.playlist_scroll_offset + 1).min(max_offset);
        }
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

pub fn playlist_menu_command(
    kind: crate::playlist::PlaylistMenuKind,
    index: usize,
) -> PlaylistCommand {
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
        app.controller_mut().state_mut().playlist.add_timed_uri(
            "file:///tmp/song.ogg",
            "Song",
            12_000,
        );
        dispatch_playlist_menu_button(&mut app, PlaylistMenuKind::Select);
        assert_eq!(app.playlist_menu_open, Some(PlaylistMenuKind::Select));
        dispatch_playlist_menu_item(&mut app, PlaylistMenuKind::Select, 2);
        assert!(app.controller().state().playlist.entries()[0].selected);

        dispatch_playlist_footer_button(&mut app, PlaylistFooterButton::Play);
        assert_eq!(
            app.controller().state().player.state(),
            crate::player::PlayerState::Playing
        );
    }

    #[test]
    fn playlist_footer_scroll_buttons_update_scroll_offset() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        for index in 0..20 {
            app.controller_mut().state_mut().playlist.add_timed_uri(
                format!("file:///tmp/{index}.ogg"),
                format!("Song {index}"),
                12_000,
            );
        }

        dispatch_playlist_footer_button(&mut app, PlaylistFooterButton::ScrollDown);
        assert_eq!(app.playlist_scroll_offset, 1);
        dispatch_playlist_footer_button(&mut app, PlaylistFooterButton::ScrollUp);
        assert_eq!(app.playlist_scroll_offset, 0);
    }

    #[test]
    fn playlist_footer_info_sums_durations() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        app.controller_mut().state_mut().playlist.add_timed_uri(
            "file:///tmp/song.ogg",
            "Song",
            12_000,
        );
        app.controller_mut().state_mut().playlist.set_position(0);

        assert_eq!(playlist_footer_info(&app), "0:12/0:12");
    }
}
