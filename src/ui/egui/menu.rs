//! egui main menu and lightweight prompt/dialog windows.

use crate::app::command::{PlaylistCommand, UiCommand};
use crate::app::view_model::parse_time_ms;

use super::app::EguiFrontendState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiPrompt {
    OpenLocation,
    JumpToTime,
}

impl EguiPrompt {
    fn title(self) -> &'static str {
        match self {
            Self::OpenLocation => "Open Location",
            Self::JumpToTime => "Jump to Time",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::OpenLocation => "file:///path/song.mp3 or https://...",
            Self::JumpToTime => "seconds or mm:ss",
        }
    }
}

pub fn show_main_menu(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if !app.main_menu_open() {
        return;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.dispatch(UiCommand::SetMainMenuVisible(false));
        return;
    }

    let mut close_after_click = false;
    let dropdown_pos = egui::pos2(6.0 * app.scale_factor, 15.0 * app.scale_factor);
    let response = egui::Area::new(egui::Id::new("xmms-egui-main-menu-dropdown"))
        .order(egui::Order::Foreground)
        .fixed_pos(dropdown_pos)
        .constrain(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                if ui.button("Open Files...").clicked() {
                    close_after_click = true;
                    app.dispatch(PlaylistCommand::ExecuteMenu {
                        kind: crate::playlist::PlaylistMenuKind::Add,
                        index: 2,
                    });
                }
                if ui.button("Open Location...").clicked() {
                    close_after_click = true;
                    app.ui.prompt_open = Some(EguiPrompt::OpenLocation);
                    app.ui.prompt_text.clear();
                }
                if ui.button("Preferences").clicked() {
                    close_after_click = true;
                    app.dispatch(UiCommand::SetPreferencesVisible(true));
                }
                if ui.button("Skin Browser").clicked() {
                    close_after_click = true;
                    app.dispatch(UiCommand::SetSkinBrowserVisible(true));
                }
                if ui.button("Skin Editor").clicked() {
                    close_after_click = true;
                    app.runtime
                        .pending_messages
                        .push("skin editor is GTK-only for now".to_string());
                }
                if ui.button("Quit").clicked() {
                    close_after_click = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

    let clicked_outside = ctx.input(|input| {
        input.pointer.any_released()
            && input.pointer.latest_pos().is_some_and(|pos| {
                let menu_button_rect = egui::Rect::from_min_size(
                    egui::pos2(6.0 * app.scale_factor, 3.0 * app.scale_factor),
                    egui::vec2(9.0 * app.scale_factor, 9.0 * app.scale_factor),
                );
                !response.response.rect.contains(pos) && !menu_button_rect.contains(pos)
            })
    });
    if close_after_click || clicked_outside {
        app.dispatch(UiCommand::SetMainMenuVisible(false));
    }
}

pub fn show_prompts(ctx: &egui::Context, app: &mut EguiFrontendState) {
    let Some(prompt) = app.ui.prompt_open else {
        return;
    };
    let mut open = true;
    let mut cancel_requested = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let mut accept_requested = ctx.input(|input| input.key_pressed(egui::Key::Enter));
    egui::Window::new(prompt.title())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.text_edit_singleline(&mut app.ui.prompt_text)
                .on_hover_text(prompt.placeholder());
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel_requested = true;
                }
                if ui.button("OK").clicked() {
                    accept_requested = true;
                }
            });
        });
    if accept_requested {
        accept_prompt(app, prompt);
    } else if cancel_requested || !open {
        app.ui.prompt_open = None;
        app.ui.prompt_text.clear();
    }
}

fn accept_prompt(app: &mut EguiFrontendState, prompt: EguiPrompt) {
    let text = app.ui.prompt_text.trim().to_string();
    if text.is_empty() {
        return;
    }
    match prompt {
        EguiPrompt::OpenLocation => {
            app.dispatch(PlaylistCommand::AddUris(vec![text]));
            app.dispatch(crate::app::command::PlayerCommand::Play);
        }
        EguiPrompt::JumpToTime => {
            if let Some(ms) = parse_time_ms(&text) {
                app.dispatch(crate::app::command::PlayerCommand::SeekToMs(ms));
            } else {
                app.runtime
                    .pending_messages
                    .push(format!("invalid jump time: {text}"));
            }
        }
    }
    app.ui.prompt_open = None;
    app.ui.prompt_text.clear();
}

pub fn show_pending_messages(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if app.runtime.pending_messages.is_empty() {
        return;
    }
    let mut open = true;
    let message = app.runtime.pending_messages.join("\n");
    egui::Window::new("Message")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(message);
            if ui.button("OK").clicked() {
                app.runtime.pending_messages.clear();
            }
        });
    if !open {
        app.runtime.pending_messages.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prompt_times_like_gtk_helpers() {
        assert_eq!(parse_time_ms("42"), Some(42_000));
        assert_eq!(parse_time_ms("1:23"), Some(83_000));
        assert_eq!(parse_time_ms(""), None);
        assert_eq!(parse_time_ms("1:2:3"), None);
        assert_eq!(parse_time_ms("nope"), None);
    }
}
