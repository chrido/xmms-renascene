//! egui preferences dialog/window.

use std::sync::Arc;

use crate::app::command::UiCommand;
use crate::app::view_model::format_title_for_preferences;
use crate::config::{Config, TimerMode};
use crate::skin::widget::{
    VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode, VisScopeMode, VisVuMode,
};

use super::app::EguiFrontendState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesPage {
    #[default]
    AudioIoPlugins,
    VisualizationPlugins,
    Options,
    Fonts,
    Title,
}

#[derive(Debug, Clone)]
pub struct PreferencesViewportState {
    pub open: bool,
    pub selected_page: PreferencesPage,
    pub config: Config,
    pub changed: bool,
    pub close_requested: bool,
    pub skin_browser_requested: bool,
}

impl PreferencesViewportState {
    pub fn new(config: &Config, selected_page: PreferencesPage, open: bool) -> Self {
        Self {
            open,
            selected_page,
            config: config.clone(),
            changed: false,
            close_requested: false,
            skin_browser_requested: false,
        }
    }
}

pub fn show_preferences(ctx: &egui::Context, app: &mut EguiFrontendState) {
    apply_pending_viewport_state(app);
    sync_viewport_state_from_app(app);

    let state = Arc::clone(&app.preferences_viewport);
    let builder = egui::ViewportBuilder::default()
        .with_title("Preferences")
        .with_inner_size(egui::vec2(560.0, 460.0))
        .with_min_inner_size(egui::vec2(420.0, 300.0))
        .with_resizable(true)
        .with_decorations(true);

    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("xmms-egui-preferences"),
        builder,
        move |ctx, class| {
            let mut state = state.lock().expect("preferences viewport state poisoned");
            if ctx.input(|input| input.viewport().close_requested()) {
                state.open = false;
                state.close_requested = true;
                return;
            }

            let before = state.config.clone();
            match class {
                egui::ViewportClass::EmbeddedWindow | egui::ViewportClass::Root => {
                    show_preferences_embedded(ctx, &mut state);
                }
                egui::ViewportClass::Deferred | egui::ViewportClass::Immediate => {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show_preferences_contents(ui, &mut state));
                }
            }
            if state.config != before {
                state.changed = true;
            }
        },
    );
}

fn sync_viewport_state_from_app(app: &mut EguiFrontendState) {
    let mut state = app
        .preferences_viewport
        .lock()
        .expect("preferences viewport state poisoned");
    if !state.open && app.preferences_open {
        *state = PreferencesViewportState::new(
            &app.controller().state().config,
            app.selected_preferences_page,
            true,
        );
    } else if !app.preferences_open {
        state.open = false;
    }
}

fn apply_pending_viewport_state(app: &mut EguiFrontendState) {
    let mut save_config = false;
    let mut queue_render = false;
    let mut open_skin_browser = false;
    let mut next_open = app.preferences_open;
    let next_page;
    let mut next_config = None;

    {
        let mut state = app
            .preferences_viewport
            .lock()
            .expect("preferences viewport state poisoned");
        next_page = state.selected_page;
        if state.close_requested {
            next_open = false;
            state.close_requested = false;
        }
        if state.changed {
            next_config = Some(state.config.clone());
            save_config = true;
            queue_render = true;
            state.changed = false;
        }
        if state.skin_browser_requested {
            open_skin_browser = true;
            state.skin_browser_requested = false;
        }
    }

    app.dispatch(UiCommand::SetPreferencesVisible(next_open));
    app.selected_preferences_page = next_page;
    if let Some(config) = next_config {
        app.apply_preferences_config(config);
    }
    if open_skin_browser {
        app.dispatch(UiCommand::SetSkinBrowserVisible(true));
    }
    if save_config {
        app.runtime
            .apply_effect(crate::app::effect::AppEffect::SaveConfig);
    }
    if queue_render {
        app.runtime
            .apply_effect(crate::app::effect::AppEffect::QueueRender(
                crate::app::effect::RenderTarget::All,
            ));
    }
}

fn show_preferences_embedded(ctx: &egui::Context, state: &mut PreferencesViewportState) {
    let mut open = state.open;
    egui::Window::new("Preferences")
        .open(&mut open)
        .resizable(true)
        .default_width(520.0)
        .constrain(false)
        .show(ctx, |ui| show_preferences_contents(ui, state));
    state.open = open;
    if !open {
        state.close_requested = true;
    }
}

fn show_preferences_contents(ui: &mut egui::Ui, state: &mut PreferencesViewportState) {
    ui.horizontal_wrapped(|ui| {
        for page in [
            PreferencesPage::AudioIoPlugins,
            PreferencesPage::VisualizationPlugins,
            PreferencesPage::Options,
            PreferencesPage::Fonts,
            PreferencesPage::Title,
        ] {
            if ui
                .selectable_label(state.selected_page == page, page.label())
                .clicked()
            {
                state.selected_page = page;
            }
        }
    });
    ui.separator();
    match state.selected_page {
        PreferencesPage::AudioIoPlugins => show_audio_page(ui, &mut state.config),
        PreferencesPage::VisualizationPlugins => show_visualization_page(ui, &mut state.config),
        PreferencesPage::Options => show_options_page(ui, &mut state.config),
        PreferencesPage::Fonts => show_fonts_page(ui, state),
        PreferencesPage::Title => show_title_page(ui, &mut state.config),
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Reset to Defaults").clicked() {
            state.config = Config::default();
            state.changed = true;
        }
        if ui.button("Close").clicked() {
            state.open = false;
            state.close_requested = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

impl PreferencesPage {
    pub fn label(self) -> &'static str {
        match self {
            Self::AudioIoPlugins => "Audio I/O Plugins",
            Self::VisualizationPlugins => "Visualization Plugins",
            Self::Options => "Options",
            Self::Fonts => "Fonts",
            Self::Title => "Title",
        }
    }
}

fn show_audio_page(ui: &mut egui::Ui, config: &mut Config) {
    ui.heading("Audio I/O Plugins");
    ui.label("Output plugin: GStreamer");
    ui.horizontal(|ui| {
        ui.label("Output device:");
        let mut device = config
            .output_device
            .clone()
            .unwrap_or_else(|| "System default".to_string());
        egui::ComboBox::from_id_salt("egui-output-device")
            .selected_text(&device)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut device, "System default".to_string(), "System default");
            });
        config.output_device = (device != "System default").then_some(device);
    });
    ui.add_enabled(false, egui::Button::new("Configure"))
        .on_disabled_hover_text(
            "GStreamer output configuration is handled by the system for egui.",
        );
    ui.separator();
    ui.label("Input plugins are handled by GStreamer and playlist import helpers.");
}

fn show_visualization_page(ui: &mut egui::Ui, config: &mut Config) {
    ui.heading("Visualization Plugins");
    combo(
        ui,
        "Visualization mode",
        &mut config.vis_mode,
        &[
            (VisMode::Analyzer, "Analyzer"),
            (VisMode::Scope, "Scope"),
            (VisMode::Off, "Off"),
        ],
    );
    combo(
        ui,
        "Analyzer mode",
        &mut config.vis_analyzer_mode,
        &[
            (VisAnalyzerMode::Normal, "Normal"),
            (VisAnalyzerMode::Fire, "Fire"),
            (VisAnalyzerMode::VerticalLines, "Vertical lines"),
        ],
    );
    combo(
        ui,
        "Analyzer style",
        &mut config.vis_analyzer_style,
        &[
            (VisAnalyzerStyle::Bars, "Bars"),
            (VisAnalyzerStyle::Lines, "Lines"),
        ],
    );
    combo(
        ui,
        "Scope mode",
        &mut config.vis_scope_mode,
        &[
            (VisScopeMode::Dot, "Dot"),
            (VisScopeMode::Line, "Line"),
            (VisScopeMode::Solid, "Solid"),
        ],
    );
    ui.checkbox(&mut config.vis_peaks_enabled, "Peaks");
    combo(
        ui,
        "Analyzer falloff",
        &mut config.vis_analyzer_falloff,
        &falloff_options(),
    );
    combo(
        ui,
        "Peaks falloff",
        &mut config.vis_peaks_falloff,
        &falloff_options(),
    );
    combo(
        ui,
        "WindowShade VU mode",
        &mut config.vis_vu_mode,
        &[(VisVuMode::Normal, "Normal"), (VisVuMode::Smooth, "Smooth")],
    );
    ui.add(egui::Slider::new(&mut config.vis_refresh_divisor, 1..=4).text("Refresh divisor"));
    ui.add(egui::Slider::new(&mut config.vis_falloff, 0.001..=0.25).text("Sensitivity/falloff"));
}

fn show_options_page(ui: &mut egui::Ui, config: &mut Config) {
    ui.heading("Options");
    ui.add(egui::Slider::new(&mut config.volume, 0..=100).text("Volume"));
    ui.add(egui::Slider::new(&mut config.balance, -100..=100).text("Balance"));
    let mut scale_factor = config.scale_factor.clamp(1.0, 5.0);
    ui.add(
        egui::Slider::new(&mut scale_factor, 1.0..=5.0)
            .text("Zoom level")
            .suffix("x"),
    );
    config.scale_factor = scale_factor;
    config.doublesize = scale_factor > 1.0;
    ui.add(
        egui::Slider::new(&mut config.pause_between_songs_time, 0..=30)
            .text("Pause between songs seconds"),
    );
    ui.add(
        egui::Slider::new(&mut config.mouse_wheel_change, 1..=25).text("Mouse wheel volume step"),
    );
    ui.checkbox(&mut config.repeat, "Repeat");
    ui.checkbox(&mut config.shuffle, "Shuffle");
    ui.checkbox(&mut config.no_playlist_advance, "No playlist advance");
    ui.checkbox(&mut config.pause_between_songs, "Pause between songs");
    ui.checkbox(&mut config.stop_with_fadeout, "Stop with fadeout");
    let mut remaining = config.timer_mode == TimerMode::Remaining;
    if ui.checkbox(&mut remaining, "Time remaining").changed() {
        config.timer_mode = if remaining {
            TimerMode::Remaining
        } else {
            TimerMode::Elapsed
        };
    }
    ui.checkbox(&mut config.playlist_visible, "Show playlist");
    ui.checkbox(&mut config.equalizer_visible, "Show equalizer");
    ui.checkbox(&mut config.playlist_detached, "Detach playlist");
    ui.checkbox(&mut config.equalizer_detached, "Detach equalizer");
    ui.checkbox(&mut config.convert_twenty, "Convert %20 to space");
    ui.checkbox(
        &mut config.convert_underscore,
        "Convert underscore to space",
    );
    ui.checkbox(&mut config.show_numbers_in_pl, "Show numbers in playlist");
    ui.checkbox(
        &mut config.vim_playlist_navigation,
        "Vim-style playlist navigation",
    );
}

fn show_fonts_page(ui: &mut egui::Ui, state: &mut PreferencesViewportState) {
    ui.heading("Fonts");
    ui.horizontal(|ui| {
        ui.label("Playlist font:");
        ui.text_edit_singleline(&mut state.config.playlist_font);
    });
    ui.label("Main window text uses the active skin bitmap font.");
    ui.horizontal(|ui| {
        ui.label("Main window font:");
        ui.add_enabled(
            false,
            egui::TextEdit::singleline(&mut state.config.mainwin_font),
        );
    });
    if ui.button("Open Skin Browser").clicked() {
        state.skin_browser_requested = true;
    }
}

fn show_title_page(ui: &mut egui::Ui, config: &mut Config) {
    ui.heading("Title");
    ui.label("Title format");
    ui.text_edit_singleline(&mut config.title_format);
    ui.label(
        "Tokens: %p performer, %a album, %t title, %n track number, %f filename, %F full path/URI.",
    );
    let preview = format_title_for_preferences(
        &config.title_format,
        "file:///tmp/Example_Artist%20-%20Example_Title.mp3",
        "Example Artist - Example Title",
        config,
    );
    ui.label(format!("Preview: {preview}"));
}

fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    let selected = options
        .iter()
        .find(|(candidate, _)| candidate == value)
        .map(|(_, label)| *label)
        .unwrap_or("Unknown");
    egui::ComboBox::from_label(label)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (candidate, label) in options {
                ui.selectable_value(value, *candidate, *label);
            }
        });
}

fn falloff_options() -> [(VisFalloffSpeed, &'static str); 5] {
    [
        (VisFalloffSpeed::Slowest, "Slowest"),
        (VisFalloffSpeed::Slow, "Slow"),
        (VisFalloffSpeed::Medium, "Medium"),
        (VisFalloffSpeed::Fast, "Fast"),
        (VisFalloffSpeed::Fastest, "Fastest"),
    ]
}

pub fn page_labels() -> Vec<&'static str> {
    [
        PreferencesPage::AudioIoPlugins,
        PreferencesPage::VisualizationPlugins,
        PreferencesPage::Options,
        PreferencesPage::Fonts,
        PreferencesPage::Title,
    ]
    .into_iter()
    .map(PreferencesPage::label)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_pages_have_stable_labels() {
        assert_eq!(
            page_labels(),
            vec![
                "Audio I/O Plugins",
                "Visualization Plugins",
                "Options",
                "Fonts",
                "Title"
            ]
        );
    }
}
