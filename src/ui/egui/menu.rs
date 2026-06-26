//! egui main menu and lightweight prompt/dialog windows.

use crate::app::command::PlaylistCommand;

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
    if !app.main_menu_open {
        return;
    }
    let mut open = app.main_menu_open;
    let mut close_after_click = false;
    egui::Window::new("XMMS Menu")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(egui::pos2(0.0, 16.0 * app.scale_factor))
        .show(ctx, |ui| {
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
                app.prompt_open = Some(EguiPrompt::OpenLocation);
                app.prompt_text.clear();
            }
            if ui.button("Preferences").clicked() {
                close_after_click = true;
                app.preferences_open = true;
            }
            if ui.button("Skin Browser").clicked() {
                close_after_click = true;
                app.skin_browser_open = true;
            }
            if ui.button("Skin Editor").clicked() {
                close_after_click = true;
                app.runtime
                    .pending_messages
                    .push("skin editor is GTK-only for now".to_string());
            }
            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    app.main_menu_open = open && !close_after_click;
}

pub fn show_prompts(ctx: &egui::Context, app: &mut EguiFrontendState) {
    let Some(prompt) = app.prompt_open else {
        return;
    };
    let mut open = true;
    egui::Window::new(prompt.title())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.text_edit_singleline(&mut app.prompt_text)
                .on_hover_text(prompt.placeholder());
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    app.prompt_open = None;
                    app.prompt_text.clear();
                }
                if ui.button("OK").clicked() {
                    accept_prompt(app, prompt);
                }
            });
        });
    if !open {
        app.prompt_open = None;
        app.prompt_text.clear();
    }
}

fn accept_prompt(app: &mut EguiFrontendState, prompt: EguiPrompt) {
    let text = app.prompt_text.trim().to_string();
    if text.is_empty() {
        return;
    }
    match prompt {
        EguiPrompt::OpenLocation => {
            app.controller_mut().state_mut().playlist.add_uri(text);
            app.dispatch(crate::app::command::PlayerCommand::Play);
        }
        EguiPrompt::JumpToTime => {
            if let Some(ms) = parse_prompt_time_ms(&text) {
                app.dispatch(crate::app::command::PlayerCommand::SeekToMs(ms));
            } else {
                app.runtime
                    .pending_messages
                    .push(format!("invalid jump time: {text}"));
            }
        }
    }
    app.prompt_open = None;
    app.prompt_text.clear();
}

pub fn parse_prompt_time_ms(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Some((minutes, seconds)) = text.split_once(':') {
        let minutes = minutes.trim().parse::<i64>().ok()?;
        let seconds = seconds.trim().parse::<i64>().ok()?;
        return Some((minutes * 60 + seconds).max(0) * 1_000);
    }
    text.parse::<i64>()
        .ok()
        .map(|seconds| seconds.max(0) * 1_000)
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
        assert_eq!(parse_prompt_time_ms("42"), Some(42_000));
        assert_eq!(parse_prompt_time_ms("1:23"), Some(83_000));
        assert_eq!(parse_prompt_time_ms("nope"), None);
    }
}
