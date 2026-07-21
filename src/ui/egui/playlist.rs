//! egui playlist panel/window.

use crate::app::command::{PanelCommand, PlayerCommand, PlaylistCommand};
use crate::app::effect::{AppEffect, FileDialogRequest};
#[cfg(target_os = "android")]
use crate::app::playlist_actions::playlist_play_first_selected_commands;
use crate::app::playlist_actions::{
    playlist_queue_target_indices, playlist_row_click_commands, PlaylistSortAction,
    PLAYLIST_SORT_MENU_ITEMS,
};
use crate::app::view_model::{
    ellipsize_chars, format_duration, formatted_playlist_entry_title,
    playlist_footer_info as shared_playlist_footer_info,
    playlist_rows_render_state as shared_playlist_rows_render_state, playlist_view_model,
    PlaylistViewModel,
};
use crate::app_log_info;
use crate::player::PlayerState;
use crate::playlist::PlaylistMenuKind;
use crate::render::{playlist_window_height, PlaylistMenuRenderState, PLAYLIST_MIN_WIDTH};
use crate::skin::layout::{
    panel_title_button_rect, playlist_footer_button_rect, playlist_menu_button_rect,
    playlist_menu_popup_rect, LayoutPanelKind, PanelTitleButton, PlaylistFooterButton,
    PlaylistMenuButton, SkinRect,
};

#[cfg(target_os = "android")]
use super::android::playlist_manager::{
    managed_playlist_name, PlaylistManager, PlaylistManagerAction,
};
#[cfg(target_os = "android")]
use super::android_runtime::AndroidLayoutSnapshot;
use super::app::EguiFrontendState;
use super::layout::clamp_popup_to_rect;
use super::render_cache::{CachedPlaylistTexture, PlaylistTextureKey};
use super::skin_texture::{
    pixel_snapped_rect, render_playlist_color_image, render_playlist_menu_color_image,
    upload_color_image,
};
use super::ui_state::ActiveOverlay;

pub fn playlist_row_count(view_model: &PlaylistViewModel) -> usize {
    view_model.rows.len()
}

#[cfg(any(target_os = "android", test))]
const ANDROID_SAVED_PLAYLIST_ROW_HEIGHT: f32 = 52.0;

#[cfg(any(target_os = "android", test))]
fn android_saved_playlist_row_size(available_width: f32) -> egui::Vec2 {
    egui::vec2(available_width.max(1.0), ANDROID_SAVED_PLAYLIST_ROW_HEIGHT)
}

#[cfg(target_os = "android")]
pub(crate) fn show_android_playlist_manager(
    ctx: &egui::Context,
    manager: &mut PlaylistManager,
    layout: Option<AndroidLayoutSnapshot>,
) -> Option<PlaylistManagerAction> {
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        return Some(PlaylistManagerAction::Close);
    }

    let pixels_per_point = ctx.pixels_per_point().max(f32::EPSILON);
    let Some(layout) = layout else {
        return None;
    };
    let screen = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            layout.width as f32 / pixels_per_point,
            layout.height as f32 / pixels_per_point,
        ),
    );
    let insets = layout.insets;
    let left_inset = insets.left as f32 / pixels_per_point;
    let top_inset = insets.top as f32 / pixels_per_point;
    let right_inset = insets.right as f32 / pixels_per_point;
    let bottom_inset = insets.bottom as f32 / pixels_per_point;
    let horizontal_margin = 16.0;
    let vertical_margin = 12.0;
    let content_width =
        (screen.width() - left_inset - right_inset - horizontal_margin * 2.0).max(1.0);
    let content_height =
        (screen.height() - top_inset - bottom_inset - vertical_margin * 2.0).max(1.0);
    let saved_playlists = manager.saved_playlists().to_vec();
    let mut action = None;

    egui::Area::new(egui::Id::new("xmms-android-playlist-manager"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, egui::Color32::from_gray(46));
            ui.add_space(top_inset + vertical_margin);
            ui.horizontal(|ui| {
                ui.add_space(left_inset + horizontal_margin);
                ui.vertical(|ui| {
                    ui.set_width(content_width);
                    ui.set_min_height(content_height);
                    super::preferences::apply_android_preferences_style(ui);
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized([88.0, 48.0], egui::Button::new("Close"))
                            .clicked()
                        {
                            action = Some(PlaylistManagerAction::Close);
                        }
                        ui.heading("Playlists");
                    });
                    ui.separator();
                    ui.heading("Save current playlist");
                    ui.label("Playlist name");
                    ui.add_sized(
                        [ui.available_width(), 48.0],
                        egui::TextEdit::singleline(manager.name_mut()),
                    );
                    if ui
                        .add_sized([ui.available_width(), 56.0], egui::Button::new("Save"))
                        .clicked()
                    {
                        action = Some(PlaylistManagerAction::Save);
                    }
                    if ui
                        .add_sized(
                            [ui.available_width(), 56.0],
                            egui::Button::new("Import file..."),
                        )
                        .clicked()
                    {
                        action = Some(PlaylistManagerAction::Import);
                    }

                    ui.separator();
                    ui.heading("Saved playlists");
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if saved_playlists.is_empty() {
                                ui.label("No playlists saved in app storage.");
                            }
                            for path in saved_playlists {
                                let name = path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .map(managed_playlist_name)
                                    .filter(|name| !name.is_empty())
                                    .unwrap_or_else(|| "Playlist".to_string());
                                ui.allocate_ui_with_layout(
                                    android_saved_playlist_row_size(ui.available_width()),
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_sized(
                                                [88.0, ANDROID_SAVED_PLAYLIST_ROW_HEIGHT],
                                                egui::Button::new("Delete"),
                                            )
                                            .clicked()
                                        {
                                            action =
                                                Some(PlaylistManagerAction::Delete(path.clone()));
                                        }
                                        if ui
                                            .add_sized(
                                                [100.0, ANDROID_SAVED_PLAYLIST_ROW_HEIGHT],
                                                egui::Button::new("Export"),
                                            )
                                            .clicked()
                                        {
                                            action =
                                                Some(PlaylistManagerAction::Export(path.clone()));
                                        }
                                        if ui
                                            .add_sized(
                                                [88.0, ANDROID_SAVED_PLAYLIST_ROW_HEIGHT],
                                                egui::Button::new("Load"),
                                            )
                                            .clicked()
                                        {
                                            action =
                                                Some(PlaylistManagerAction::Load(path.clone()));
                                        }
                                        ui.with_layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label(name);
                                            },
                                        );
                                    },
                                );
                            }
                        });
                });
            });
        });
    action
}

pub fn show_playlist(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = playlist_view_model(app.controller().state());
    if !view_model.visible {
        return;
    }
    let rows = shared_playlist_rows_render_state(
        app.controller().state(),
        app.playlist_scroll_offset,
        false,
        None,
        app.playlist_width,
        app.playlist_height,
    );
    let shaded_info = shaded_playlist_info(app);
    let footer_info = playlist_footer_info(app);
    let (footer_time_minutes, footer_time_seconds) = playlist_footer_time_parts(app);
    let render_scale = app.scale_factor as f64 * ui.ctx().pixels_per_point() as f64;
    let texture_key = PlaylistTextureKey {
        generation: app.render_cache.generation,
        focused: true,
        shaded: view_model.shaded,
        width: app.playlist_width,
        height: app.playlist_height,
        shaded_info,
        rows,
        footer_info,
        footer_time_minutes,
        footer_time_seconds,
        render_scale_bits: render_scale.to_bits(),
    };
    if app
        .render_cache
        .playlist
        .as_ref()
        .is_none_or(|cached| cached.key != texture_key)
    {
        let Ok(image) = render_playlist_color_image(
            &app.active_skin,
            texture_key.focused,
            texture_key.shaded,
            texture_key.width,
            texture_key.height,
            Some(&texture_key.shaded_info),
            &texture_key.rows,
            Some(&texture_key.footer_info),
            Some(&texture_key.footer_time_minutes),
            Some(&texture_key.footer_time_seconds),
            render_scale,
        ) else {
            ui.label("failed to render skinned playlist");
            return;
        };
        if let Some(cached) = &mut app.render_cache.playlist {
            cached.texture.set(image, egui::TextureOptions::NEAREST);
            cached.key = texture_key;
        } else {
            app.render_cache.playlist = Some(CachedPlaylistTexture {
                key: texture_key,
                texture: upload_color_image(ui.ctx(), "xmms-playlist", image),
            });
        }
    }
    let texture_id = app
        .render_cache
        .playlist
        .as_ref()
        .expect("playlist texture initialized")
        .texture
        .id();
    let base_height = playlist_window_height(view_model.shaded, app.playlist_height);
    let size = egui::vec2(
        app.playlist_width as f32 * app.scale_factor,
        base_height as f32 * app.scale_factor,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().image(
        texture_id,
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

    let title = formatted_playlist_entry_title(state, entry);
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
    app.ui.playlist_menu_hover = None;
    add_playlist_title_button_hits(ui, app, base_rect);
    if view_model.shaded {
        return;
    }

    if app.ui.active_overlay.playlist_menu().is_none() {
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
    // Start the resize on the primary press edge whose origin lies on the handle,
    // rather than on egui's `drag_started()`. Under a WM-less/software-rendered X
    // server the synthetic press+drag can be coalesced or delivered before hover
    // is established, so egui never attributes the drag to the handle and
    // `drag_started()` stays false. The press-origin edge is still reported, so
    // this makes the resize grab reliable while the press-origin (not the
    // post-threshold pointer) avoids absorbing the initial threshold motion.
    let press_origin = ui.ctx().input(|input| {
        input
            .pointer
            .button_pressed(egui::PointerButton::Primary)
            .then(|| input.pointer.press_origin())
            .flatten()
    });
    if let Some(origin) = press_origin {
        if rect.contains(origin) {
            let local_y = ((origin.y - base_rect.top()) / app.scale_factor).round() as i32;
            app.playlist_resize_start = Some(app.playlist_height - local_y);
        }
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
    #[cfg(target_os = "android")]
    let rows_sense = egui::Sense::click_and_drag();
    #[cfg(not(target_os = "android"))]
    let rows_sense = egui::Sense::click();
    let response = ui.interact(rows_rect, ui.id().with("playlist-rows"), rows_sense);
    #[cfg(target_os = "android")]
    let touch_gesture_handled =
        handle_playlist_touch_scroll(ui, app, &response, rows_rect, view_model);
    #[cfg(not(target_os = "android"))]
    let touch_gesture_handled = false;
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
            app.ui.active_overlay = ActiveOverlay::ConfirmPhysicalDelete;
            ui.close();
        }
        ui.separator();
        if ui.button("Toggle Queue").clicked() {
            toggle_playlist_queue_targets(app);
            ui.close();
        }
        if ui
            .add_enabled(
                !app.controller().playlist_queue().is_empty(),
                egui::Button::new("Clear Queue"),
            )
            .clicked()
        {
            app.dispatch(PlaylistCommand::ClearQueue);
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
    if !touch_gesture_handled
        && (response.clicked() || response.double_clicked())
        && response.interact_pointer_pos().is_some()
    {
        let pointer = response.interact_pointer_pos().unwrap();
        let row = ((pointer.y - rows_rect.top()) / (11.0 * app.scale_factor)).floor() as usize;
        let index = app.playlist_scroll_offset.saturating_add(row);
        let ctrl = ui
            .ctx()
            .input(|input| input.modifiers.ctrl || input.modifiers.command);
        if let Some(model) = view_model.rows.get(index) {
            app.dispatch_all(playlist_row_click_commands(
                model.index,
                response.double_clicked(),
                ctrl,
            ));
        }
    }
}

fn toggle_playlist_queue_targets(app: &mut EguiFrontendState) -> bool {
    let targets = playlist_queue_target_indices(&app.controller().state().playlist);
    if targets.is_empty() {
        return false;
    }
    app.dispatch(PlaylistCommand::ToggleQueue(targets));
    true
}

#[cfg(target_os = "android")]
fn handle_playlist_touch_scroll(
    ui: &egui::Ui,
    app: &mut EguiFrontendState,
    response: &egui::Response,
    rows_rect: egui::Rect,
    view_model: &PlaylistViewModel,
) -> bool {
    let mut gesture_handled = false;
    let press_origin = ui.ctx().input(|input| {
        input
            .pointer
            .button_pressed(egui::PointerButton::Primary)
            .then(|| input.pointer.press_origin())
            .flatten()
            .filter(|origin| rows_rect.contains(*origin))
    });
    let drag_start = press_origin.or_else(|| {
        response
            .drag_started()
            .then(|| response.interact_pointer_pos())
            .flatten()
    });
    if let Some(drag_start) = drag_start {
        let row = {
            let row =
                ((drag_start.y - rows_rect.top()) / (11.0 * app.scale_factor)).floor() as usize;
            let index = app.playlist_scroll_offset.saturating_add(row);
            view_model.rows.get(index).map(|model| model.index)
        };
        app.playlist_touch_gesture.begin(drag_start, row);
    }
    if let Some(drag_start) = app.playlist_touch_gesture.start() {
        let event_delta = ui.ctx().input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::PointerMoved(pos) | egui::Event::Touch { pos, .. } => {
                        Some(*pos - drag_start)
                    }
                    _ => None,
                })
                .max_by(|left, right| left.length_sq().total_cmp(&right.length_sq()))
        });
        if let Some(event_delta) = event_delta {
            app.playlist_touch_gesture.observe(event_delta);
        }
    }
    if response.dragged() {
        let rows = app.playlist_touch_gesture.drag(
            response.total_drag_delta().unwrap_or_default(),
            11.0 * app.scale_factor,
        );
        if rows != 0 {
            scroll_playlist_rows(app, rows);
            ui.ctx().request_repaint();
        }
    }
    let drag_released = response.drag_stopped()
        || (app.playlist_touch_gesture.is_active()
            && !ui.ctx().input(|input| input.pointer.primary_down()));
    if drag_released {
        let release_pointer = ui
            .ctx()
            .input(|input| input.pointer.latest_pos())
            .or_else(|| response.interact_pointer_pos());
        let release = app
            .playlist_touch_gesture
            .release(release_pointer)
            .expect("active playlist gesture");
        let drag_delta = release.delta;
        gesture_handled = drag_delta.length_sq() >= 8.0_f32.powi(2);
        let release_velocity = ui.ctx().input(|input| input.pointer.velocity());
        if is_playlist_right_swipe(drag_delta) || is_playlist_left_swipe(drag_delta) {
            let swiped_index = release.row.or_else(|| {
                response.interact_pointer_pos().and_then(|pointer| {
                    let drag_start = pointer - drag_delta;
                    let row = ((drag_start.y - rows_rect.top()) / (11.0 * app.scale_factor)).floor()
                        as usize;
                    let index = app.playlist_scroll_offset.saturating_add(row);
                    view_model.rows.get(index).map(|model| model.index)
                })
            });
            if let Some(swiped_index) = swiped_index {
                if is_playlist_right_swipe(drag_delta) {
                    set_swiped_playlist_selection(app, swiped_index, true);
                } else {
                    set_swiped_playlist_selection(app, swiped_index, false);
                }
            }
        } else {
            if is_playlist_upward_play_swipe(drag_delta, release_velocity, release.duration) {
                play_first_selected_playlist_entry(app);
            } else if is_playlist_downward_pause_swipe(
                drag_delta,
                release_velocity,
                release.duration,
            ) {
                app.dispatch(PlayerCommand::Pause);
                app_log_info!(playlist, "swipe playback paused");
            }
        }
    }
    gesture_handled
}

#[cfg(any(target_os = "android", test))]
fn is_playlist_right_swipe(delta: egui::Vec2) -> bool {
    delta.x >= 48.0 && delta.x >= delta.y.abs() * 1.5
}

#[cfg(any(target_os = "android", test))]
fn is_playlist_left_swipe(delta: egui::Vec2) -> bool {
    delta.x <= -48.0 && -delta.x >= delta.y.abs() * 1.5
}

#[cfg(any(target_os = "android", test))]
fn is_playlist_upward_play_swipe(
    delta: egui::Vec2,
    release_velocity: egui::Vec2,
    gesture_duration: std::time::Duration,
) -> bool {
    delta.y <= -72.0
        && -delta.y >= delta.x.abs() * 1.75
        && has_vertical_swipe_momentum(delta, release_velocity, gesture_duration, false)
}

#[cfg(any(target_os = "android", test))]
fn is_playlist_downward_pause_swipe(
    delta: egui::Vec2,
    release_velocity: egui::Vec2,
    gesture_duration: std::time::Duration,
) -> bool {
    delta.y >= 72.0
        && delta.y >= delta.x.abs() * 1.75
        && has_vertical_swipe_momentum(delta, release_velocity, gesture_duration, true)
}

#[cfg(any(target_os = "android", test))]
fn has_vertical_swipe_momentum(
    delta: egui::Vec2,
    release_velocity: egui::Vec2,
    gesture_duration: std::time::Duration,
    downward: bool,
) -> bool {
    let release_has_momentum = if downward {
        release_velocity.y >= 300.0 && release_velocity.y >= release_velocity.x.abs() * 1.5
    } else {
        release_velocity.y <= -300.0 && -release_velocity.y >= release_velocity.x.abs() * 1.5
    };
    let average_speed = delta.y.abs() / gesture_duration.as_secs_f32().max(f32::EPSILON);
    release_has_momentum
        || (gesture_duration <= std::time::Duration::from_secs(1) && average_speed >= 300.0)
}

#[cfg(target_os = "android")]
fn set_swiped_playlist_selection(app: &mut EguiFrontendState, swiped_index: usize, selected: bool) {
    let Some(entry) = app
        .controller()
        .state()
        .playlist
        .entries()
        .get(swiped_index)
    else {
        return;
    };
    if entry.selected != selected {
        app.dispatch(PlaylistCommand::ToggleEntrySelection(swiped_index));
        app_log_info!(playlist, "swipe selection applied", swiped_index, selected);
    }
}

#[cfg(target_os = "android")]
fn play_first_selected_playlist_entry(app: &mut EguiFrontendState) {
    let action = {
        let state = app.controller().state();
        playlist_play_first_selected_commands(&state.playlist, state.player.state())
    };
    let Some(action) = action else {
        return;
    };
    app.dispatch_all(action.commands);
    let selected_index = action.selected_index;
    app_log_info!(playlist, "swipe playback started", selected_index);
}

#[cfg(target_os = "android")]
fn scroll_playlist_rows(app: &mut EguiFrontendState, rows: i32) {
    if rows < 0 {
        app.playlist_scroll_offset = app
            .playlist_scroll_offset
            .saturating_sub(rows.unsigned_abs() as usize);
    } else {
        app.playlist_scroll_offset = app
            .playlist_scroll_offset
            .saturating_add(rows as usize)
            .min(app.playlist_max_scroll_offset());
    }
}

pub(crate) fn dispatch_playlist_menu_button(app: &mut EguiFrontendState, menu: PlaylistMenuButton) {
    let menu_name = format!("{menu:?}");
    app_log_info!(playlist, "menu opened", menu_name);
    app.ui.active_overlay = ActiveOverlay::PlaylistMenu(menu);
}

fn show_physical_delete_confirmation(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if app.ui.active_overlay != ActiveOverlay::ConfirmPhysicalDelete {
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
                    app.ui.active_overlay = ActiveOverlay::None;
                }
                if ui.button("Delete").clicked() {
                    app.dispatch(PlaylistCommand::PhysicallyDeleteSelected);
                    app.ui.active_overlay = ActiveOverlay::None;
                }
            });
        });
    if !open {
        app.ui.active_overlay = ActiveOverlay::None;
    }
}

fn add_playlist_menu_popover(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
) {
    let Some(kind) = app.ui.active_overlay.playlist_menu() else {
        return;
    };
    if ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
        app.ui.active_overlay = ActiveOverlay::None;
        return;
    }
    #[cfg(target_os = "android")]
    if kind == PlaylistMenuKind::Misc {
        show_android_playlist_misc_popover(ui.ctx(), app, base_rect);
        return;
    }
    let popup = playlist_menu_popup_rect(kind, app.playlist_width, app.playlist_height);
    let popup_rect = scale_skin_rect(base_rect, popup, app.scale_factor);
    let item_height = 18.0 * app.scale_factor;
    app.ui.playlist_menu_hover = None;
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
            app.ui.playlist_menu_hover = Some((kind, index));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            clicked_item = Some(index);
        }
    }

    #[cfg(target_os = "android")]
    fn show_android_playlist_misc_popover(
        ctx: &egui::Context,
        app: &mut EguiFrontendState,
        playlist_rect: egui::Rect,
    ) {
        let misc_button = playlist_menu_button_rect(
            PlaylistMenuButton::Misc,
            app.playlist_width,
            app.playlist_height,
        );
        let popup_size = egui::vec2(260.0, 184.0);
        let popup_pos = clamp_popup_to_rect(
            egui::pos2(
                playlist_rect.left() + misc_button.x as f32 * app.scale_factor,
                playlist_rect.top() + misc_button.y as f32 * app.scale_factor - popup_size.y,
            ),
            playlist_rect,
            popup_size,
        );
        let mut clicked_item = None;
        let response = egui::Area::new(egui::Id::new("xmms-egui-android-playlist-misc-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .constrain(false)
            .show(ctx, |ui| {
                super::preferences::apply_android_preferences_style(ui);
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(240.0);
                    for (index, label) in ["Sort", "File Info", "Options"].into_iter().enumerate() {
                        if ui
                            .add_sized([ui.available_width(), 56.0], egui::Button::new(label))
                            .clicked()
                        {
                            clicked_item = Some(index);
                        }
                    }
                });
            });

        let clicked_outside = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .or_else(|| input.pointer.latest_pos())
                    .is_some_and(|pos| {
                        let misc_rect =
                            scale_skin_rect(playlist_rect, misc_button, app.scale_factor);
                        !response.response.rect.contains(pos) && !misc_rect.contains(pos)
                    })
        });
        if let Some(index) = clicked_item {
            app.ui.active_overlay = ActiveOverlay::None;
            dispatch_playlist_menu_item(app, PlaylistMenuKind::Misc, index);
        } else if clicked_outside {
            app.ui.active_overlay = ActiveOverlay::None;
        }
    }

    let hover = app
        .ui
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
        app.ui.active_overlay = ActiveOverlay::None;
        dispatch_playlist_menu_item(app, kind, index);
    } else if clicked_outside {
        app.ui.active_overlay = ActiveOverlay::None;
    }
}

pub(crate) fn dispatch_playlist_menu_item(
    app: &mut EguiFrontendState,
    kind: PlaylistMenuKind,
    index: usize,
) {
    match (kind, index) {
        (PlaylistMenuKind::Add, 0) => {
            app.ui.prompt_open = Some(super::menu::EguiPrompt::OpenLocation);
            app.ui.prompt_text.clear();
        }
        (PlaylistMenuKind::Add, 1) => app.apply_effect(AppEffect::OpenFileDialog(
            FileDialogRequest::AddAudioDirectory,
        )),
        (PlaylistMenuKind::Add, 2) => {
            app.apply_effect(AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles));
        }
        (PlaylistMenuKind::Misc, 0) => {
            app.ui.active_overlay = ActiveOverlay::PlaylistSort;
        }
        _ => app.dispatch(PlaylistCommand::ExecuteMenu { kind, index }),
    }
}

fn show_playlist_sort_popover(
    ctx: &egui::Context,
    app: &mut EguiFrontendState,
    playlist_rect: egui::Rect,
) {
    if app.ui.active_overlay != ActiveOverlay::PlaylistSort {
        return;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.ui.active_overlay = ActiveOverlay::None;
        return;
    }

    let misc_button = playlist_menu_button_rect(
        PlaylistMenuButton::Misc,
        app.playlist_width,
        app.playlist_height,
    );
    let estimated_popup_height = if cfg!(target_os = "android") {
        420.0
    } else {
        220.0
    };
    let popup_width = if cfg!(target_os = "android") {
        300.0
    } else {
        200.0
    };
    // egui Areas are clipped to the OS window, which is only as wide/tall as the
    // docked playlist. Anchoring the popover at the Misc button (far right, near
    // the bottom) would push its buttons off-window and make them unclickable
    // (GTK gets away with this because it uses a native top-level popover).
    // Clamp the popover so it stays fully inside the playlist window.
    let popup_pos = clamp_popup_to_rect(
        egui::pos2(
            playlist_rect.left() + misc_button.x as f32 * app.scale_factor,
            playlist_rect.top() + misc_button.y as f32 * app.scale_factor - estimated_popup_height,
        ),
        playlist_rect,
        egui::vec2(popup_width, estimated_popup_height),
    );
    let mut close_after_click = false;
    let response = egui::Area::new(egui::Id::new("xmms-egui-playlist-sort-popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(popup_pos)
        .constrain(false)
        .show(ctx, |ui| {
            #[cfg(target_os = "android")]
            super::preferences::apply_android_preferences_style(ui);
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(if cfg!(target_os = "android") {
                    280.0
                } else {
                    180.0
                });
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
        let pointer_triggered = if cfg!(target_os = "android") {
            input.pointer.any_pressed()
        } else {
            input.pointer.any_released()
        };
        pointer_triggered
            && input
                .pointer
                .interact_pos()
                .or_else(|| input.pointer.latest_pos())
                .is_some_and(|pos| {
                    let misc_rect = scale_skin_rect(playlist_rect, misc_button, app.scale_factor);
                    !response.response.rect.contains(pos) && !misc_rect.contains(pos)
                })
    });
    if close_after_click || clicked_outside {
        app.ui.active_overlay = ActiveOverlay::None;
    }
}

fn playlist_sort_item(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    action: PlaylistSortAction,
    label: &str,
) -> bool {
    #[cfg(target_os = "android")]
    let response = ui.add_sized([ui.available_width(), 56.0], egui::Button::new(label));
    #[cfg(not(target_os = "android"))]
    let response = ui.button(label);
    if !response.clicked() {
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
        let popup_height = 220.0;
        let pos = clamp_popup_to_rect(
            egui::pos2(
                playlist_rect.left() + misc_button.x as f32,
                playlist_rect.top() + misc_button.y as f32 - popup_height,
            ),
            playlist_rect,
            egui::vec2(popup_width, popup_height),
        );
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

        let rows = shared_playlist_rows_render_state(
            app.controller().state(),
            app.playlist_scroll_offset,
            false,
            None,
            app.playlist_width,
            app.playlist_height,
        );
        let expected = crate::app::view_model::format_title_for_preferences(
            "%t (%p)",
            "file:///tmp/song.ogg",
            "Example Artist - Example Title",
            &app.controller().state().config,
        );
        assert_eq!(rows.entries[0].title, expected);
        assert_ne!(rows.entries[0].title, "Example Artist - Example Title");
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
        app.ui.active_overlay = ActiveOverlay::PlaylistSort;
        dispatch_playlist_menu_button(&mut app, PlaylistMenuKind::Select);
        assert_eq!(
            app.ui.active_overlay,
            ActiveOverlay::PlaylistMenu(PlaylistMenuKind::Select)
        );
        dispatch_playlist_menu_item(&mut app, PlaylistMenuKind::Select, 2);
        assert!(app.controller().state().playlist.entries()[0].selected);

        dispatch_playlist_menu_item(&mut app, PlaylistMenuKind::Misc, 0);
        assert_eq!(app.ui.active_overlay, ActiveOverlay::PlaylistSort);

        dispatch_playlist_footer_button(&mut app, PlaylistFooterButton::Play);
        assert_eq!(
            app.controller().state().player.state(),
            crate::player::PlayerState::Playing
        );
    }

    #[test]
    fn egui_queue_controls_and_rows_use_the_shared_store_queue() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        app.controller_mut()
            .state_mut()
            .playlist
            .add_uri("file:///tmp/one.ogg");
        app.controller_mut()
            .state_mut()
            .playlist
            .add_uri("file:///tmp/two.ogg");
        app.dispatch(PlaylistCommand::SetPosition(1));

        assert!(toggle_playlist_queue_targets(&mut app));
        assert_eq!(app.controller().playlist_queue(), vec![1]);

        let rows = shared_playlist_rows_render_state(
            app.controller().state(),
            0,
            false,
            None,
            app.playlist_width,
            app.playlist_height,
        );
        assert_eq!(rows.entries[0].queue_position, None);
        assert_eq!(rows.entries[1].queue_position, Some(0));

        app.dispatch(PlaylistCommand::ClearQueue);
        assert!(app.controller().playlist_queue().is_empty());
    }

    #[test]
    fn android_playlist_swipes_require_directional_horizontal_motion() {
        assert!(is_playlist_right_swipe(egui::vec2(48.0, 0.0)));
        assert!(is_playlist_right_swipe(egui::vec2(80.0, 20.0)));
        assert!(!is_playlist_right_swipe(egui::vec2(47.0, 0.0)));
        assert!(!is_playlist_right_swipe(egui::vec2(-80.0, 0.0)));
        assert!(!is_playlist_right_swipe(egui::vec2(60.0, 50.0)));

        assert!(is_playlist_left_swipe(egui::vec2(-48.0, 0.0)));
        assert!(is_playlist_left_swipe(egui::vec2(-80.0, 20.0)));
        assert!(!is_playlist_left_swipe(egui::vec2(-47.0, 0.0)));
        assert!(!is_playlist_left_swipe(egui::vec2(80.0, 0.0)));
        assert!(!is_playlist_left_swipe(egui::vec2(-60.0, 50.0)));
    }

    #[test]
    fn android_saved_playlist_rows_use_button_height() {
        assert_eq!(
            android_saved_playlist_row_size(320.0),
            egui::vec2(320.0, ANDROID_SAVED_PLAYLIST_ROW_HEIGHT)
        );
        assert_eq!(
            android_saved_playlist_row_size(0.0),
            egui::vec2(1.0, ANDROID_SAVED_PLAYLIST_ROW_HEIGHT)
        );
    }

    #[test]
    fn android_playlist_upward_play_swipe_requires_distance_direction_and_velocity() {
        assert!(is_playlist_upward_play_swipe(
            egui::vec2(10.0, -80.0),
            egui::vec2(40.0, -400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(!is_playlist_upward_play_swipe(
            egui::vec2(0.0, -71.0),
            egui::vec2(0.0, -400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(!is_playlist_upward_play_swipe(
            egui::vec2(60.0, -80.0),
            egui::vec2(40.0, -400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(!is_playlist_upward_play_swipe(
            egui::vec2(0.0, -100.0),
            egui::vec2(0.0, -299.0),
            std::time::Duration::from_secs(2)
        ));
        assert!(!is_playlist_upward_play_swipe(
            egui::vec2(0.0, 100.0),
            egui::vec2(0.0, 400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(is_playlist_upward_play_swipe(
            egui::vec2(0.0, -120.0),
            egui::Vec2::ZERO,
            std::time::Duration::from_millis(100)
        ));
    }

    #[test]
    fn android_playlist_downward_pause_swipe_requires_distance_direction_and_velocity() {
        assert!(is_playlist_downward_pause_swipe(
            egui::vec2(10.0, 80.0),
            egui::vec2(40.0, 400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(!is_playlist_downward_pause_swipe(
            egui::vec2(0.0, 71.0),
            egui::vec2(0.0, 400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(!is_playlist_downward_pause_swipe(
            egui::vec2(60.0, 80.0),
            egui::vec2(40.0, 400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(!is_playlist_downward_pause_swipe(
            egui::vec2(0.0, 100.0),
            egui::vec2(0.0, 299.0),
            std::time::Duration::from_secs(2)
        ));
        assert!(!is_playlist_downward_pause_swipe(
            egui::vec2(0.0, -100.0),
            egui::vec2(0.0, -400.0),
            std::time::Duration::from_millis(500)
        ));
        assert!(is_playlist_downward_pause_swipe(
            egui::vec2(0.0, 120.0),
            egui::Vec2::ZERO,
            std::time::Duration::from_millis(100)
        ));
        assert!(!is_playlist_downward_pause_swipe(
            egui::vec2(0.0, 120.0),
            egui::Vec2::ZERO,
            std::time::Duration::from_secs(2)
        ));
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
