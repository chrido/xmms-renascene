//! egui playlist panel/window.

use crate::app::command::{PanelCommand, PlayerCommand, PlaylistCommand};
use crate::app::effect::{AppEffect, FileDialogRequest};
use crate::app::playlist_actions::{
    playlist_row_click_commands, PlaylistSortAction, PLAYLIST_SORT_MENU_ITEMS,
};
use crate::app::view_model::{
    ellipsize_chars, format_duration, format_title_for_preferences,
    playlist_footer_info as shared_playlist_footer_info,
    playlist_rows_render_state as shared_playlist_rows_render_state, playlist_view_model,
    PlaylistViewModel,
};
use crate::app_log_info;
use crate::player::PlayerState;
use crate::playlist::PlaylistMenuKind;
use crate::render::{playlist_window_height, PlaylistMenuRenderState, PlaylistRowsRenderState, PLAYLIST_MIN_WIDTH};
use crate::skin::layout::{
    panel_title_button_rect, playlist_footer_button_rect, playlist_menu_button_rect,
    playlist_menu_popup_rect, LayoutPanelKind, PanelTitleButton, PlaylistFooterButton,
    PlaylistMenuButton, SkinRect,
};

use super::app::EguiFrontendState;
use super::skin_texture::{
    pixel_snapped_rect, render_playlist_color_image, render_playlist_menu_color_image,
    upload_color_image,
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
    let shaded_info = shaded_playlist_info(app);
    let footer_info = playlist_footer_info(app);
    let (footer_time_minutes, footer_time_seconds) = playlist_footer_time_parts(app);
    let render_scale = app.scale_factor as f64 * ui.ctx().pixels_per_point() as f64;
    let Ok(image) = render_playlist_color_image(
        &app.active_skin,
        true,
        view_model.shaded,
        app.playlist_width,
        app.playlist_height,
        Some(&shaded_info),
        &rows,
        Some(&footer_info),
        Some(&footer_time_minutes),
        Some(&footer_time_seconds),
        render_scale,
    ) else {
        ui.label("failed to render skinned playlist");
        return;
    };
    let texture = upload_color_image(ui.ctx(), "xmms-playlist", image);
    let base_height = playlist_window_height(view_model.shaded, app.playlist_height);
    let size = egui::vec2(
        app.playlist_width as f32 * app.scale_factor,
        base_height as f32 * app.scale_factor,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().image(
        texture.id(),
        pixel_snapped_rect(ui.ctx(), rect),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    add_playlist_titlebar_drag_region(ui, app, rect);
    add_playlist_hit_regions(ui, app, rect, &view_model);
    add_playlist_resize_handle(ui, app, rect, &view_model);
    show_playlist_sort_popover(ui.ctx(), app, rect);
    add_playlist_menu_popover(ui, app, rect);
    show_physical_delete_confirmation(ui.ctx(), app);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn playlist_rows_render_state(
    app: &EguiFrontendState,
    view_model: &PlaylistViewModel,
) -> PlaylistRowsRenderState {
    let _ = view_model;
    shared_playlist_rows_render_state(
        app.controller().state(),
        app.playlist_scroll_offset,
        false,
        None,
        app.playlist_width,
        app.playlist_height,
    )
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
    shared_playlist_footer_info(app.controller().state())
}

pub(crate) fn shaded_playlist_info(app: &EguiFrontendState) -> String {
    let state = app.controller().state();
    let Some(position) = state.playlist.position() else {
        return String::new();
    };
    let Some(entry) = state.playlist.entries().get(position) else {
        return String::new();
    };

    let title = format_title_for_preferences(
        &state.config.title_format,
        &entry.filename,
        &entry.title,
        &state.config,
    );
    let prefix = if state.config.show_numbers_in_pl {
        format!("{}. ", position + 1)
    } else {
        String::new()
    };
    let suffix = if entry.length_ms >= 0 {
        format!(" {}", format_duration(entry.length_ms))
    } else {
        String::new()
    };
    let max_len = ((app.playlist_width - 35) / 5)
        .saturating_sub(prefix.len() as i32)
        .saturating_sub(suffix.len() as i32)
        .max(0) as usize;
    let title = ellipsize_chars(&title, max_len);
    format!("{prefix}{title:<max_len$}{suffix}")
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
            playlist_menu_button_rect(menu, app.playlist_width, app.playlist_height),
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
            playlist_footer_button_rect(button, app.playlist_width, app.playlist_height),
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

fn add_playlist_resize_handle(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &PlaylistViewModel,
) {
    if view_model.shaded {
        app.playlist_resize_start = None;
        return;
    }
    let rect = scale_skin_rect(
        base_rect,
        SkinRect::new(app.playlist_width - 20, app.playlist_height - 20, 20, 20),
        app.scale_factor,
    );
    let response = ui.interact(
        rect,
        ui.id().with("playlist-resize-handle"),
        egui::Sense::click_and_drag(),
    );
    if response.hovered() || app.playlist_resize_start.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
    }
    if response.drag_started() {
        let pointer = response
            .interact_pointer_pos()
            .unwrap_or(rect.right_bottom());
        let local_y = ((pointer.y - base_rect.top()) / app.scale_factor).round() as i32;
        app.playlist_resize_start = Some(app.playlist_height - local_y);
    }
    if let Some(offset_y) = app.playlist_resize_start {
        if ui.ctx().input(|input| input.pointer.primary_down()) {
            if let Some(pointer) = ui.ctx().input(|input| input.pointer.latest_pos()) {
                let local_y = ((pointer.y - base_rect.top()) / app.scale_factor).round() as i32;
                if app.set_playlist_size(PLAYLIST_MIN_WIDTH, local_y + offset_y) {
                    ui.ctx().request_repaint();
                }
            }
        } else {
            app.playlist_resize_start = None;
        }
    }
    if response.drag_stopped() {
        app.playlist_resize_start = None;
    }
}

fn add_playlist_titlebar_drag_region(
    ui: &mut egui::Ui,
    app: &EguiFrontendState,
    base_rect: egui::Rect,
) {
    let titlebar = scale_skin_rect(
        base_rect,
        SkinRect::new(
            0,
            0,
            app.playlist_width,
            crate::render::MAIN_TITLEBAR_HEIGHT,
        ),
        app.scale_factor,
    );
    let response = ui.interact(
        titlebar,
        ui.id().with("playlist-titlebar-drag"),
        egui::Sense::click_and_drag(),
    );
    if response.drag_started() {
        let Some(pointer) = response.interact_pointer_pos() else {
            return;
        };
        let x = ((pointer.x - base_rect.left()) / app.scale_factor).floor() as i32;
        let y = ((pointer.y - base_rect.top()) / app.scale_factor).floor() as i32;
        if [PanelTitleButton::Shade, PanelTitleButton::Close]
            .into_iter()
            .any(|button| {
                panel_title_button_rect(LayoutPanelKind::Playlist, button, app.playlist_width)
                    .contains(x, y)
            })
        {
            return;
        }
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
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
            panel_title_button_rect(LayoutPanelKind::Playlist, button, app.playlist_width),
            app.scale_factor,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("playlist-title-button", button as u8)),
            egui::Sense::click(),
        );
        if response.clicked() {
            let button_name = format!("{button:?}");
            app_log_info!(playlist, "title button", button_name);
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
    let rows_rect = scale_skin_rect(
        base_rect,
        SkinRect::new(12, 20, app.playlist_width - 31, app.playlist_height - 58),
        app.scale_factor,
    );
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
        let ctrl = ui
            .ctx()
            .input(|input| input.modifiers.ctrl || input.modifiers.command);
        if let Some(model) = view_model.rows.get(index) {
            for command in playlist_row_click_commands(model.index, response.double_clicked(), ctrl) {
                app.dispatch(command);
            }
        }
    }
}

pub(crate) fn dispatch_playlist_menu_button(app: &mut EguiFrontendState, menu: PlaylistMenuButton) {
    let menu_name = format!("{menu:?}");
    app_log_info!(playlist, "menu opened", menu_name);
    app.playlist_sort_menu_open = false;
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
    if ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
        app.playlist_menu_open = None;
        return;
    }
    let popup = playlist_menu_popup_rect(kind, app.playlist_width, app.playlist_height);
    let popup_rect = scale_skin_rect(base_rect, popup, app.scale_factor);
    let item_height = 18.0 * app.scale_factor;
    app.playlist_menu_hover = None;
    let mut clicked_item = None;
    for index in 0..kind.item_count() {
        let item_rect = egui::Rect::from_min_size(
            egui::pos2(
                popup_rect.left(),
                popup_rect.top() + index as f32 * item_height,
            ),
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
    let menu_scale = app.scale_factor as f64 * ui.ctx().pixels_per_point() as f64;
    match render_playlist_menu_color_image(
        &app.active_skin,
        render_state,
        popup.width,
        popup.height,
        menu_scale,
    ) {
        Ok(image) => {
            let texture = upload_color_image(
                ui.ctx(),
                format!("xmms-playlist-menu-{kind:?}-{hover:?}"),
                image,
            );
            ui.painter().image(
                texture.id(),
                pixel_snapped_rect(ui.ctx(), popup_rect),
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

    let clicked_outside = ui.ctx().input(|input| {
        input.pointer.any_released()
            && input.pointer.latest_pos().is_some_and(|pos| {
                let button_rect = scale_skin_rect(
                    base_rect,
                    playlist_menu_button_rect(kind, app.playlist_width, app.playlist_height),
                    app.scale_factor,
                );
                !popup_rect.contains(pos) && !button_rect.contains(pos)
            })
    });

    if let Some(index) = clicked_item {
        dispatch_playlist_menu_item(app, kind, index);
        app.playlist_menu_open = None;
    } else if clicked_outside {
        app.playlist_menu_open = None;
    }
}

pub(crate) fn dispatch_playlist_menu_item(
    app: &mut EguiFrontendState,
    kind: PlaylistMenuKind,
    index: usize,
) {
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

fn sort_popover_position(
    playlist_rect: egui::Rect,
    misc_button: SkinRect,
    scale_factor: f32,
    popup_width: f32,
    popup_height: f32,
) -> egui::Pos2 {
    let button_y = playlist_rect.top() + misc_button.y as f32 * scale_factor;
    let anchor_x = playlist_rect.left() + misc_button.x as f32 * scale_factor;
    let max_x = (playlist_rect.right() - popup_width).max(playlist_rect.left());
    let popup_x = anchor_x.min(max_x).max(playlist_rect.left());
    let popup_y = (button_y - popup_height).max(playlist_rect.top());
    egui::pos2(popup_x, popup_y)
}

fn show_playlist_sort_popover(
    ctx: &egui::Context,
    app: &mut EguiFrontendState,
    playlist_rect: egui::Rect,
) {
    if !app.playlist_sort_menu_open {
        return;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.playlist_sort_menu_open = false;
        return;
    }

    let misc_button = playlist_menu_button_rect(
        PlaylistMenuButton::Misc,
        app.playlist_width,
        app.playlist_height,
    );
    let estimated_popup_height = 220.0;
    let popup_width = 200.0;
    // egui Areas are clipped to the OS window, which is only as wide/tall as the
    // docked playlist. Anchoring the popover at the Misc button (far right, near
    // the bottom) would push its buttons off-window and make them unclickable
    // (GTK gets away with this because it uses a native top-level popover).
    // Clamp the popover so it stays fully inside the playlist window.
    let popup_pos = sort_popover_position(
        playlist_rect,
        misc_button,
        app.scale_factor,
        popup_width,
        estimated_popup_height,
    );
    let mut close_after_click = false;
    let response = egui::Area::new(egui::Id::new("xmms-egui-playlist-sort-popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(popup_pos)
        .constrain(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                egui::ScrollArea::vertical()
                    .max_height(estimated_popup_height)
                    .show(ui, |ui| {
                        for sort_item in PLAYLIST_SORT_MENU_ITEMS {
                            close_after_click |=
                                playlist_sort_item(ui, app, sort_item.action, sort_item.label);
                        }
                    });
            });
        });

    let clicked_outside = ctx.input(|input| {
        input.pointer.any_released()
            && input.pointer.latest_pos().is_some_and(|pos| {
                let misc_rect = scale_skin_rect(playlist_rect, misc_button, app.scale_factor);
                !response.response.rect.contains(pos) && !misc_rect.contains(pos)
            })
    });
    if close_after_click || clicked_outside {
        app.playlist_sort_menu_open = false;
    }
}

fn playlist_sort_item(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    action: PlaylistSortAction,
    label: &str,
) -> bool {
    if !ui.button(label).clicked() {
        return false;
    }
    app.dispatch(action.command());
    true
}

pub(crate) fn dispatch_playlist_footer_button(
    app: &mut EguiFrontendState,
    button: PlaylistFooterButton,
) {
    let button_name = format!("{button:?}");
    app_log_info!(playlist, "footer button", button_name);
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
            let visible_rows = ((app.playlist_height - 58) / 11).max(1) as usize;
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
    fn sort_popover_stays_within_narrow_playlist_window() {
        // Docked playlist window is only as wide as the playlist; the Misc button
        // sits near the right edge, so an un-clamped popover would overflow the
        // window and its buttons would be unclickable.
        let playlist_rect =
            egui::Rect::from_min_size(egui::pos2(0.0, 116.0), egui::vec2(275.0, 232.0));
        let misc_button =
            crate::skin::layout::playlist_menu_button_rect(PlaylistMenuButton::Misc, 275, 232);
        let popup_width = 200.0;
        let pos = sort_popover_position(playlist_rect, misc_button, 1.0, popup_width, 220.0);
        assert!(pos.x >= playlist_rect.left());
        assert!(
            pos.x + popup_width <= playlist_rect.right() + 0.5,
            "popover right edge {} overflows window right {}",
            pos.x + popup_width,
            playlist_rect.right()
        );
        assert!(pos.y >= playlist_rect.top());
    }

    #[test]
    fn playlist_rows_apply_preferences_title_format() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        app.controller_mut().state_mut().config.title_format = "%t (%p)".to_string();
        app.controller_mut().state_mut().playlist.add_timed_uri(
            "file:///tmp/song.ogg",
            "Example Artist - Example Title",
            12_000,
        );

        let view_model = playlist_view_model(app.controller().state());
        let rows = playlist_rows_render_state(&app, &view_model);
        let expected = format_title_for_preferences(
            "%t (%p)",
            "file:///tmp/song.ogg",
            "Example Artist - Example Title",
            &app.controller().state().config,
        );
        assert_eq!(rows.entries[0].title, expected);
        assert_ne!(rows.entries[0].title, "Example Artist - Example Title");
    }

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
        app.playlist_sort_menu_open = true;
        dispatch_playlist_menu_button(&mut app, PlaylistMenuKind::Select);
        assert_eq!(app.playlist_menu_open, Some(PlaylistMenuKind::Select));
        assert!(!app.playlist_sort_menu_open);
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
    fn shaded_playlist_info_matches_gtk_title_duration_layout() {
        let mut app = match EguiFrontendState::new(crate::app::preview::PreviewOptions::default()) {
            Ok(app) => app,
            Err(err) => panic!("failed to construct egui state: {err}"),
        };
        app.controller_mut().state_mut().config.show_numbers_in_pl = true;
        app.controller_mut().state_mut().playlist.add_timed_uri(
            "file:///tmp/current-demo.ogg",
            "Current demo track",
            245_000,
        );
        app.controller_mut().state_mut().playlist.set_position(0);

        let info = shaded_playlist_info(&app);

        assert!(info.starts_with("1. Current demo track"));
        assert!(info.ends_with(" 4:05"));
    }

    #[test]
    fn playlist_footer_info_matches_gtk_selected_and_total_durations() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        app.controller_mut().state_mut().playlist.add_timed_uri(
            "file:///tmp/one.ogg",
            "One",
            60_000,
        );
        app.controller_mut()
            .state_mut()
            .playlist
            .add_uri("file:///tmp/unknown.ogg");
        app.controller_mut().state_mut().playlist.add_timed_uri(
            "file:///tmp/two.ogg",
            "Two",
            90_000,
        );
        app.controller_mut().state_mut().playlist.set_position(0);

        assert_eq!(playlist_footer_info(&app), "0:00/2:30+");

        app.controller_mut().state_mut().playlist.entries_mut()[1].selected = true;
        assert_eq!(playlist_footer_info(&app), "?/2:30+");

        app.controller_mut().state_mut().playlist.entries_mut()[1].selected = false;
        app.controller_mut().state_mut().playlist.entries_mut()[2].selected = true;
        assert_eq!(playlist_footer_info(&app), "1:30/2:30+");
    }
}
