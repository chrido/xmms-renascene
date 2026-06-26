use std::cell::{Cell, RefCell};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use gtk::prelude::*;

use crate::app_state::AppState;
use crate::config::{Config, TimerMode};
use crate::equalizer::{
    default_equalizer_presets, find_preset, import_winamp_eqf, load_preset_store,
    load_winamp_eqf_first, load_xmms_preset_file, preset_store_path, remove_presets,
    save_preset_store, save_winamp_eqf, save_xmms_preset_file, sort_presets, upsert_preset,
    winamp_original_presets, EqualizerPreset,
};
use crate::mpris::{
    gio_service::MprisService, playback_status as mpris_playback_status, MprisCommand, MprisEvent,
    MprisMetadata, MprisPlayerProperties, MprisRootProperties,
};
use crate::player::{
    equalizer_position_to_db, group_output_devices, list_gstreamer_output_devices,
    GStreamerBackend, OutputDevice, OutputDeviceGroups, OutputDeviceSelection, PlaybackEvent,
    PlayerState,
};
use crate::playlist::{file_uri_to_path, DurationIndexResult, Playlist, PlaylistSortKey};
use crate::render::{
    blit_surface_rect, docked_panel_size, equalizer_window_height, main_window_height,
    paint_scaled, playlist_window_height, render_equalizer_state, render_main_player_state,
    render_playlist_frame, render_playlist_menu, render_playlist_rows, render_scaled, scale_dim,
    surface_from_xpm, DockedPanelState, EqualizerControl, EqualizerRenderState, MainPushButton,
    MainSlider, MainToggleButton, MainWindowRenderState, PlaylistMenuRenderKind,
    PlaylistMenuRenderState, PlaylistRowRenderEntry, PlaylistRowsRenderState, RenderPass,
    VisualizationRenderState, EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH,
    MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH, PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH, PLAYLIST_MIN_HEIGHT, PLAYLIST_MIN_WIDTH,
};
use crate::session::{
    default_config_dir, fallback_state_paths, load_saved_state, save_fallback_state,
};
use crate::skin::layout::{
    equalizer_control_at, equalizer_shaded_slider_at, equalizer_slider_at, equalizer_slider_layout,
    main_push_button_rect, main_slider_layout, main_toggle_button_rect, panel_title_button_at,
    playlist_footer_button_at, playlist_menu_button_at, playlist_menu_popup_rect,
    snap_playlist_size, EqualizerSlider, LayoutPanelKind as LayoutPanel, PanelTitleButton,
    PlaylistFooterButton, PlaylistMenuButton, SkinRect,
};
use crate::skin::widget::{
    NumberDisplay, PlayStatusValue, VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode,
    VisScopeMode, VisVuMode, Visualization, WidgetId,
};
use crate::skin::{
    discover_skins_in_dirs, skin_browser_search_dirs, DefaultSkin, SkinEntry, SkinPixmapKind,
};
use crate::skineditor::{
    ElementSlot, SkinEditorState, SkinGradient, Tool, COLOR_SHELF_SIZE, GRADIENT_SHELF_SIZE,
    MAX_ZOOM, MIN_ZOOM, ZOOM_STEP,
};

pub(crate) mod file_info;
mod style;

use file_info::{file_info_details_for_entry, show_file_info_dialog, FileInfoDetails};
use style::{
    refresh_xmms_skin_css, style_color_shelf_button, style_skin_color_button,
    style_skin_editor_custom_color_button,
};

const DEFAULT_SCALE: i32 = 2;
const STOP_FADE_DURATION_MS: i64 = 1_000;
type PreferencesChanged = Rc<dyn Fn()>;
const PREFERENCES_VOLUME_WIDGET: &str = "xmms-preferences-volume";
const PREFERENCES_BALANCE_WIDGET: &str = "xmms-preferences-balance";
const SKIN_BROWSER_ROOT_WIDGET: &str = "xmms-skin-browser-root";
const SKIN_BROWSER_HEADER_WIDGET: &str = "xmms-skin-browser-header";
const SKIN_BROWSER_LIST_WIDGET: &str = "xmms-skin-browser-list";
const SKIN_BROWSER_ADD_WIDGET: &str = "xmms-skin-browser-add";
const SKIN_BROWSER_CLOSE_WIDGET: &str = "xmms-skin-browser-close";
const SKIN_EDITOR_COLOR_SHELF_COLUMNS: usize = 8;
const SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE: i32 = 34;
const SKIN_EDITOR_COLOR_SHELF_GAP: i32 = 4;
const SKIN_EDITOR_SIDEBAR_WIDTH: i32 = SKIN_EDITOR_COLOR_SHELF_COLUMNS as i32
    * SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE
    + (SKIN_EDITOR_COLOR_SHELF_COLUMNS as i32 - 1) * SKIN_EDITOR_COLOR_SHELF_GAP;
const SKIN_EDITOR_GRADIENT_WIDTH: i32 = SKIN_EDITOR_SIDEBAR_WIDTH;
const SKIN_EDITOR_GRADIENT_HEIGHT: i32 = 34;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreviewOptions {
    pub show_playlist: bool,
    pub show_equalizer: bool,
    pub main_shaded: Option<bool>,
    pub playlist_shaded: Option<bool>,
    pub equalizer_shaded: Option<bool>,
    pub playlist_detached: Option<bool>,
    pub equalizer_detached: Option<bool>,
    pub playlist_size: Option<(i32, i32)>,
    pub reset: bool,
    pub open_preferences: bool,
    pub open_skin_editor: bool,
    pub skin_path: Option<String>,
    pub screenshot_path: Option<String>,
    pub scale_factor: Option<String>,
}

pub fn run_default_skin_preview(options: PreviewOptions) {
    run_preview_application(PreviewMode::Interactive, options);
}

pub fn run_default_skin_preview_smoke(options: PreviewOptions) {
    run_preview_application(PreviewMode::Smoke, options);
}

pub fn write_player_screenshot(options: PreviewOptions, path: &Path) -> Result<(), String> {
    let state = preview_state_from_options(options)?;
    let docked_state = state.docked_panel_state();
    let (width, height) = docked_panel_size(docked_state);
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|err| format!("failed to create screenshot surface: {err}"))?;
    let cr = cairo::Context::new(&surface)
        .map_err(|err| format!("failed to create screenshot context: {err}"))?;
    render_docked_ui_state(&cr, state.active_skin(), &state, RenderPass::Bitmap)
        .map_err(|err| format!("failed to render screenshot: {err}"))?;
    render_docked_ui_state(&cr, state.active_skin(), &state, RenderPass::Text)
        .map_err(|err| format!("failed to render screenshot: {err}"))?;
    drop(cr);
    write_surface_png(&mut surface, path)
        .map_err(|err| format!("failed to write screenshot '{}': {err}", path.display()))
}

enum PreviewMode {
    Interactive,
    Smoke,
}

fn run_preview_application(mode: PreviewMode, options: PreviewOptions) {
    let mut flags = gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    if std::env::var_os("XMMS_NON_UNIQUE").is_some() {
        flags |= gtk::gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = gtk::Application::builder()
        .application_id("org.xmms.Renascene.RustPreview")
        .flags(flags)
        .register_session(true)
        .build();

    app.connect_command_line(|app, _cmdline| {
        app.activate();
        gtk::glib::ExitCode::SUCCESS
    });

    app.connect_activate(move |app| {
        let persist_session = matches!(mode, PreviewMode::Interactive);
        if let Err(err) = build_preview_window(app, options.clone(), persist_session) {
            eprintln!("xmms-rs: failed to create GTK preview: {err}");
            app.quit();
            return;
        }

        if matches!(mode, PreviewMode::Smoke) {
            let app = app.clone();
            gtk::glib::idle_add_local_once(move || app.quit());
        }
    });

    app.run_with_args(&["xmms-rs"]);
}

fn build_preview_window(
    app: &gtk::Application,
    options: PreviewOptions,
    persist_session: bool,
) -> Result<(), String> {
    let (config_path, playlist_path) = fallback_state_paths(&default_config_dir());
    let app_state = if persist_session {
        load_saved_state(&config_path, &playlist_path, options.reset)
            .map_err(|err| format!("failed to load saved state: {err}"))?
    } else {
        AppState::default()
    };
    let open_preferences = options.open_preferences;
    let open_skin_editor = options.open_skin_editor;
    let mut state = preview_state_from_app_state(app_state, options)?;
    if let Some(config_dir) = config_path.parent() {
        state.set_equalizer_preset_dir(config_dir.to_path_buf());
    }
    match GStreamerBackend::new() {
        Ok(backend) => state.set_playback_backend(Rc::new(RefCell::new(backend))),
        Err(err) => eprintln!("xmms-rs: audio playback backend unavailable: {err}"),
    }
    let (initial_width, initial_height) = state.docked_panel_size();
    let initial_scale = state.scale_factor();
    let initial_device_width = scale_dim(initial_width, initial_scale);
    let initial_device_height = scale_dim(initial_height, initial_scale);
    let main_state = Rc::new(RefCell::new(state));
    refresh_xmms_skin_css(main_state.borrow().active_skin());

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Renascene Rust Preview")
        .resizable(false)
        .decorated(false)
        .default_width(initial_device_width)
        .default_height(initial_device_height)
        .build();
    if persist_session {
        let main_state = Rc::clone(&main_state);
        let config_path = config_path.clone();
        let playlist_path = playlist_path.clone();
        window.connect_close_request(move |_| {
            if let Err(err) = main_state
                .borrow_mut()
                .save_runtime_snapshot(&config_path, &playlist_path)
            {
                eprintln!("xmms-rs: failed to save session: {err}");
            }
            gtk::glib::Propagation::Proceed
        });
    }
    if persist_session {
        let main_state = Rc::clone(&main_state);
        let config_path = config_path.clone();
        let playlist_path = playlist_path.clone();
        app.connect_shutdown(move |_| {
            if let Err(err) = main_state
                .borrow_mut()
                .save_runtime_snapshot(&config_path, &playlist_path)
            {
                eprintln!("xmms-rs: failed to save session: {err}");
            }
        });
    }

    let drawing_area = gtk::DrawingArea::builder()
        .content_width(initial_device_width)
        .content_height(initial_device_height)
        .focusable(true)
        .build();
    let panel_windows = Rc::new(PanelWindows::new(app, &main_state, &drawing_area, &window));
    let mpris_service = Rc::new(MprisService::own_session_bus(Rc::clone(&main_state)));
    sync_panel_windows(&panel_windows, &main_state.borrow());
    resize_main_window(&window, &drawing_area, &main_state.borrow());
    let menu_popover = Rc::new(build_main_menu_popover(
        app,
        &window,
        &drawing_area,
        &panel_windows.preferences,
        &panel_windows.open_location,
        &panel_windows.skin_browser,
        &panel_windows.skin_editor,
        &main_state,
    ));
    let playlist_sort_popover = Rc::new(build_playlist_sort_popover(
        &drawing_area,
        &main_state,
        &drawing_area,
    ));
    let equalizer_presets_popover = Rc::new(build_equalizer_presets_popover(
        &drawing_area,
        &main_state,
        &drawing_area,
    ));

    {
        let main_state = Rc::clone(&main_state);
        drawing_area.set_draw_func(move |_area, cr, width, height| {
            let state = main_state.borrow();
            let docked_state = state.docked_panel_state();
            let (base_width, base_height) = docked_panel_size(docked_state);
            if let Err(err) =
                render_scaled(cr, width, height, base_width, base_height, |cr, pass| {
                    render_docked_ui_state(cr, state.active_skin(), &state, pass).map(|_| ())
                })
            {
                eprintln!("xmms-rs: failed to render main-window preview: {err}");
            }
        });
    }

    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        let panel_windows = Rc::clone(&panel_windows);
        click.connect_pressed(move |gesture, n_press, x, y| {
            let (base_x, base_y) = event_to_base_coords(&drawing_area, &main_state.borrow(), x, y);
            let docked_panel = { main_state.borrow().docked_panel_at(base_x, base_y) };
            if let Some((kind, panel_x, panel_y)) = docked_panel {
                main_state.borrow_mut().select_docked_panel(kind);
                if n_press >= 2
                    && kind == PanelKind::Equalizer
                    && main_state
                        .borrow()
                        .panel_title_drag_region(kind, panel_x, panel_y)
                {
                    main_state.borrow_mut().toggle_equalizer_shaded();
                    sync_panel_windows(&panel_windows, &main_state.borrow());
                    resize_main_window(&window, &drawing_area, &main_state.borrow());
                    drawing_area.queue_draw();
                    return;
                }
                match kind {
                    PanelKind::Equalizer => {
                        if main_state.borrow_mut().equalizer_press(panel_x, panel_y) {
                            drawing_area.queue_draw();
                        }
                    }
                    PanelKind::Playlist => {
                        if n_press >= 2
                            && main_state
                                .borrow_mut()
                                .activate_playlist_entry_at(panel_x, panel_y)
                        {
                            drawing_area.queue_draw();
                            return;
                        }
                        let ctrl_pressed = gesture
                            .current_event_state()
                            .contains(gtk::gdk::ModifierType::CONTROL_MASK);
                        let pressed = main_state.borrow_mut().playlist_press_with_ctrl(
                            panel_x,
                            panel_y,
                            ctrl_pressed,
                        ) || main_state
                            .borrow_mut()
                            .playlist_scrollbar_press(panel_x, panel_y);
                        if pressed {
                            drawing_area.queue_draw();
                        }
                        if !pressed
                            && main_state.borrow().playlist_resize_region(panel_x, panel_y)
                            && main_state
                                .borrow_mut()
                                .begin_docked_playlist_resize(panel_y)
                        {
                            drawing_area.queue_draw();
                        }
                    }
                }
                return;
            }
            main_state.borrow_mut().select_docked_main();
            drawing_area.queue_draw();
            if main_state.borrow().main_title_drag_region(base_x, base_y) {
                let Some(device) = gesture.current_event_device() else {
                    return;
                };
                let Some(surface) = window.surface() else {
                    return;
                };
                let Ok(toplevel) = surface.downcast::<gtk::gdk::Toplevel>() else {
                    return;
                };
                toplevel.begin_move(
                    &device,
                    gesture.current_button() as i32,
                    x,
                    y,
                    gesture.current_event_time(),
                );
                return;
            }
            main_state.borrow_mut().press(base_x, base_y);
            sync_preferences_options_controls(&panel_windows.preferences, &main_state);
            drawing_area.queue_draw();
        });
    }
    {
        let app = app.clone();
        let window = window.clone();
        let drawing_area = drawing_area.clone();
        let menu_popover = Rc::clone(&menu_popover);
        let playlist_sort_popover = Rc::clone(&playlist_sort_popover);
        let equalizer_presets_popover = Rc::clone(&equalizer_presets_popover);
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        click.connect_released(move |_gesture, _n_press, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, &main_state.borrow(), x, y);
            if main_state.borrow_mut().end_docked_playlist_resize() {
                sync_panel_windows(&panel_windows, &main_state.borrow());
                resize_main_window(&window, &drawing_area, &main_state.borrow());
                drawing_area.queue_draw();
                return;
            }
            let docked_panel = { main_state.borrow().docked_panel_at(x, y) };
            if let Some((kind, panel_x, panel_y)) = docked_panel {
                let action = {
                    let mut state = main_state.borrow_mut();
                    match kind {
                        PanelKind::Equalizer => {
                            let title_action = state.panel_click(kind, panel_x, panel_y);
                            if title_action == PanelAction::None {
                                state.equalizer_release(panel_x, panel_y)
                            } else {
                                title_action
                            }
                        }
                        PanelKind::Playlist => {
                            if state.playlist_scrollbar_release() {
                                PanelAction::Changed
                            } else if state.playlist_menu_pressed() {
                                state.playlist_release(panel_x, panel_y)
                            } else if state.playlist_entry_release() {
                                PanelAction::Changed
                            } else {
                                state.panel_click(kind, panel_x, panel_y)
                            }
                        }
                    }
                };
                handle_panel_action_for_main_window(
                    action,
                    &window,
                    &drawing_area,
                    &panel_windows,
                    &main_state,
                    &playlist_sort_popover,
                    &equalizer_presets_popover,
                );
                drawing_area.queue_draw();
                return;
            }
            let action = main_state.borrow_mut().release(x, y);
            apply_ui_action(
                action,
                &app,
                &window,
                &drawing_area,
                &menu_popover,
                &main_state,
            );
            sync_preferences_options_controls(&panel_windows.preferences, &main_state);
            sync_panel_windows(&panel_windows, &main_state.borrow());
            resize_main_window(&window, &drawing_area, &main_state.borrow());
            drawing_area.queue_draw();
        });
    }
    window.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    let main_hover_base = Rc::new(Cell::new(None::<(i32, i32)>));
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let main_hover_base = Rc::clone(&main_hover_base);
        motion.connect_motion(move |_motion, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, &main_state.borrow(), x, y);
            main_hover_base.set(Some((x, y)));
            if main_state.borrow().is_docked_playlist_resizing() {
                if main_state.borrow_mut().docked_playlist_resize_motion(y) {
                    sync_panel_windows(&panel_windows, &main_state.borrow());
                    resize_main_window(&window, &drawing_area, &main_state.borrow());
                    drawing_area.queue_draw();
                }
                return;
            }
            let docked_panel = { main_state.borrow().docked_panel_at(x, y) };
            if let Some((kind, panel_x, panel_y)) = docked_panel {
                let changed = match kind {
                    PanelKind::Equalizer => {
                        main_state.borrow_mut().equalizer_motion(panel_x, panel_y)
                    }
                    PanelKind::Playlist => {
                        let scrolled = main_state
                            .borrow_mut()
                            .playlist_scrollbar_motion(panel_x, panel_y);
                        let menu_changed =
                            main_state.borrow_mut().playlist_motion(panel_x, panel_y);
                        scrolled || menu_changed
                    }
                };
                if changed {
                    drawing_area.queue_draw();
                }
                return;
            }
            if main_state.borrow_mut().motion(x, y) {
                sync_preferences_options_controls(&panel_windows.preferences, &main_state);
                drawing_area.queue_draw();
            }
        });
    }
    window.add_controller(motion);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let main_hover_base = Rc::clone(&main_hover_base);
        scroll.connect_scroll(move |scroll, _dx, dy| {
            let hover = main_hover_base.get().or_else(|| {
                scroll
                    .current_event()
                    .and_then(|event| event.position())
                    .map(|(event_x, event_y)| {
                        event_to_base_coords(&drawing_area, &main_state.borrow(), event_x, event_y)
                    })
            });
            let Some((x, y)) = hover else {
                return gtk::glib::Propagation::Proceed;
            };
            if main_state.borrow_mut().scroll_main(x, y, dy) {
                sync_panel_windows(&panel_windows, &main_state.borrow());
                resize_main_window(&window, &drawing_area, &main_state.borrow());
                drawing_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
                panel_windows.equalizer_area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(scroll);

    add_file_drop_controller(&drawing_area, Rc::clone(&main_state), true, true);

    let key_controller = gtk::EventControllerKey::new();
    {
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let window = window.clone();
        let drawing_area = drawing_area.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            if focus_cycle_shortcut(key, state) {
                main_state.borrow_mut().cycle_visible_focus();
                drawing_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
                panel_windows.equalizer_area.queue_draw();
                return gtk::glib::Propagation::Stop;
            }
            if handle_main_playlist_key_pressed(&main_state, key, state) {
                drawing_area.queue_draw();
                sync_panel_windows(&panel_windows, &main_state.borrow());
                resize_main_window(&window, &drawing_area, &main_state.borrow());
                return gtk::glib::Propagation::Stop;
            }
            let Some(shortcut) = keyboard_shortcut_from_event(key, state) else {
                return gtk::glib::Propagation::Proceed;
            };
            handle_keyboard_shortcut(
                shortcut,
                &window,
                &drawing_area,
                &panel_windows,
                &main_state,
            );
            gtk::glib::Propagation::Stop
        });
    }
    window.add_controller(key_controller);

    {
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        window.connect_is_active_notify(move |window| {
            if window.is_active() {
                let mut state = main_state.borrow_mut();
                state.set_panel_focused(PanelKind::Equalizer, false);
                state.set_panel_focused(PanelKind::Playlist, false);
                panel_windows.equalizer_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
            }
        });
    }

    {
        let drawing_area = drawing_area.clone();
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let mpris_service = Rc::clone(&mpris_service);
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            let (redraw, mpris_events, mpris_properties) = {
                let mut state = main_state.borrow_mut();
                let redraw = state.update_timer_tick(100);
                let events = state.take_mpris_events();
                let properties = state.mpris_player_properties();
                (redraw, events, properties)
            };
            mpris_service.emit_events(&mpris_events, &mpris_properties);
            if redraw {
                drawing_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
                panel_windows.equalizer_area.queue_draw();
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    window.set_child(Some(&drawing_area));
    window.present();
    present_visible_panel_windows(&panel_windows, &main_state.borrow());
    if open_preferences {
        main_state.borrow_mut().set_preferences_visible(true);
        panel_windows.preferences.present();
    }
    if open_skin_editor {
        main_state.borrow_mut().set_skin_editor_visible(true);
        panel_windows.skin_editor.present();
    }
    Ok(())
}

fn apply_skinned_window_chrome(
    window: &impl IsA<gtk::Window>,
    title: &str,
    extra_css_classes: &[&str],
) {
    window.as_ref().add_css_class("xmms-skinned-window");
    for class in extra_css_classes {
        window.as_ref().add_css_class(class);
    }
    set_skinned_window_titlebar(window, title, extra_css_classes);
}

pub(super) fn set_skinned_window_titlebar(
    window: &impl IsA<gtk::Window>,
    title: &str,
    extra_css_classes: &[&str],
) {
    let titlebar = gtk::HeaderBar::new();
    titlebar.add_css_class("xmms-skinned-window");
    titlebar.add_css_class("xmms-skinned-window-titlebar");
    for class in extra_css_classes {
        titlebar.add_css_class(class);
    }
    titlebar.set_show_title_buttons(true);
    titlebar.set_decoration_layout(Some(":close"));
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("xmms-skinned-window-title");
    titlebar.set_title_widget(Some(&title_label));
    window.as_ref().set_titlebar(Some(&titlebar));
}

fn skinned_application_window(
    app: &gtk::Application,
    title: &str,
    default_width: i32,
    default_height: i32,
    extra_css_classes: &[&str],
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();
    apply_skinned_window_chrome(&window, title, extra_css_classes);
    window
}

pub(super) fn skinned_window(
    title: &str,
    default_width: i32,
    default_height: i32,
    extra_css_classes: &[&str],
) -> gtk::Window {
    let window = gtk::Window::builder()
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();
    apply_skinned_window_chrome(&window, title, extra_css_classes);
    window
}

fn load_skin_from_config(config: &Config) -> io::Result<DefaultSkin> {
    match config.skin.as_deref() {
        Some(path) => DefaultSkin::load_from_path(Path::new(path)),
        None => DefaultSkin::load_bundled(),
    }
}

fn preview_state_from_options(options: PreviewOptions) -> Result<MainWindowUiState, String> {
    preview_state_from_app_state(AppState::default(), options)
}

fn preview_state_from_app_state(
    mut app_state: AppState,
    options: PreviewOptions,
) -> Result<MainWindowUiState, String> {
    if options.reset {
        app_state = AppState::default();
    }
    if options.show_playlist || options.playlist_size.is_some() {
        app_state.config.playlist_visible = true;
    }
    if options.show_equalizer {
        app_state.config.equalizer_visible = true;
    }
    if let Some(shaded) = options.main_shaded {
        app_state.config.main_shaded = shaded;
    }
    if let Some(shaded) = options.playlist_shaded {
        app_state.config.playlist_shaded = shaded;
    }
    if let Some(shaded) = options.equalizer_shaded {
        app_state.config.equalizer_shaded = shaded;
    }
    if let Some(detached) = options.playlist_detached {
        app_state.config.playlist_detached = detached;
    }
    if let Some(detached) = options.equalizer_detached {
        app_state.config.equalizer_detached = detached;
    }
    if let Some(skin_path) = options.skin_path.as_ref() {
        app_state.config.skin = Some(skin_path.clone());
    }
    if let Some(scale_factor) = options.scale_factor.as_ref() {
        app_state.config.scale_factor = scale_factor
            .parse::<f64>()
            .map_err(|_| format!("invalid scale factor '{scale_factor}'"))?
            .clamp(1.0, 5.0);
        app_state.config.doublesize = app_state.config.scale_factor > 1.0;
    }

    let mut state = MainWindowUiState::from_app_state(app_state);
    if let Some((width, height)) = options.playlist_size {
        state.set_playlist_size(width, height);
    }
    if let Some(skin_path) = options.skin_path.as_ref() {
        state
            .load_configured_skin()
            .map_err(|err| format!("failed to load skin '{}': {err}", skin_path))?;
    }
    Ok(state)
}

fn write_surface_png(surface: &mut cairo::ImageSurface, path: &Path) -> io::Result<()> {
    surface.flush();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let width = surface.width() as u32;
    let height = surface.height() as u32;
    let stride = surface.stride() as usize;
    let data = surface
        .data()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);

    for y in 0..height as usize {
        let row = &data[y * stride..][..width as usize * 4];
        for pixel in row.chunks_exact(4) {
            let argb = u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]);
            rgba.push(((argb >> 16) & 0xff) as u8);
            rgba.push(((argb >> 8) & 0xff) as u8);
            rgba.push((argb & 0xff) as u8);
            rgba.push(((argb >> 24) & 0xff) as u8);
        }
    }

    image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| io::Error::other("invalid screenshot pixel buffer"))?
        .save(path)
        .map_err(io::Error::other)
}

fn style_xmms_popover(popover: &gtk::Popover) {
    popover.add_css_class("xmms-menu-popover");
}

fn xmms_menu_box(spacing: i32) -> gtk::Box {
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, spacing);
    menu_box.add_css_class("xmms-menu-box");
    menu_box
}

fn xmms_menu_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_halign(gtk::Align::Fill);
    button.add_css_class("xmms-menu-button");
    button
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainKeyboardShortcut {
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    OpenFiles,
    ToggleRepeat,
    ToggleShuffle,
    Preferences,
    OpenLocation,
    ToggleNoAdvance,
    ShadeMain,
    JumpTime,
    SkinBrowser,
    OpenDirectory,
    PresentMain,
    TogglePlaylist,
    ToggleEqualizer,
    ShadePlaylist,
    ShadeEqualizer,
    ToggleTimerRemaining,
    ToggleSticky,
    DoubleScale,
    HalfScale,
    ToggleEasyMove,
    StartOfList,
    FileInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
}

impl ArrowKey {
    fn from_gdk(key: gtk::gdk::Key) -> Option<Self> {
        match key {
            gtk::gdk::Key::Up | gtk::gdk::Key::KP_Up => Some(Self::Up),
            gtk::gdk::Key::Down | gtk::gdk::Key::KP_Down => Some(Self::Down),
            gtk::gdk::Key::Left | gtk::gdk::Key::KP_Left => Some(Self::Left),
            gtk::gdk::Key::Right | gtk::gdk::Key::KP_Right => Some(Self::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyCommand {
    Volume(i32),
    Balance(i32),
    Seek(i32),
    PreviousTrack,
    NextTrack,
    PlaylistMove(isize),
    EqualizerAdjust(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferencesPage {
    Audio,
    Visualization,
    Options,
    Fonts,
    Title,
}

pub fn preferences_window_default_size() -> (i32, i32) {
    (560, 680)
}

pub fn preferences_zoom_spans_full_width() -> bool {
    true
}

pub fn preferences_page_parity_controls(page: PreferencesPage) -> &'static [&'static str] {
    match page {
        PreferencesPage::Audio => &[
            "Input Plugins",
            "Output Plugin",
            "Output device:",
            "Configure",
        ],
        PreferencesPage::Visualization => &[
            "Visualization mode:",
            "Analyzer mode:",
            "Analyzer style:",
            "Scope mode:",
            "Show analyzer peaks",
            "Analyzer falloff:",
            "Peaks falloff:",
            "WindowShade VU mode:",
            "Refresh rate:",
        ],
        PreferencesPage::Options => &[
            "Volume:",
            "Balance:",
            "Zoom level:",
            "Podcast cache TTL (days):",
            "Podcast refresh interval (minutes):",
            "Repeat",
            "Shuffle",
            "No playlist advance",
            "Pause between songs",
            "Pause between songs time (seconds):",
            "Mouse Wheel adjusts Volume by (%):",
            "Stop with fadeout",
            "Time remaining",
            "Dock playlist",
            "Dock equalizer",
            "Convert %20 to space",
            "Convert underscore to space",
            "Show numbers in playlist",
            "Vim-style playlist navigation",
        ],
        PreferencesPage::Fonts => &[
            "Playlist font family:",
            "Open Skin Browser",
            "Skin bitmap font",
        ],
        PreferencesPage::Title => &["Title format:"],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualizationPreferenceSensitivity {
    pub analyzer_mode: bool,
    pub analyzer_style: bool,
    pub analyzer_peaks: bool,
    pub analyzer_falloff: bool,
    pub peaks_falloff: bool,
    pub scope_mode: bool,
    pub windowshade_vu: bool,
    pub refresh_rate: bool,
}

pub fn visualization_preference_sensitivity(
    mode: VisMode,
    peaks_enabled: bool,
) -> VisualizationPreferenceSensitivity {
    let analyzer = mode == VisMode::Analyzer;
    let scope = mode == VisMode::Scope;
    VisualizationPreferenceSensitivity {
        analyzer_mode: analyzer,
        analyzer_style: analyzer,
        analyzer_peaks: analyzer,
        analyzer_falloff: analyzer,
        peaks_falloff: analyzer && peaks_enabled,
        scope_mode: scope,
        windowshade_vu: analyzer,
        refresh_rate: analyzer || scope,
    }
}

fn keyboard_shortcut_from_event(
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> Option<MainKeyboardShortcut> {
    [
        ("z", MainKeyboardShortcut::Previous),
        ("x", MainKeyboardShortcut::Play),
        ("c", MainKeyboardShortcut::Pause),
        ("v", MainKeyboardShortcut::Stop),
        ("b", MainKeyboardShortcut::Next),
        ("l", MainKeyboardShortcut::OpenFiles),
        ("r", MainKeyboardShortcut::ToggleRepeat),
        ("s", MainKeyboardShortcut::ToggleShuffle),
        ("<Control>p", MainKeyboardShortcut::Preferences),
        ("<Control>l", MainKeyboardShortcut::OpenLocation),
        ("<Control>n", MainKeyboardShortcut::ToggleNoAdvance),
        ("<Control>w", MainKeyboardShortcut::ShadeMain),
        ("<Control>j", MainKeyboardShortcut::JumpTime),
        ("<Alt>s", MainKeyboardShortcut::SkinBrowser),
        ("<Shift>l", MainKeyboardShortcut::OpenDirectory),
        ("<Alt>w", MainKeyboardShortcut::PresentMain),
        ("<Alt>e", MainKeyboardShortcut::TogglePlaylist),
        ("<Alt>g", MainKeyboardShortcut::ToggleEqualizer),
        ("<Control><Shift>w", MainKeyboardShortcut::ShadePlaylist),
        ("<Control><Alt>w", MainKeyboardShortcut::ShadeEqualizer),
        ("<Control>r", MainKeyboardShortcut::ToggleTimerRemaining),
        ("<Control>a", MainKeyboardShortcut::ToggleSticky),
        ("<Control>d", MainKeyboardShortcut::DoubleScale),
        ("<Control>m", MainKeyboardShortcut::HalfScale),
        ("<Control>e", MainKeyboardShortcut::ToggleEasyMove),
        ("<Control>z", MainKeyboardShortcut::StartOfList),
        ("<Control>3", MainKeyboardShortcut::FileInfo),
        ("Insert", MainKeyboardShortcut::OpenFiles),
        ("<Shift>Insert", MainKeyboardShortcut::OpenDirectory),
        ("<Alt>Insert", MainKeyboardShortcut::OpenLocation),
    ]
    .into_iter()
    .find_map(|(accelerator, shortcut)| {
        shortcut_matches(key, state, accelerator).then_some(shortcut)
    })
}

fn handle_main_playlist_key_pressed(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    {
        let mut ui_state = main_state.borrow_mut();
        if let Some(handled) = handle_active_playlist_search_key_pressed(&mut ui_state, key, state)
        {
            return handled;
        }
    }
    let mut ui_state = main_state.borrow_mut();
    match ui_state.selected_docked_panel() {
        Some(PanelKind::Playlist) => {}
        Some(PanelKind::Equalizer) => {
            return handle_equalizer_key_pressed(&mut ui_state, key, state)
        }
        None => return handle_main_player_key_pressed(&mut ui_state, key, state),
    }
    if handle_playlist_parity_key_pressed(&mut ui_state, key, state) {
        return true;
    }
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return false;
    }
    if handle_playlist_navigation_key_pressed(&mut ui_state, key) {
        return true;
    }
    if key == gtk::gdk::Key::slash {
        return ui_state.start_playlist_search();
    }
    if key != gtk::gdk::Key::Delete && key != gtk::gdk::Key::KP_Delete {
        return false;
    }
    ui_state.remove_selected_playlist_entries()
}

fn handle_keyboard_shortcut(
    shortcut: MainKeyboardShortcut,
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    panel_windows: &PanelWindows,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    match shortcut {
        MainKeyboardShortcut::Previous => {
            main_state
                .borrow_mut()
                .activate_push(MainPushButton::Previous);
        }
        MainKeyboardShortcut::Play => {
            main_state.borrow_mut().activate_push(MainPushButton::Play);
        }
        MainKeyboardShortcut::Pause => {
            main_state.borrow_mut().activate_push(MainPushButton::Pause);
        }
        MainKeyboardShortcut::Stop => {
            main_state.borrow_mut().activate_push(MainPushButton::Stop);
        }
        MainKeyboardShortcut::Next => {
            main_state.borrow_mut().activate_push(MainPushButton::Next);
        }
        MainKeyboardShortcut::OpenFiles => {
            main_state.borrow_mut().set_file_dialog_visible(true);
            show_open_file_dialog(window, Rc::clone(main_state));
        }
        MainKeyboardShortcut::ToggleRepeat => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Repeat);
        }
        MainKeyboardShortcut::ToggleShuffle => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Shuffle);
        }
        MainKeyboardShortcut::Preferences => {
            main_state.borrow_mut().set_preferences_visible(true);
            panel_windows.preferences.present();
        }
        MainKeyboardShortcut::OpenLocation => {
            main_state.borrow_mut().set_open_location_visible(true);
            panel_windows.open_location.present();
        }
        MainKeyboardShortcut::ToggleNoAdvance => {
            let mut state = main_state.borrow_mut();
            let enabled = !state.app_state.playlist.no_advance();
            state.app_state.playlist.set_no_advance(enabled);
        }
        MainKeyboardShortcut::ShadeMain => {
            let toggled_panel = main_state.borrow_mut().toggle_selected_window_shade();
            match toggled_panel {
                Some(PanelKind::Playlist) => sync_single_panel_window_from_state(
                    PanelKind::Playlist,
                    &panel_windows.playlist,
                    &panel_windows.playlist_area,
                    main_state,
                ),
                Some(PanelKind::Equalizer) => sync_single_panel_window_from_state(
                    PanelKind::Equalizer,
                    &panel_windows.equalizer,
                    &panel_windows.equalizer_area,
                    main_state,
                ),
                None => resize_main_window(window, drawing_area, &main_state.borrow()),
            }
        }
        MainKeyboardShortcut::JumpTime => {
            main_state.borrow_mut().set_jump_time_visible(true);
            panel_windows.jump_time.present();
        }
        MainKeyboardShortcut::SkinBrowser => {
            main_state.borrow_mut().set_skin_browser_visible(true);
            panel_windows.skin_browser.present();
        }
        MainKeyboardShortcut::OpenDirectory => {
            main_state.borrow_mut().set_directory_dialog_visible(true);
            show_open_directory_dialog(window, Rc::clone(main_state));
        }
        MainKeyboardShortcut::PresentMain => {
            window.present();
        }
        MainKeyboardShortcut::TogglePlaylist => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Playlist);
            sync_panel_windows(panel_windows, &main_state.borrow());
        }
        MainKeyboardShortcut::ToggleEqualizer => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Equalizer);
            sync_panel_windows(panel_windows, &main_state.borrow());
        }
        MainKeyboardShortcut::ShadePlaylist => {
            {
                let mut state = main_state.borrow_mut();
                state.toggle_playlist_shaded();
            }
            sync_single_panel_window_from_state(
                PanelKind::Playlist,
                &panel_windows.playlist,
                &panel_windows.playlist_area,
                main_state,
            );
        }
        MainKeyboardShortcut::ShadeEqualizer => {
            {
                let mut state = main_state.borrow_mut();
                state.toggle_equalizer_shaded();
            }
            sync_single_panel_window_from_state(
                PanelKind::Equalizer,
                &panel_windows.equalizer,
                &panel_windows.equalizer_area,
                main_state,
            );
        }
        MainKeyboardShortcut::ToggleTimerRemaining => {
            let enabled = !main_state.borrow().preference_timer_remaining();
            main_state
                .borrow_mut()
                .set_preference_timer_remaining(enabled);
        }
        MainKeyboardShortcut::ToggleSticky => {
            main_state.borrow_mut().toggle_sticky();
        }
        MainKeyboardShortcut::DoubleScale => {
            main_state.borrow_mut().double_fractional_scale();
        }
        MainKeyboardShortcut::HalfScale => {
            main_state.borrow_mut().halve_fractional_scale();
        }
        MainKeyboardShortcut::ToggleEasyMove => {
            main_state.borrow_mut().toggle_easy_move();
        }
        MainKeyboardShortcut::StartOfList => {
            main_state.borrow_mut().select_first_playlist_entry();
        }
        MainKeyboardShortcut::FileInfo => {
            show_file_info_dialog(window, Rc::clone(main_state));
        }
    }
    resize_main_window(window, drawing_area, &main_state.borrow());
    drawing_area.queue_draw();
}

fn add_file_drop_controller(
    widget: &impl IsA<gtk::Widget>,
    main_state: Rc<RefCell<MainWindowUiState>>,
    clear_first: bool,
    start_playback: bool,
) {
    let drop = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    {
        let widget = widget.clone();
        drop.connect_drop(move |_target, value, _x, _y| {
            let Ok(files) = value.get::<gtk::gdk::FileList>() else {
                return false;
            };
            let uris = files
                .files()
                .into_iter()
                .map(|file| file.uri().to_string())
                .collect::<Vec<_>>();
            if !main_state
                .borrow_mut()
                .accept_dropped_uris(uris, clear_first, start_playback)
            {
                return false;
            }
            widget.queue_draw();
            true
        });
    }
    widget.add_controller(drop);
}

fn resize_main_window(
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    state: &MainWindowUiState,
) {
    let (width, height) = state.docked_panel_size();
    let scale = state.scale_factor();
    drawing_area.set_content_width(scale_dim(width, scale));
    drawing_area.set_content_height(scale_dim(height, scale));
    window.set_default_size(scale_dim(width, scale), scale_dim(height, scale));
}

fn unscale_dim(value: i32, scale: f64) -> i32 {
    ((f64::from(value) / scale.clamp(1.0, 5.0)) + 0.5).max(1.0) as i32
}

fn render_docked_ui_state(
    cr: &gtk::cairo::Context,
    skin: &DefaultSkin,
    state: &MainWindowUiState,
    pass: RenderPass,
) -> Result<bool, crate::render::RenderError> {
    let mut y = 0;
    let mut rendered = false;
    if pass.is_bitmap() {
        rendered |= render_main_player_state(cr, skin, &state.render_state())?;
    }
    y += main_window_height(state.shaded);

    if state.panel_state(PanelKind::Equalizer).is_docked_visible() {
        if pass.is_bitmap() {
            cr.save()?;
            cr.translate(0.0, f64::from(y));
            rendered |= render_equalizer_state(cr, skin, &state.equalizer_render_state())?;
            cr.restore()?;
        }
        y += equalizer_window_height(state.equalizer.panel.shaded);
    }

    if state.panel_state(PanelKind::Playlist).is_docked_visible() {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        if pass.is_bitmap() {
            rendered |= render_playlist_frame(
                cr,
                skin,
                state.playlist_focused(),
                state.playlist_ui.panel.shaded,
                state.playlist_ui.width,
                state.playlist_ui.height,
                Some(&state.shaded_playlist_info()),
                Some(&state.playlist_footer_info()),
                Some(&state.playlist_footer_time_min_text()),
                Some(&state.playlist_footer_time_sec_text()),
            )?;
        }
        if !state.playlist_ui.panel.shaded {
            let row_state = state.playlist_rows_render_state();
            rendered |= render_playlist_rows(cr, skin, &row_state, pass)?;
        }
        if pass.is_text() {
            if let Some(menu) = state.playlist_menu() {
                let (x, y, w, h) =
                    playlist_menu_rect(menu, state.playlist_ui.width, state.playlist_ui.height);
                paint_scaled(cr, x, y, w, h, |menu_cr| {
                    render_playlist_menu(
                        menu_cr,
                        skin,
                        PlaylistMenuRenderState {
                            kind: menu.render_kind(),
                            hover: state.playlist_menu_hover(),
                        },
                    )
                    .map(|_| ())
                })?;
                rendered = true;
            }
        }
        cr.restore()?;
    }

    Ok(rendered)
}

fn build_main_menu_popover(
    app: &gtk::Application,
    parent_window: &gtk::ApplicationWindow,
    parent: &gtk::DrawingArea,
    preferences_window: &gtk::ApplicationWindow,
    open_location_window: &gtk::ApplicationWindow,
    skin_browser_window: &gtk::ApplicationWindow,
    skin_editor_window: &gtk::ApplicationWindow,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .build();
    style_xmms_popover(&popover);
    popover.set_parent(parent);

    let menu_box = xmms_menu_box(0);
    let open_files = xmms_menu_button("Open Files...");
    {
        let parent_window = parent_window.clone();
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        open_files.connect_clicked(move |_| {
            main_state.borrow_mut().set_menu_visible(false);
            popover.popdown();
            show_open_file_dialog(&parent_window, Rc::clone(&main_state));
        });
    }
    menu_box.append(&open_files);

    let open_location = xmms_menu_button("Open Location...");
    {
        let open_location_window = open_location_window.clone();
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        open_location.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                state.set_menu_visible(false);
                state.set_open_location_visible(true);
            }
            popover.popdown();
            open_location_window.present();
        });
    }
    menu_box.append(&open_location);

    let preferences = xmms_menu_button("Preferences");
    {
        let preferences_window = preferences_window.clone();
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        preferences.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                state.set_menu_visible(false);
                state.set_preferences_visible(true);
            }
            popover.popdown();
            preferences_window.present();
        });
    }
    menu_box.append(&preferences);

    let skin_browser = xmms_menu_button("Skin Browser");
    {
        let skin_browser_window = skin_browser_window.clone();
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        skin_browser.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                state.set_menu_visible(false);
                state.set_skin_browser_visible(true);
            }
            popover.popdown();
            skin_browser_window.present();
        });
    }
    menu_box.append(&skin_browser);

    let skin_editor = xmms_menu_button("Skin Editor");
    {
        let skin_editor_window = skin_editor_window.clone();
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        skin_editor.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                state.set_menu_visible(false);
                state.set_skin_editor_visible(true);
            }
            popover.popdown();
            skin_editor_window.present();
        });
    }
    menu_box.append(&skin_editor);

    let quit = xmms_menu_button("Quit");
    {
        let app = app.clone();
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        quit.connect_clicked(move |_| {
            main_state.borrow_mut().set_menu_visible(false);
            popover.popdown();
            app.quit();
        });
    }
    menu_box.append(&quit);

    popover.set_child(Some(&menu_box));
    {
        let main_state = Rc::clone(main_state);
        popover.connect_closed(move |_| main_state.borrow_mut().set_menu_visible(false));
    }
    popover
}

struct PanelWindows {
    equalizer: gtk::ApplicationWindow,
    equalizer_area: gtk::DrawingArea,
    playlist: gtk::ApplicationWindow,
    playlist_area: gtk::DrawingArea,
    preferences: gtk::ApplicationWindow,
    open_location: gtk::ApplicationWindow,
    jump_time: gtk::ApplicationWindow,
    skin_browser: gtk::ApplicationWindow,
    skin_editor: gtk::ApplicationWindow,
}

impl PanelWindows {
    fn new(
        app: &gtk::Application,
        main_state: &Rc<RefCell<MainWindowUiState>>,
        main_area: &gtk::DrawingArea,
        parent_window: &gtk::ApplicationWindow,
    ) -> Self {
        let (equalizer, equalizer_area) = build_equalizer_window(app, main_state, main_area);
        let open_location =
            build_prompt_window(app, parent_window, main_state, PromptKind::OpenLocation);
        let jump_time = build_prompt_window(app, parent_window, main_state, PromptKind::JumpTime);
        let (playlist, playlist_area) =
            build_playlist_window(app, main_state, main_area, &open_location);
        let skin_browser =
            build_skin_browser_window(app, main_state, main_area, &equalizer_area, &playlist_area);
        let skin_editor =
            build_skin_editor_window(app, main_state, main_area, &equalizer_area, &playlist_area);
        let preferences = build_preferences_window(
            app,
            main_state,
            parent_window,
            main_area,
            &equalizer,
            &equalizer_area,
            &playlist,
            &playlist_area,
        );
        Self {
            equalizer,
            equalizer_area,
            playlist,
            playlist_area,
            preferences,
            open_location,
            jump_time,
            skin_browser,
            skin_editor,
        }
    }
}

fn build_equalizer_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> (gtk::ApplicationWindow, gtk::DrawingArea) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Renascene Rust Equalizer")
        .resizable(false)
        .decorated(false)
        .default_width(EQUALIZER_WINDOW_WIDTH * DEFAULT_SCALE)
        .default_height(EQUALIZER_WINDOW_HEIGHT * DEFAULT_SCALE)
        .build();
    let drawing_area = gtk::DrawingArea::builder()
        .content_width(EQUALIZER_WINDOW_WIDTH * DEFAULT_SCALE)
        .content_height(EQUALIZER_WINDOW_HEIGHT * DEFAULT_SCALE)
        .focusable(true)
        .build();
    let state = Rc::clone(main_state);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let state = state.borrow();
        let render_state = state.equalizer_render_state();
        let base_height = if render_state.shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        };
        if let Err(err) = render_scaled(
            cr,
            width,
            height,
            EQUALIZER_WINDOW_WIDTH,
            base_height,
            |cr, pass| {
                if pass.is_bitmap() {
                    render_equalizer_state(cr, state.active_skin(), &render_state).map(|_| ())
                } else {
                    Ok(())
                }
            },
        ) {
            eprintln!("xmms-rs: failed to render equalizer preview: {err}");
        }
    });
    let presets_menu = build_equalizer_presets_popover(&drawing_area, main_state, main_area);
    add_panel_click_controller(
        &window,
        &drawing_area,
        Rc::clone(main_state),
        main_area.clone(),
        PanelKind::Equalizer,
        Some(presets_menu),
        None,
        None,
    );
    add_equalizer_key_controller(&drawing_area, Rc::clone(main_state));
    window.set_child(Some(&drawing_area));
    (window, drawing_area)
}

fn build_playlist_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    open_location_window: &gtk::ApplicationWindow,
) -> (gtk::ApplicationWindow, gtk::DrawingArea) {
    let (playlist_width, playlist_height) = main_state.borrow().playlist_size();
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Renascene Rust Playlist")
        .resizable(true)
        .decorated(false)
        .default_width(playlist_width * DEFAULT_SCALE)
        .default_height(playlist_height * DEFAULT_SCALE)
        .build();
    let drawing_area = gtk::DrawingArea::builder()
        .content_width(playlist_width * DEFAULT_SCALE)
        .content_height(playlist_height * DEFAULT_SCALE)
        .focusable(true)
        .build();
    let state = Rc::clone(main_state);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let state = state.borrow();
        let skin = state.active_skin();
        let shaded = state.playlist_ui.panel.shaded;
        let focused = state.playlist_focused();
        let playlist_width = state.playlist_ui.width;
        let playlist_height = state.playlist_ui.height;
        let base_height = if shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            playlist_height
        };
        if let Err(err) = render_scaled(
            cr,
            width,
            height,
            playlist_width,
            base_height,
            |cr, pass| {
                if pass.is_bitmap() {
                    render_playlist_frame(
                        cr,
                        skin,
                        focused,
                        shaded,
                        playlist_width,
                        playlist_height,
                        Some(&state.shaded_playlist_info()),
                        Some(&state.playlist_footer_info()),
                        Some(&state.playlist_footer_time_min_text()),
                        Some(&state.playlist_footer_time_sec_text()),
                    )?;
                }
                if !shaded {
                    let row_state = state.playlist_rows_render_state();
                    render_playlist_rows(cr, skin, &row_state, pass)?;
                }
                if pass.is_text() {
                    if let Some(menu) = state.playlist_menu() {
                        let (x, y, w, h) =
                            playlist_menu_rect(menu, playlist_width, playlist_height);
                        let render_state = PlaylistMenuRenderState {
                            kind: menu.render_kind(),
                            hover: state.playlist_menu_hover(),
                        };
                        paint_scaled(cr, x, y, w, h, |menu_cr| {
                            render_playlist_menu(menu_cr, skin, render_state).map(|_| ())
                        })?;
                    }
                }
                Ok(())
            },
        ) {
            eprintln!("xmms-rs: failed to render playlist preview: {err}");
        }
    });

    add_file_drop_controller(&drawing_area, Rc::clone(main_state), false, false);
    add_playlist_context_menu(&drawing_area, Rc::clone(main_state), main_area.clone());
    add_playlist_key_controller(&drawing_area, Rc::clone(main_state));

    {
        let main_state = Rc::clone(main_state);
        drawing_area.connect_resize(move |area, width, height| {
            let mut state = main_state.borrow_mut();
            if !state.is_panel_detached(PanelKind::Playlist) {
                return;
            }
            let scale = state.scale_factor();
            let base_height = if state.playlist_ui.panel.shaded {
                state.playlist_ui.height
            } else {
                unscale_dim(height, scale).max(PLAYLIST_MIN_HEIGHT)
            };
            if state.set_playlist_size(
                unscale_dim(width, scale).max(PLAYLIST_MIN_WIDTH),
                base_height,
            ) {
                area.queue_draw();
            }
        });
    }
    add_panel_click_controller(
        &window,
        &drawing_area,
        Rc::clone(main_state),
        main_area.clone(),
        PanelKind::Playlist,
        None,
        Some(open_location_window.clone()),
        Some(build_playlist_sort_popover(
            &drawing_area,
            main_state,
            main_area,
        )),
    );
    window.set_child(Some(&drawing_area));
    (window, drawing_area)
}

fn add_playlist_key_controller(
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let key_controller = gtk::EventControllerKey::new();
    {
        let area = area.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            if handle_playlist_key_pressed(&main_state, key, state) {
                area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    area.add_controller(key_controller);
}

fn add_equalizer_key_controller(
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            if handle_equalizer_key_pressed(&mut main_state.borrow_mut(), key, state) {
                area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    area.add_controller(key_controller);
}

fn focus_cycle_shortcut(key: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> bool {
    key == gtk::gdk::Key::Tab
        && state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
        && !state.intersects(gtk::gdk::ModifierType::ALT_MASK | gtk::gdk::ModifierType::META_MASK)
}

fn handle_arrow_key_pressed(
    ui_state: &mut MainWindowUiState,
    focus: KeyboardFocus,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return false;
    }
    let Some(arrow) = ArrowKey::from_gdk(key) else {
        return false;
    };
    ui_state.apply_key_command(ui_state.arrow_key_command(focus, arrow))
}

fn handle_main_player_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    handle_arrow_key_pressed(ui_state, KeyboardFocus::Main, key, state)
}

fn handle_equalizer_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    handle_arrow_key_pressed(ui_state, KeyboardFocus::Equalizer, key, state)
}

fn handle_playlist_key_pressed(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    {
        let mut ui_state = main_state.borrow_mut();
        if let Some(handled) = handle_active_playlist_search_key_pressed(&mut ui_state, key, state)
        {
            return handled;
        }
    }

    {
        let mut ui_state = main_state.borrow_mut();
        if handle_playlist_parity_key_pressed(&mut ui_state, key, state) {
            return true;
        }
    }
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return false;
    }
    if key == gtk::gdk::Key::Delete || key == gtk::gdk::Key::KP_Delete {
        return main_state.borrow_mut().remove_selected_playlist_entries();
    }
    {
        let mut ui_state = main_state.borrow_mut();
        if handle_playlist_navigation_key_pressed(&mut ui_state, key) {
            return true;
        }
    }
    if key == gtk::gdk::Key::slash {
        return main_state.borrow_mut().start_playlist_search();
    }
    false
}

fn handle_active_playlist_search_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> Option<bool> {
    if !ui_state.playlist_search_active() {
        return None;
    }
    if key == gtk::gdk::Key::Escape {
        ui_state.stop_playlist_search();
        return Some(true);
    }
    if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
        ui_state.stop_playlist_search();
        ui_state.play_selected_playlist_entry();
        return Some(true);
    }
    if key == gtk::gdk::Key::BackSpace {
        ui_state.pop_playlist_search_char();
        return Some(true);
    }
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return Some(true);
    }
    if let Some(ch) = key.to_unicode().filter(|ch| !ch.is_control()) {
        ui_state.push_playlist_search_char(ch);
        return Some(true);
    }
    Some(true)
}

fn handle_playlist_parity_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    let control = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
    let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
    let alt = state.contains(gtk::gdk::ModifierType::ALT_MASK);
    let is_delete = key == gtk::gdk::Key::Delete || key == gtk::gdk::Key::KP_Delete;
    let is_q = key == gtk::gdk::Key::q || key == gtk::gdk::Key::Q;
    if control && is_delete {
        return ui_state.crop_playlist_to_selected_or_current();
    }
    if alt && is_q {
        return ui_state.open_queue_manager();
    }
    if shift && is_q {
        return ui_state.clear_playlist_queue();
    }
    if !control && !shift && !alt && is_q {
        return ui_state.toggle_queue_selected_playlist_entries();
    }
    if handle_arrow_key_pressed(ui_state, KeyboardFocus::Playlist, key, state) {
        return true;
    }
    match key {
        gtk::gdk::Key::Page_Up | gtk::gdk::Key::KP_Page_Up => ui_state.move_playlist_page(-1),
        gtk::gdk::Key::Page_Down | gtk::gdk::Key::KP_Page_Down => ui_state.move_playlist_page(1),
        gtk::gdk::Key::Home | gtk::gdk::Key::KP_Home => ui_state.move_playlist_to_start(),
        gtk::gdk::Key::End | gtk::gdk::Key::KP_End => ui_state.move_playlist_to_end(),
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
            ui_state.activate_selected_or_current_playlist_entry()
        }
        _ => false,
    }
}

fn handle_playlist_navigation_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
) -> bool {
    match key {
        gtk::gdk::Key::j => ui_state.move_playlist_selection(1),
        gtk::gdk::Key::k => ui_state.move_playlist_selection(-1),
        gtk::gdk::Key::p => ui_state.play_selected_playlist_entry(),
        _ => false,
    }
}

fn add_playlist_context_menu(
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    main_area: gtk::DrawingArea,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    style_xmms_popover(&popover);
    popover.set_parent(area);

    let menu_box = xmms_menu_box(4);
    for (label, action) in [
        ("Remove Selected", PlaylistContextAction::RemoveSelected),
        ("Remove Dead Files", PlaylistContextAction::RemoveDead),
        ("Physically Delete", PlaylistContextAction::PhysicallyDelete),
        ("Select All", PlaylistContextAction::SelectAll),
        ("Select None", PlaylistContextAction::SelectNone),
        ("Invert Selection", PlaylistContextAction::InvertSelection),
    ] {
        let button = xmms_menu_button(label);
        let state = Rc::clone(&main_state);
        let area = area.clone();
        let main_area = main_area.clone();
        let popover = popover.clone();
        button.connect_clicked(move |_| {
            popover.popdown();
            if action == PlaylistContextAction::PhysicallyDelete {
                show_playlist_delete_confirmation(
                    &area,
                    Rc::clone(&state),
                    area.clone(),
                    main_area.clone(),
                );
            } else {
                state.borrow_mut().activate_playlist_context_action(action);
                area.queue_draw();
                main_area.queue_draw();
            }
        });
        menu_box.append(&button);
    }
    popover.set_child(Some(&menu_box));

    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    right_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let popover = popover.clone();
        right_click.connect_pressed(move |_gesture, _n_press, x, y| {
            area.grab_focus();
            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        });
    }
    area.add_controller(right_click);
}

fn show_playlist_delete_confirmation(
    parent: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window("Delete selected files?", 280, 100, &[]);
    window.set_modal(true);
    if let Some(root) = parent
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
    {
        window.set_transient_for(Some(&root));
    }

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    layout.append(&gtk::Label::new(Some(
        "Delete selected local files from disk?",
    )));

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let cancel = gtk::Button::with_label("Cancel");
    let delete = gtk::Button::with_label("Delete");
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        delete.connect_clicked(move |_| {
            main_state
                .borrow_mut()
                .activate_playlist_context_action(PlaylistContextAction::PhysicallyDelete);
            window.close();
            playlist_area.queue_draw();
            main_area.queue_draw();
        });
    }
    buttons.append(&cancel);
    buttons.append(&delete);
    layout.append(&buttons);
    window.set_child(Some(&layout));
    window.present();
}

fn build_playlist_sort_popover(
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .build();
    style_xmms_popover(&popover);
    popover.set_parent(parent);
    let menu_box = xmms_menu_box(0);
    for (label, action) in [
        ("Sort List: By Title", PlaylistSortAction::ListByTitle),
        ("Sort List: By Filename", PlaylistSortAction::ListByFilename),
        (
            "Sort List: By Path + Filename",
            PlaylistSortAction::ListByPath,
        ),
        ("Sort List: By Date", PlaylistSortAction::ListByDate),
        (
            "Sort Selection: By Title",
            PlaylistSortAction::SelectionByTitle,
        ),
        (
            "Sort Selection: By Filename",
            PlaylistSortAction::SelectionByFilename,
        ),
        (
            "Sort Selection: By Path + Filename",
            PlaylistSortAction::SelectionByPath,
        ),
        (
            "Sort Selection: By Date",
            PlaylistSortAction::SelectionByDate,
        ),
        ("Randomize List", PlaylistSortAction::RandomizeList),
        ("Reverse List", PlaylistSortAction::ReverseList),
    ] {
        let item = xmms_menu_button(label);
        {
            let main_state = Rc::clone(main_state);
            let parent = parent.clone();
            let main_area = main_area.clone();
            let popover = popover.clone();
            item.connect_clicked(move |_| {
                main_state
                    .borrow_mut()
                    .activate_playlist_sort_action(action);
                popover.popdown();
                parent.queue_draw();
                main_area.queue_draw();
            });
        }
        menu_box.append(&item);
    }
    popover.set_child(Some(&menu_box));
    popover
}

fn show_playlist_sort_menu(popover: &gtk::Popover, area: &gtk::DrawingArea) {
    let width = area.allocated_width().max(1) as f64;
    let height = area.allocated_height().max(1) as f64;
    let rect = gtk::gdk::Rectangle::new(
        (99.0 * (width / f64::from(PLAYLIST_DEFAULT_WIDTH))) as i32,
        (f64::from(PLAYLIST_DEFAULT_HEIGHT - 29) * (height / f64::from(PLAYLIST_DEFAULT_HEIGHT)))
            as i32,
        25,
        1,
    );
    popover.set_pointing_to(Some(&rect));
    popover.popup();
}

fn build_equalizer_presets_popover(
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> gtk::Popover {
    let action_group = gtk::gio::SimpleActionGroup::new();
    let menu = gtk::gio::Menu::new();

    for (label, actions) in [
        (
            "Load",
            &[
                ("Preset", EqualizerPresetAction::LoadPreset),
                ("Auto-load preset", EqualizerPresetAction::LoadAutoPreset),
                ("Default", EqualizerPresetAction::LoadDefault),
                ("Zero", EqualizerPresetAction::LoadZero),
                ("From file", EqualizerPresetAction::LoadFromFile),
                (
                    "From WinAMP EQF file",
                    EqualizerPresetAction::LoadFromWinampFile,
                ),
            ][..],
        ),
        (
            "Import",
            &[("WinAMP Presets", EqualizerPresetAction::ImportWinampPresets)][..],
        ),
        (
            "Save",
            &[
                ("Preset", EqualizerPresetAction::SavePreset),
                ("Auto-load preset", EqualizerPresetAction::SaveAutoPreset),
                ("Default", EqualizerPresetAction::SaveDefault),
                ("To file", EqualizerPresetAction::SaveToFile),
                (
                    "To WinAMP EQF file",
                    EqualizerPresetAction::SaveToWinampFile,
                ),
            ][..],
        ),
        (
            "Delete",
            &[
                ("Preset", EqualizerPresetAction::DeletePreset),
                ("Auto-load preset", EqualizerPresetAction::DeleteAutoPreset),
            ][..],
        ),
    ] {
        let submenu = gtk::gio::Menu::new();
        for (child_label, action) in actions {
            let action_name = equalizer_preset_action_name(*action);
            submenu.append(
                Some(child_label),
                Some(&format!("eq-presets.{action_name}")),
            );
            install_equalizer_preset_action(
                &action_group,
                *action,
                action_name,
                parent,
                main_state,
                main_area,
            );
        }
        if label == "Load" {
            let winamp_section = gtk::gio::Menu::new();
            for (index, preset) in winamp_original_presets().into_iter().enumerate() {
                let action_name = format!("load-winamp-original-preset-{index}");
                winamp_section.append(
                    Some(&preset.name),
                    Some(&format!("eq-presets.{action_name}")),
                );
                install_equalizer_direct_preset_action(
                    &action_group,
                    action_name,
                    preset,
                    parent,
                    main_state,
                    main_area,
                );
            }
            submenu.append_section(Some("Winamp original presets"), &winamp_section);

            let preset_section = gtk::gio::Menu::new();
            for (index, preset) in main_state
                .borrow()
                .sorted_equalizer_presets(false)
                .into_iter()
                .filter(|preset| !preset.name.eq_ignore_ascii_case("Default"))
                .enumerate()
            {
                let action_name = format!("load-named-preset-{index}");
                preset_section.append(
                    Some(&preset.name),
                    Some(&format!("eq-presets.{action_name}")),
                );
                install_equalizer_named_preset_action(
                    &action_group,
                    action_name,
                    preset.name,
                    parent,
                    main_state,
                    main_area,
                );
            }
            if preset_section.n_items() > 0 {
                submenu.append_section(Some("Presets"), &preset_section);
            }
        }
        menu.append_submenu(Some(label), &submenu);
    }

    install_equalizer_preset_action(
        &action_group,
        EqualizerPresetAction::Configure,
        equalizer_preset_action_name(EqualizerPresetAction::Configure),
        parent,
        main_state,
        main_area,
    );
    menu.append(
        Some("Configure Equalizer"),
        Some(&format!(
            "eq-presets.{}",
            equalizer_preset_action_name(EqualizerPresetAction::Configure)
        )),
    );

    parent.insert_action_group("eq-presets", Some(&action_group));
    let popover_menu = gtk::PopoverMenu::from_model_full(&menu, gtk::PopoverMenuFlags::NESTED);
    popover_menu.set_autohide(true);
    popover_menu.set_has_arrow(false);
    let popover: gtk::Popover = popover_menu.upcast();
    style_xmms_popover(&popover);
    popover.set_parent(parent);
    popover
}

fn install_equalizer_preset_action(
    group: &gtk::gio::SimpleActionGroup,
    action: EqualizerPresetAction,
    action_name: &'static str,
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) {
    let simple_action = gtk::gio::SimpleAction::new(action_name, None);
    let main_state = Rc::clone(main_state);
    let parent = parent.clone();
    let main_area = main_area.clone();
    simple_action.connect_activate(move |_, _| {
        activate_equalizer_preset_action(
            action,
            &parent,
            Rc::clone(&main_state),
            parent.clone(),
            main_area.clone(),
        );
    });
    group.add_action(&simple_action);
}

fn install_equalizer_direct_preset_action(
    group: &gtk::gio::SimpleActionGroup,
    action_name: String,
    preset: EqualizerPreset,
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) {
    let simple_action = gtk::gio::SimpleAction::new(&action_name, None);
    let main_state = Rc::clone(main_state);
    let parent = parent.clone();
    let main_area = main_area.clone();
    simple_action.connect_activate(move |_, _| {
        main_state
            .borrow_mut()
            .apply_equalizer_preset_values(&preset);
        parent.queue_draw();
        main_area.queue_draw();
    });
    group.add_action(&simple_action);
}

fn install_equalizer_named_preset_action(
    group: &gtk::gio::SimpleActionGroup,
    action_name: String,
    preset_name: String,
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) {
    let simple_action = gtk::gio::SimpleAction::new(&action_name, None);
    let main_state = Rc::clone(main_state);
    let parent = parent.clone();
    let main_area = main_area.clone();
    simple_action.connect_activate(move |_, _| {
        main_state
            .borrow_mut()
            .load_named_equalizer_preset(&preset_name, false);
        parent.queue_draw();
        main_area.queue_draw();
    });
    group.add_action(&simple_action);
}

fn equalizer_preset_action_name(action: EqualizerPresetAction) -> &'static str {
    match action {
        EqualizerPresetAction::LoadPreset => "load-preset",
        EqualizerPresetAction::LoadAutoPreset => "load-auto-preset",
        EqualizerPresetAction::LoadDefault => "load-default",
        EqualizerPresetAction::LoadZero => "load-zero",
        EqualizerPresetAction::LoadFromFile => "load-from-file",
        EqualizerPresetAction::LoadFromWinampFile => "load-from-winamp-file",
        EqualizerPresetAction::ImportWinampPresets => "import-winamp-presets",
        EqualizerPresetAction::SavePreset => "save-preset",
        EqualizerPresetAction::SaveAutoPreset => "save-auto-preset",
        EqualizerPresetAction::SaveDefault => "save-default",
        EqualizerPresetAction::SaveToFile => "save-to-file",
        EqualizerPresetAction::SaveToWinampFile => "save-to-winamp-file",
        EqualizerPresetAction::DeletePreset => "delete-preset",
        EqualizerPresetAction::DeleteAutoPreset => "delete-auto-preset",
        EqualizerPresetAction::Configure => "configure",
    }
}

fn build_preferences_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_window: &gtk::ApplicationWindow,
    main_area: &gtk::DrawingArea,
    equalizer_window: &gtk::ApplicationWindow,
    equalizer_area: &gtk::DrawingArea,
    playlist_window: &gtk::ApplicationWindow,
    playlist_area: &gtk::DrawingArea,
) -> gtk::ApplicationWindow {
    let (default_width, default_height) = preferences_window_default_size();
    let window = skinned_application_window(
        app,
        "Preferences",
        default_width,
        default_height,
        &["xmms-preferences"],
    );

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.add_css_class("xmms-skinned-window");
    root.add_css_class("xmms-preferences");
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);

    let notebook = gtk::Notebook::new();
    notebook.set_vexpand(true);
    let preferences_changed: PreferencesChanged = Rc::new({
        let main_state = Rc::clone(main_state);
        let main_window = main_window.clone();
        let main_area = main_area.clone();
        let equalizer_window = equalizer_window.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_window = playlist_window.clone();
        let playlist_area = playlist_area.clone();
        move || {
            sync_single_panel_window_from_state(
                PanelKind::Equalizer,
                &equalizer_window,
                &equalizer_area,
                &main_state,
            );
            sync_single_panel_window_from_state(
                PanelKind::Playlist,
                &playlist_window,
                &playlist_area,
                &main_state,
            );
            resize_main_window(&main_window, &main_area, &main_state.borrow());
            main_area.queue_draw();
        }
    });

    for (page, label, page_widget) in [
        (
            PreferencesPage::Audio,
            "Audio I/O Plugins",
            build_preferences_audio_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Visualization,
            "Visualization Plugins",
            build_preferences_visualization_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Options,
            "Options",
            build_preferences_options_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Fonts,
            "Fonts",
            build_preferences_fonts_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Title,
            "Title",
            build_preferences_title_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
    ] {
        let scrolled = gtk::ScrolledWindow::new();
        scrolled.add_css_class("xmms-skinned-window");
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&page_widget));
        notebook.append_page(&scrolled, Some(&gtk::Label::new(Some(label))));
        if page == PreferencesPage::Options {
            notebook.set_current_page(Some(2));
        }
    }
    {
        let main_state = Rc::clone(main_state);
        notebook.connect_switch_page(move |_notebook, _page_widget, page_num| {
            let page = match page_num {
                0 => PreferencesPage::Audio,
                1 => PreferencesPage::Visualization,
                2 => PreferencesPage::Options,
                3 => PreferencesPage::Fonts,
                _ => PreferencesPage::Title,
            };
            main_state.borrow_mut().set_preferences_page(page);
        });
    }
    root.append(&notebook);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    buttons.set_halign(gtk::Align::End);
    let reset = gtk::Button::with_label("Reset to Defaults");
    {
        let main_state = Rc::clone(main_state);
        let preferences_changed = Rc::clone(&preferences_changed);
        reset.connect_clicked(move |_| {
            main_state.borrow_mut().reset_preferences_to_defaults();
            preferences_changed();
        });
    }
    buttons.append(&reset);
    root.append(&buttons);
    window.set_child(Some(&root));

    {
        let main_state = Rc::clone(main_state);
        window.connect_close_request(move |window| {
            main_state.borrow_mut().set_preferences_visible(false);
            window.hide();
            gtk::glib::Propagation::Stop
        });
    }

    window
}

fn prefs_page_box() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_margin_top(8);
    page.set_margin_bottom(8);
    page.set_margin_start(8);
    page.set_margin_end(8);
    page
}

fn prefs_frame(title: &str, parent: &gtk::Box) -> gtk::Box {
    let frame = gtk::Frame::new(Some(title));
    frame.set_margin_top(6);
    frame.set_margin_bottom(6);
    frame.set_margin_start(6);
    frame.set_margin_end(6);
    parent.append(&frame);

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);
    box_.set_margin_start(8);
    box_.set_margin_end(8);
    frame.set_child(Some(&box_));
    box_
}

fn prefs_grid() -> gtk::Grid {
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);
    grid
}

fn prefs_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn prefs_attach_label(grid: &gtk::Grid, label: &str, child: &impl IsA<gtk::Widget>, row: i32) {
    grid.attach(&prefs_label(label), 0, row, 1, 1);
    grid.attach(child, 1, row, 1, 1);
    child.set_hexpand(true);
}

fn find_spin_button_by_name(root: &impl IsA<gtk::Widget>, name: &str) -> Option<gtk::SpinButton> {
    let root = root.as_ref();
    if root.widget_name() == name {
        if let Ok(spin) = root.clone().downcast::<gtk::SpinButton>() {
            return Some(spin);
        }
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(spin) = find_spin_button_by_name(&widget, name) {
            return Some(spin);
        }
        child = widget.next_sibling();
    }
    None
}

fn set_spin_value_if_changed(spin: &gtk::SpinButton, value: i32) {
    if spin.value_as_int() != value {
        spin.set_value(value as f64);
    }
}

fn sync_preferences_options_controls(
    preferences_window: &gtk::ApplicationWindow,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let (volume, balance) = {
        let state = main_state.borrow();
        (state.volume(), state.balance())
    };
    if let Some(spin) = find_spin_button_by_name(preferences_window, PREFERENCES_VOLUME_WIDGET) {
        set_spin_value_if_changed(&spin, volume);
    }
    if let Some(spin) = find_spin_button_by_name(preferences_window, PREFERENCES_BALANCE_WIDGET) {
        set_spin_value_if_changed(&spin, balance);
    }
}

fn prefs_check(label: &str, active: bool) -> gtk::CheckButton {
    let check = gtk::CheckButton::with_label(label);
    check.set_halign(gtk::Align::Start);
    check.set_active(active);
    check
}

fn build_preferences_audio_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let input = prefs_frame("Input Plugins", &page);
    input.append(&prefs_label("GStreamer input support (built in)"));
    input.append(&prefs_label(
        "File, URI, and stream decoding are provided by installed GStreamer plugins.",
    ));

    let output = prefs_frame("Output Plugin", &page);
    let grid = prefs_grid();
    output.append(&grid);
    let output_combo = gtk::ComboBoxText::new();
    output_combo.append(Some("auto"), "Automatic (System Default)");
    if let Ok(devices) = list_gstreamer_output_devices() {
        for device in devices {
            output_combo.append(Some(&device.id), &device.display_name);
        }
    }
    if let Some(device) = main_state.borrow().preference_output_device() {
        if !output_combo.set_active_id(Some(device)) {
            output_combo.append(Some(device), device);
            output_combo.set_active_id(Some(device));
        }
    } else {
        output_combo.set_active_id(Some("auto"));
    }
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        output_combo.connect_changed(move |combo| {
            let selected = combo.active_id().map(|id| id.to_string());
            let device = selected.filter(|id| id != "auto");
            main_state.borrow_mut().set_preference_output_device(device);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Output device:", &output_combo, 0);

    let configure = gtk::Button::with_label("Configure");
    configure.connect_clicked(|_| {
        eprintln!("xmms-rs: output device configuration is handled by the system audio settings");
    });
    grid.attach(&configure, 1, 1, 1, 1);
    page
}

fn build_preferences_options_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let box_ = prefs_frame("Options", &page);
    let grid = prefs_grid();
    box_.append(&grid);

    let volume = gtk::SpinButton::with_range(0.0, 100.0, 1.0);
    volume.set_widget_name(PREFERENCES_VOLUME_WIDGET);
    volume.set_value(main_state.borrow().volume() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        volume.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_volume(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Volume:", &volume, 0);

    let balance = gtk::SpinButton::with_range(-100.0, 100.0, 1.0);
    balance.set_widget_name(PREFERENCES_BALANCE_WIDGET);
    balance.set_value(main_state.borrow().balance() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        balance.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_balance(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Balance:", &balance, 1);

    let (scale, zoom_text) = {
        let state = main_state.borrow();
        let scale = state.app_state.config.scale_factor.clamp(1.0, 5.0);
        (scale, format!("{scale:.1}x"))
    };
    let zoom = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 5.0, 0.1);
    zoom.set_digits(1);
    zoom.set_draw_value(false);
    zoom.set_value(scale);
    let zoom_value = gtk::Entry::new();
    zoom_value.set_editable(false);
    zoom_value.set_width_chars(5);
    zoom_value.set_hexpand(false);
    zoom_value.set_text(&zoom_text);
    zoom.set_hexpand(true);
    let zoom_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    zoom_box.set_hexpand(true);
    zoom_box.append(&zoom);
    zoom_box.append(&zoom_value);
    {
        let main_state = Rc::clone(main_state);
        let zoom_value = zoom_value.clone();
        let on_change = on_change.clone();
        zoom.connect_value_changed(move |scale| {
            let value = scale.value().clamp(1.0, 5.0);
            zoom_value.set_text(&format!("{value:.1}x"));
            main_state.borrow_mut().set_preference_scale_factor(value);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    grid.attach(&prefs_label("Zoom level:"), 0, 2, 2, 1);
    grid.attach(&zoom_box, 0, 3, 2, 1);

    let ttl = gtk::SpinButton::with_range(1.0, 3650.0, 1.0);
    ttl.set_value(main_state.borrow().preference_podcast_cache_ttl_days() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        ttl.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_podcast_cache_ttl_days(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Podcast cache TTL (days):", &ttl, 4);

    let refresh = gtk::SpinButton::with_range(1.0, 10080.0, 1.0);
    refresh.set_value(
        main_state
            .borrow()
            .preference_podcast_refresh_interval_minutes() as f64,
    );
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        refresh.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_podcast_refresh_interval_minutes(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Podcast refresh interval (minutes):", &refresh, 5);

    let pause_time = gtk::SpinButton::with_range(0.0, 1000.0, 1.0);
    pause_time.set_value(main_state.borrow().preference_pause_between_songs_time() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        pause_time.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_pause_between_songs_time(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Pause between songs time (seconds):", &pause_time, 6);

    let mouse_wheel = gtk::SpinButton::with_range(1.0, 100.0, 1.0);
    mouse_wheel.set_value(main_state.borrow().preference_mouse_wheel_change() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        mouse_wheel.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_mouse_wheel_change(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Mouse Wheel adjusts Volume by (%):", &mouse_wheel, 7);

    let checks = {
        let state = main_state.borrow();
        [
            ("Repeat", state.repeat(), PreferenceCheck::Repeat),
            ("Shuffle", state.shuffle(), PreferenceCheck::Shuffle),
            (
                "No playlist advance",
                state.preference_no_playlist_advance(),
                PreferenceCheck::NoAdvance,
            ),
            (
                "Pause between songs",
                state.preference_pause_between_songs(),
                PreferenceCheck::PauseBetweenSongs,
            ),
            (
                "Stop with fadeout",
                state.preference_stop_with_fadeout(),
                PreferenceCheck::StopWithFadeout,
            ),
            (
                "Time remaining",
                state.preference_timer_remaining(),
                PreferenceCheck::TimerRemaining,
            ),
            (
                "Dock playlist",
                !state.is_panel_detached(PanelKind::Playlist),
                PreferenceCheck::DockPlaylist,
            ),
            (
                "Dock equalizer",
                !state.is_panel_detached(PanelKind::Equalizer),
                PreferenceCheck::DockEqualizer,
            ),
            (
                "Convert %20 to space",
                state.preference_convert_twenty(),
                PreferenceCheck::ConvertTwenty,
            ),
            (
                "Convert underscore to space",
                state.preference_convert_underscore(),
                PreferenceCheck::ConvertUnderscore,
            ),
            (
                "Show numbers in playlist",
                state.preference_show_numbers_in_playlist(),
                PreferenceCheck::ShowNumbers,
            ),
            (
                "Vim-style playlist navigation",
                state.preference_vim_playlist_navigation(),
                PreferenceCheck::VimPlaylistNavigation,
            ),
        ]
    };
    for (index, (label, active, action)) in checks.into_iter().enumerate() {
        let check = prefs_check(label, active);
        {
            let main_state = Rc::clone(main_state);
            let on_change = on_change.clone();
            check.connect_toggled(move |check| {
                let mut state = main_state.borrow_mut();
                match action {
                    PreferenceCheck::Repeat => state.set_preference_repeat(check.is_active()),
                    PreferenceCheck::Shuffle => state.set_preference_shuffle(check.is_active()),
                    PreferenceCheck::NoAdvance => {
                        state.set_preference_no_playlist_advance(check.is_active())
                    }
                    PreferenceCheck::PauseBetweenSongs => {
                        state.set_preference_pause_between_songs(check.is_active())
                    }
                    PreferenceCheck::StopWithFadeout => {
                        state.set_preference_stop_with_fadeout(check.is_active())
                    }
                    PreferenceCheck::TimerRemaining => {
                        state.set_preference_timer_remaining(check.is_active())
                    }
                    PreferenceCheck::DockPlaylist => {
                        state.set_preference_playlist_docked(check.is_active())
                    }
                    PreferenceCheck::DockEqualizer => {
                        state.set_preference_equalizer_docked(check.is_active())
                    }
                    PreferenceCheck::ConvertTwenty => {
                        state.set_preference_convert_twenty(check.is_active())
                    }
                    PreferenceCheck::ConvertUnderscore => {
                        state.set_preference_convert_underscore(check.is_active())
                    }
                    PreferenceCheck::ShowNumbers => {
                        state.set_preference_show_numbers_in_playlist(check.is_active())
                    }
                    PreferenceCheck::VimPlaylistNavigation => {
                        state.set_preference_vim_playlist_navigation(check.is_active())
                    }
                }
                drop(state);
                if let Some(on_change) = &on_change {
                    on_change();
                }
            });
        }
        grid.attach(&check, (index % 2) as i32, 8 + (index / 2) as i32, 1, 1);
    }
    page
}

#[derive(Debug, Clone, Copy)]
enum PreferenceCheck {
    Repeat,
    Shuffle,
    NoAdvance,
    PauseBetweenSongs,
    StopWithFadeout,
    TimerRemaining,
    DockPlaylist,
    DockEqualizer,
    ConvertTwenty,
    ConvertUnderscore,
    ShowNumbers,
    VimPlaylistNavigation,
}

fn build_preferences_fonts_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let playlist = prefs_frame("Playlist", &page);
    let grid = prefs_grid();
    playlist.append(&grid);
    let playlist_font = gtk::ComboBoxText::with_entry();
    for font in ["Helvetica", "Sans", "Serif", "Monospace"] {
        playlist_font.append(Some(font), font);
    }
    let current_font = main_state.borrow().preference_playlist_font().to_string();
    if !playlist_font.set_active_id(Some(&current_font)) {
        playlist_font.append(Some(&current_font), &current_font);
        playlist_font.set_active_id(Some(&current_font));
    }
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        playlist_font.connect_changed(move |combo| {
            if let Some(font) = combo.active_id() {
                main_state.borrow_mut().set_preference_playlist_font(&font);
                if let Some(on_change) = &on_change {
                    on_change();
                }
            }
        });
    }
    prefs_attach_label(&grid, "Playlist font family:", &playlist_font, 0);
    playlist.append(&prefs_label("XMMS used a Helvetica bold 10px playlist font. This port keeps the original fixed row height, so only the family is configurable."));

    let main = prefs_frame("Main Window", &page);
    let mainwin_font = gtk::Entry::new();
    mainwin_font.set_editable(false);
    mainwin_font.set_text(main_state.borrow().preference_mainwin_font());
    main.append(&mainwin_font);
    main.append(&prefs_label(
        "The main window uses the skin bitmap font, matching XMMS skins.",
    ));
    let skin_browser = gtk::Button::with_label("Open Skin Browser");
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        skin_browser.connect_clicked(move |_| {
            main_state.borrow_mut().set_skin_browser_visible(true);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    main.append(&skin_browser);
    page
}

fn build_preferences_title_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let box_ = prefs_frame("Title", &page);
    let grid = prefs_grid();
    box_.append(&grid);
    let title = gtk::Entry::new();
    title.set_text(main_state.borrow().preference_title_format());
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        title.connect_changed(move |entry| {
            main_state
                .borrow_mut()
                .set_preference_title_format(entry.text().as_str());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Title format:", &title, 0);
    box_.append(&prefs_label("Original XMMS tokens include %p artist, %a album, %g genre, %f filename, and %t title. The current decoder uses embedded titles when available and stores this format for compatibility."));
    page
}

fn build_preferences_visualization_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let box_ = prefs_frame("Visualization", &page);
    let grid = prefs_grid();
    box_.append(&grid);
    box_.append(&prefs_label(
        "Controls that do not affect the selected visualization mode are disabled.",
    ));

    let mode = gtk::ComboBoxText::new();
    for (id, label) in [
        ("analyzer", "Analyzer"),
        ("scope", "Scope"),
        ("milkdrop", "MilkDrop-inspired"),
        ("off", "Off"),
    ] {
        mode.append(Some(id), label);
    }
    mode.set_active_id(Some(match main_state.borrow().visualization_mode() {
        VisMode::Scope => "scope",
        VisMode::Milkdrop => "milkdrop",
        VisMode::Off => "off",
        VisMode::Analyzer => "analyzer",
    }));
    prefs_attach_label(&grid, "Visualization mode:", &mode, 0);

    let analyzer_mode = gtk::ComboBoxText::new();
    for (id, label) in [
        ("normal", "Analyzer normal"),
        ("fire", "Analyzer fire"),
        ("vlines", "Analyzer vertical lines"),
    ] {
        analyzer_mode.append(Some(id), label);
    }
    analyzer_mode.set_active_id(Some(
        match main_state.borrow().visualization_analyzer_mode() {
            VisAnalyzerMode::Fire => "fire",
            VisAnalyzerMode::VerticalLines => "vlines",
            VisAnalyzerMode::Normal => "normal",
        },
    ));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        analyzer_mode.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("fire") => VisAnalyzerMode::Fire,
                Some("vlines") => VisAnalyzerMode::VerticalLines,
                _ => VisAnalyzerMode::Normal,
            };
            main_state
                .borrow_mut()
                .set_visualization_analyzer_mode(mode);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Analyzer mode:", &analyzer_mode, 1);

    let style = gtk::ComboBoxText::new();
    style.append(Some("bars"), "Analyzer bars");
    style.append(Some("lines"), "Analyzer lines");
    style.set_active_id(Some(
        match main_state.borrow().visualization_analyzer_style() {
            VisAnalyzerStyle::Lines => "lines",
            VisAnalyzerStyle::Bars => "bars",
        },
    ));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        style.connect_changed(move |combo| {
            let style = match combo.active_id().as_deref() {
                Some("lines") => VisAnalyzerStyle::Lines,
                _ => VisAnalyzerStyle::Bars,
            };
            main_state
                .borrow_mut()
                .set_visualization_analyzer_style(style);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Analyzer style:", &style, 2);

    let scope = gtk::ComboBoxText::new();
    for (id, label) in [
        ("dot", "Dot scope"),
        ("line", "Line scope"),
        ("solid", "Solid scope"),
    ] {
        scope.append(Some(id), label);
    }
    scope.set_active_id(Some(match main_state.borrow().visualization_scope_mode() {
        VisScopeMode::Dot => "dot",
        VisScopeMode::Solid => "solid",
        VisScopeMode::Line => "line",
    }));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        scope.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("dot") => VisScopeMode::Dot,
                Some("solid") => VisScopeMode::Solid,
                _ => VisScopeMode::Line,
            };
            main_state.borrow_mut().set_visualization_scope_mode(mode);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Scope mode:", &scope, 3);

    let peaks = prefs_check(
        "Show analyzer peaks",
        main_state.borrow().visualization_peaks_enabled(),
    );
    grid.attach(&peaks, 1, 4, 1, 1);

    let falloff = falloff_combo(main_state.borrow().visualization_analyzer_falloff());
    let peaks_falloff = falloff_combo(main_state.borrow().visualization_peaks_falloff());
    {
        let main_state = Rc::clone(main_state);
        let peaks_falloff = peaks_falloff.clone();
        let on_change = on_change.clone();
        falloff.connect_changed(move |combo| {
            let analyzer = falloff_from_combo(combo);
            let peaks = falloff_from_combo(&peaks_falloff);
            main_state
                .borrow_mut()
                .set_visualization_falloff(analyzer, peaks);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let falloff = falloff.clone();
        let on_change = on_change.clone();
        peaks_falloff.connect_changed(move |combo| {
            let analyzer = falloff_from_combo(&falloff);
            let peaks = falloff_from_combo(combo);
            main_state
                .borrow_mut()
                .set_visualization_falloff(analyzer, peaks);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Analyzer falloff:", &falloff, 5);
    prefs_attach_label(&grid, "Peaks falloff:", &peaks_falloff, 6);

    let vu = gtk::ComboBoxText::new();
    vu.append(Some("normal"), "Normal");
    vu.append(Some("smooth"), "Smooth");
    vu.set_active_id(Some(match main_state.borrow().visualization_vu_mode() {
        VisVuMode::Smooth => "smooth",
        VisVuMode::Normal => "normal",
    }));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        vu.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("smooth") => VisVuMode::Smooth,
                _ => VisVuMode::Normal,
            };
            main_state.borrow_mut().set_visualization_vu_mode(mode);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "WindowShade VU mode:", &vu, 7);

    let refresh = gtk::ComboBoxText::new();
    for (id, label) in [
        ("full", "Full"),
        ("half", "Half"),
        ("quarter", "Quarter"),
        ("eighth", "Eighth"),
    ] {
        refresh.append(Some(id), label);
    }
    refresh.set_active_id(Some(
        match main_state.borrow().visualization_refresh_divisor() {
            8.. => "eighth",
            4..=7 => "quarter",
            2..=3 => "half",
            _ => "full",
        },
    ));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        refresh.connect_changed(move |combo| {
            let divisor = match combo.active_id().as_deref() {
                Some("eighth") => 8,
                Some("quarter") => 4,
                Some("half") => 2,
                _ => 1,
            };
            main_state
                .borrow_mut()
                .set_visualization_refresh_divisor(divisor);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Refresh rate:", &refresh, 8);

    update_visualization_preference_sensitivity(
        &mode,
        &analyzer_mode,
        &style,
        &scope,
        &peaks,
        &falloff,
        &peaks_falloff,
        &vu,
        &refresh,
    );
    {
        let main_state = Rc::clone(main_state);
        let analyzer_mode = analyzer_mode.clone();
        let style = style.clone();
        let scope = scope.clone();
        let peaks = peaks.clone();
        let falloff = falloff.clone();
        let peaks_falloff = peaks_falloff.clone();
        let vu = vu.clone();
        let refresh = refresh.clone();
        let on_change = on_change.clone();
        mode.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("scope") => VisMode::Scope,
                Some("milkdrop") => VisMode::Milkdrop,
                Some("off") => VisMode::Off,
                _ => VisMode::Analyzer,
            };
            main_state.borrow_mut().set_visualization_mode(mode);
            update_visualization_preference_sensitivity(
                combo,
                &analyzer_mode,
                &style,
                &scope,
                &peaks,
                &falloff,
                &peaks_falloff,
                &vu,
                &refresh,
            );
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let mode = mode.clone();
        let analyzer_mode = analyzer_mode.clone();
        let style = style.clone();
        let scope = scope.clone();
        let falloff = falloff.clone();
        let peaks_falloff = peaks_falloff.clone();
        let vu = vu.clone();
        let refresh = refresh.clone();
        let on_change = on_change.clone();
        peaks.connect_toggled(move |check| {
            main_state
                .borrow_mut()
                .set_visualization_peaks_enabled(check.is_active());
            update_visualization_preference_sensitivity(
                &mode,
                &analyzer_mode,
                &style,
                &scope,
                check,
                &falloff,
                &peaks_falloff,
                &vu,
                &refresh,
            );
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    page
}

fn update_visualization_preference_sensitivity(
    mode: &gtk::ComboBoxText,
    analyzer_mode: &gtk::ComboBoxText,
    analyzer_style: &gtk::ComboBoxText,
    scope_mode: &gtk::ComboBoxText,
    peaks: &gtk::CheckButton,
    analyzer_falloff: &gtk::ComboBoxText,
    peaks_falloff: &gtk::ComboBoxText,
    vu: &gtk::ComboBoxText,
    refresh: &gtk::ComboBoxText,
) {
    let mode = match mode.active_id().as_deref() {
        Some("scope") => VisMode::Scope,
        Some("milkdrop") => VisMode::Milkdrop,
        Some("off") => VisMode::Off,
        _ => VisMode::Analyzer,
    };
    let sensitivity = visualization_preference_sensitivity(mode, peaks.is_active());
    analyzer_mode.set_sensitive(sensitivity.analyzer_mode);
    analyzer_style.set_sensitive(sensitivity.analyzer_style);
    peaks.set_sensitive(sensitivity.analyzer_peaks);
    analyzer_falloff.set_sensitive(sensitivity.analyzer_falloff);
    peaks_falloff.set_sensitive(sensitivity.peaks_falloff);
    scope_mode.set_sensitive(sensitivity.scope_mode);
    vu.set_sensitive(sensitivity.windowshade_vu);
    refresh.set_sensitive(sensitivity.refresh_rate);
}

fn falloff_combo(active: VisFalloffSpeed) -> gtk::ComboBoxText {
    let combo = gtk::ComboBoxText::new();
    for (id, label) in [
        ("slowest", "Slowest"),
        ("slow", "Slow"),
        ("medium", "Medium"),
        ("fast", "Fast"),
        ("fastest", "Fastest"),
    ] {
        combo.append(Some(id), label);
    }
    combo.set_active_id(Some(falloff_id(active)));
    combo
}

fn falloff_id(speed: VisFalloffSpeed) -> &'static str {
    match speed {
        VisFalloffSpeed::Slowest => "slowest",
        VisFalloffSpeed::Slow => "slow",
        VisFalloffSpeed::Fast => "fast",
        VisFalloffSpeed::Fastest => "fastest",
        VisFalloffSpeed::Medium => "medium",
    }
}

fn falloff_from_combo(combo: &gtk::ComboBoxText) -> VisFalloffSpeed {
    match combo.active_id().as_deref() {
        Some("slowest") => VisFalloffSpeed::Slowest,
        Some("slow") => VisFalloffSpeed::Slow,
        Some("fast") => VisFalloffSpeed::Fast,
        Some("fastest") => VisFalloffSpeed::Fastest,
        _ => VisFalloffSpeed::Medium,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptKind {
    OpenLocation,
    JumpTime,
}

impl PromptKind {
    fn title(self) -> &'static str {
        match self {
            Self::OpenLocation => "Play Location",
            Self::JumpTime => "Jump to Time",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::OpenLocation => "https://...",
            Self::JumpTime => "seconds or mm:ss",
        }
    }

    fn set_visible(self, state: &mut MainWindowUiState, visible: bool) {
        match self {
            Self::OpenLocation => state.set_open_location_visible(visible),
            Self::JumpTime => state.set_jump_time_visible(visible),
        }
    }

    fn accept(self, state: &mut MainWindowUiState, text: &str) {
        match self {
            Self::OpenLocation => state.accept_open_location(text),
            Self::JumpTime => state.accept_jump_time(text),
        }
    }
}

fn build_prompt_window(
    app: &gtk::Application,
    parent: &gtk::ApplicationWindow,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    kind: PromptKind,
) -> gtk::ApplicationWindow {
    let window = skinned_application_window(app, kind.title(), 360, 110, &[]);
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_resizable(false);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.add_css_class("xmms-skinned-window");
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let entry = gtk::Entry::builder()
        .placeholder_text(kind.placeholder())
        .build();
    content.append(&entry);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label("OK");
    buttons.append(&cancel);
    buttons.append(&ok);
    content.append(&buttons);
    window.set_child(Some(&content));

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.hide());
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let main_state = Rc::clone(main_state);
        ok.connect_clicked(move |_| {
            kind.accept(&mut main_state.borrow_mut(), entry.text().as_str());
            window.hide();
        });
    }
    {
        let main_state = Rc::clone(main_state);
        window.connect_close_request(move |window| {
            kind.set_visible(&mut main_state.borrow_mut(), false);
            window.hide();
            gtk::glib::Propagation::Stop
        });
    }
    {
        let main_state = Rc::clone(main_state);
        window.connect_hide(move |_| {
            kind.set_visible(&mut main_state.borrow_mut(), false);
        });
    }
    window
}

fn build_skin_browser_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> gtk::ApplicationWindow {
    let window = skinned_application_window(app, "Skin selector", 300, 280, &[]);
    let add = gtk::Button::with_label("Add...");
    add.set_widget_name(SKIN_BROWSER_ADD_WIDGET);
    let close = gtk::Button::with_label("Close");
    close.set_widget_name(SKIN_BROWSER_CLOSE_WIDGET);
    let (content, list) = build_skin_browser_content(&add, &close);
    window.set_child(Some(&content));
    let populating = Rc::new(Cell::new(false));

    {
        let window = window.clone();
        let main_state = Rc::clone(main_state);
        let list = list.clone();
        let populating = Rc::clone(&populating);
        add.connect_clicked(move |_| {
            show_add_skin_dialog(
                &window,
                Rc::clone(&main_state),
                list.clone(),
                Rc::clone(&populating),
            );
        });
    }
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.hide());
    }
    {
        let main_state = Rc::clone(main_state);
        let populating = Rc::clone(&populating);
        let list = list.clone();
        window.connect_show(move |_| {
            let dirs = runtime_skin_browser_dirs();
            populating.set(true);
            if let Err(err) = refresh_skin_browser_list(&list, &mut main_state.borrow_mut(), &dirs)
            {
                eprintln!("xmms-rs: failed to scan skins: {err}");
            }
            populating.set(false);
        });
    }
    connect_skin_browser_selection(
        &list,
        main_state,
        main_area,
        equalizer_area,
        playlist_area,
        &populating,
    );
    {
        let main_state = Rc::clone(main_state);
        window.connect_close_request(move |window| {
            main_state.borrow_mut().set_skin_browser_visible(false);
            window.hide();
            gtk::glib::Propagation::Stop
        });
    }
    {
        let main_state = Rc::clone(main_state);
        window.connect_hide(move |_| {
            main_state.borrow_mut().set_skin_browser_visible(false);
        });
    }
    window
}

fn build_skin_editor_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> gtk::ApplicationWindow {
    let window = skinned_application_window(app, "Skin Editor (alpha)", 980, 700, &[]);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    root.add_css_class("xmms-skinned-window");
    root.set_margin_top(8);
    root.set_margin_bottom(8);
    root.set_margin_start(8);
    root.set_margin_end(8);

    let canvas = gtk::DrawingArea::builder().focusable(true).build();
    update_skin_editor_canvas_size(&canvas, &main_state.borrow());
    {
        let main_state = Rc::clone(main_state);
        canvas.set_draw_func(move |_area, cr, _width, _height| {
            if let Err(err) = draw_skin_editor_canvas(cr, &main_state.borrow()) {
                eprintln!("xmms-rs: failed to draw skin editor: {err}");
            }
        });
    }

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hexpand(true);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&canvas));
    root.append(&scrolled);

    let (tools, zoom_scale, color_controls) = build_skin_editor_tools(
        &window,
        &canvas,
        main_state,
        main_area,
        equalizer_area,
        playlist_area,
    );
    let tool_scroller = gtk::ScrolledWindow::new();
    tool_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    tool_scroller.set_hexpand(false);
    tool_scroller.set_halign(gtk::Align::Start);
    tool_scroller.set_propagate_natural_width(false);
    tool_scroller.set_min_content_width(SKIN_EDITOR_SIDEBAR_WIDTH);
    tool_scroller.set_max_content_width(SKIN_EDITOR_SIDEBAR_WIDTH);
    tool_scroller.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    tool_scroller.set_child(Some(&tools));
    root.append(&tool_scroller);

    let pan_drag = Rc::new(Cell::new(None::<SkinEditorPanDrag>));
    let canvas_hadjustment = scrolled.hadjustment();
    let canvas_vadjustment = scrolled.vadjustment();

    let click = gtk::GestureClick::new();
    click.set_button(1);
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let color_controls = color_controls.clone();
        let pan_drag = Rc::clone(&pan_drag);
        let canvas_hadjustment = canvas_hadjustment.clone();
        let canvas_vadjustment = canvas_vadjustment.clone();
        click.connect_pressed(move |gesture, _n_press, x, y| {
            if main_state.borrow().skin_editor().tool == Tool::Drag {
                let (start_x, start_y) = gesture
                    .current_event()
                    .and_then(|event| event.position())
                    .unwrap_or((x, y));
                pan_drag.set(Some(SkinEditorPanDrag {
                    start_x,
                    start_y,
                    start_hadjustment: canvas_hadjustment.value(),
                    start_vadjustment: canvas_vadjustment.value(),
                }));
                return;
            }
            let (changed, picked_color) = {
                let mut state = main_state.borrow_mut();
                let previous_color = state.skin_editor().color;
                let slots = state.skin_editor.layout(state.active_skin());
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.press(state.active_skin_mut(), &slots, x, y);
                let picked_color = (editor.color != previous_color).then_some(editor.color);
                state.skin_editor = editor;
                (changed, picked_color)
            };
            if let Some(color) = picked_color {
                set_skin_editor_color_controls(&color_controls, color);
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let color_controls = color_controls.clone();
        let pan_drag = Rc::clone(&pan_drag);
        click.connect_released(move |_gesture, _n_press, x, y| {
            if pan_drag.take().is_some() {
                return;
            }
            let (changed, picked_color) = {
                let mut state = main_state.borrow_mut();
                let previous_color = state.skin_editor().color;
                let slots = state.skin_editor.layout(state.active_skin());
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.release(state.active_skin_mut(), &slots, x, y);
                let picked_color = (editor.color != previous_color).then_some(editor.color);
                state.skin_editor = editor;
                (changed, picked_color)
            };
            if let Some(color) = picked_color {
                set_skin_editor_color_controls(&color_controls, color);
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    canvas.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let color_controls = color_controls.clone();
        let pan_drag = Rc::clone(&pan_drag);
        let canvas_hadjustment = canvas_hadjustment.clone();
        let canvas_vadjustment = canvas_vadjustment.clone();
        motion.connect_motion(move |motion, x, y| {
            if let Some(pan) = pan_drag.get() {
                let (current_x, current_y) = motion
                    .current_event()
                    .and_then(|event| event.position())
                    .unwrap_or((x, y));
                set_adjustment_value(
                    &canvas_hadjustment,
                    pan.start_hadjustment + pan.start_x - current_x,
                );
                set_adjustment_value(
                    &canvas_vadjustment,
                    pan.start_vadjustment + pan.start_y - current_y,
                );
                return;
            }
            let (changed, picked_color) = {
                let mut state = main_state.borrow_mut();
                let previous_color = state.skin_editor().color;
                let slots = state.skin_editor.layout(state.active_skin());
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.drag(state.active_skin_mut(), &slots, x, y);
                let picked_color = (editor.color != previous_color).then_some(editor.color);
                state.skin_editor = editor;
                (changed, picked_color)
            };
            if let Some(color) = picked_color {
                set_skin_editor_color_controls(&color_controls, color);
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    canvas.add_controller(motion);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let zoom_scale = zoom_scale.clone();
        scroll.connect_scroll(move |_scroll, _dx, dy| {
            let zoom = {
                let mut state = main_state.borrow_mut();
                let current = state.skin_editor().zoom;
                let zoom = if dy < 0.0 {
                    current + ZOOM_STEP
                } else if dy > 0.0 {
                    current - ZOOM_STEP
                } else {
                    current
                };
                state.skin_editor_mut().set_zoom(zoom);
                update_skin_editor_canvas_size(&canvas, &state);
                state.skin_editor().zoom
            };
            if (zoom_scale.value() - zoom).abs() > f64::EPSILON {
                zoom_scale.set_value(zoom);
            }
            canvas.queue_draw();
            gtk::glib::Propagation::Stop
        });
    }
    canvas.add_controller(scroll);

    {
        let main_state = Rc::clone(main_state);
        window.connect_close_request(move |window| {
            main_state.borrow_mut().set_skin_editor_visible(false);
            window.hide();
            gtk::glib::Propagation::Stop
        });
    }
    {
        let main_state = Rc::clone(main_state);
        window.connect_hide(move |_| {
            main_state.borrow_mut().set_skin_editor_visible(false);
        });
    }

    window.set_child(Some(&root));
    window
}

#[derive(Clone)]
struct SkinEditorColorControls {
    chooser: gtk::ColorChooserWidget,
    button: gtk::Button,
}

#[derive(Clone, Copy)]
struct SkinEditorPanDrag {
    start_x: f64,
    start_y: f64,
    start_hadjustment: f64,
    start_vadjustment: f64,
}

fn build_skin_editor_tools(
    window: &gtk::ApplicationWindow,
    canvas: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> (gtk::Box, gtk::Scale, SkinEditorColorControls) {
    let tools = gtk::Box::new(gtk::Orientation::Vertical, 8);
    tools.set_hexpand(false);
    tools.set_halign(gtk::Align::Start);
    tools.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);

    let title = gtk::Label::new(Some("Tools"));
    title.set_xalign(0.0);
    tools.append(&title);

    let tool_flow = gtk::FlowBox::new();
    tool_flow.set_selection_mode(gtk::SelectionMode::None);
    tool_flow.set_min_children_per_line(1);
    tool_flow.set_max_children_per_line(SKIN_EDITOR_COLOR_SHELF_COLUMNS as u32);
    tool_flow.set_column_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    tool_flow.set_row_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    tool_flow.set_hexpand(false);
    let mut tool_group = None;
    for tool in [
        Tool::Brush,
        Tool::SprayCan,
        Tool::Fill,
        Tool::Line,
        Tool::Rectangle,
        Tool::Selection,
        Tool::Lighten,
        Tool::Darken,
        Tool::Dither,
        Tool::ColorPicker,
        Tool::Drag,
    ] {
        let button =
            append_skin_editor_tool(&tool_flow, tool_group.as_ref(), tool, main_state, canvas);
        if tool_group.is_none() {
            tool_group = Some(button);
        }
    }
    tools.append(&tool_flow);

    let fill_rectangle = gtk::CheckButton::with_label("Fill rectangle");
    fill_rectangle.set_active(true);
    {
        let main_state = Rc::clone(main_state);
        fill_rectangle.connect_toggled(move |button| {
            main_state.borrow_mut().skin_editor_mut().fill_rectangle = button.is_active();
        });
    }
    tools.append(&fill_rectangle);

    let brush_size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 15.0, 1.0);
    brush_size.set_hexpand(false);
    brush_size.set_halign(gtk::Align::Start);
    brush_size.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    brush_size.set_value(1.0);
    brush_size.set_digits(0);
    brush_size.set_draw_value(true);
    append_labeled_control(&tools, "Brush size (1-15)", &brush_size);
    {
        let main_state = Rc::clone(main_state);
        brush_size.connect_value_changed(move |scale| {
            main_state
                .borrow_mut()
                .skin_editor_mut()
                .set_brush_size(scale.value().round().clamp(1.0, 15.0) as u32);
        });
    }

    let zoom = gtk::Scale::with_range(gtk::Orientation::Horizontal, MIN_ZOOM, MAX_ZOOM, ZOOM_STEP);
    zoom.set_hexpand(false);
    zoom.set_halign(gtk::Align::Start);
    zoom.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    zoom.set_value(main_state.borrow().skin_editor().zoom);
    zoom.set_digits(2);
    zoom.set_draw_value(true);
    append_labeled_control(&tools, "Zoom (1x-10x)", &zoom);
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        zoom.connect_value_changed(move |scale| {
            {
                let mut state = main_state.borrow_mut();
                state
                    .skin_editor_mut()
                    .set_zoom(scale.value().clamp(MIN_ZOOM, MAX_ZOOM));
                update_skin_editor_canvas_size(&canvas, &state);
            }
            canvas.queue_draw();
        });
    }

    let color_button = gtk::Button::with_label("Custom color");
    color_button.set_hexpand(false);
    color_button.set_halign(gtk::Align::Start);
    color_button.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    color_button.set_tooltip_text(Some("Open custom color chooser"));
    style_skin_editor_custom_color_button(&color_button, [0, 0, 0, 255]);
    append_labeled_control(&tools, "Color", &color_button);

    let color_popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .build();
    color_popover.set_parent(&color_button);
    let color = gtk::ColorChooserWidget::new();
    color.set_rgba(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
    color.set_use_alpha(true);
    color.set_show_editor(true);
    color.set_size_request(220, 200);
    color_popover.set_child(Some(&color));
    {
        let color_popover = color_popover.clone();
        color_button.connect_clicked(move |_| {
            color_popover.popup();
        });
    }
    {
        let color_button = color_button.clone();
        let main_state = Rc::clone(main_state);
        color.connect_rgba_notify(move |chooser| {
            let rgba = rgba_to_u8(chooser.rgba());
            main_state.borrow_mut().skin_editor_mut().color = rgba;
            style_skin_editor_custom_color_button(&color_button, rgba);
        });
    }

    let color_controls = SkinEditorColorControls {
        chooser: color.clone(),
        button: color_button.clone(),
    };
    build_skin_editor_color_shelf(&tools, &color_controls, main_state);
    build_skin_editor_gradient_control(&tools, &color_controls, main_state);

    let couple_controls = gtk::CheckButton::with_label("Couple frames");
    couple_controls.set_tooltip_text(Some(
        "When painting volume, balance, equalizer, or shaded equalizer snippets, apply the same edit to later value frames",
    ));
    {
        let main_state = Rc::clone(main_state);
        couple_controls.connect_toggled(move |button| {
            main_state
                .borrow_mut()
                .skin_editor_mut()
                .couple_control_edits = button.is_active();
        });
    }
    tools.append(&couple_controls);

    let couple_gradient = gtk::CheckButton::with_label("Gradient coupling");
    couple_gradient.set_tooltip_text(Some(
        "When coupled edits are enabled, color each coupled value frame from the gradient",
    ));
    {
        let main_state = Rc::clone(main_state);
        couple_gradient.connect_toggled(move |button| {
            main_state
                .borrow_mut()
                .skin_editor_mut()
                .coupled_edits_use_gradient = button.is_active();
        });
    }
    tools.append(&couple_gradient);

    let transparent = gtk::CheckButton::with_label("Paint transparent");
    {
        let main_state = Rc::clone(main_state);
        let color = color.clone();
        let color_button = color_button.clone();
        transparent.connect_toggled(move |button| {
            let mut state = main_state.borrow_mut();
            if button.is_active() {
                state.skin_editor_mut().color[3] = 0;
                let color = state.skin_editor().color;
                style_skin_editor_custom_color_button(&color_button, color);
            } else {
                let rgba = rgba_to_u8(color.rgba());
                state.skin_editor_mut().color = rgba;
                style_skin_editor_custom_color_button(&color_button, rgba);
            }
        });
    }
    tools.append(&transparent);

    let selection_actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let copy = gtk::Button::with_label("Copy");
    let cut = gtk::Button::with_label("Cut");
    let paste = gtk::Button::with_label("Paste");
    {
        let main_state = Rc::clone(main_state);
        copy.connect_clicked(move |_| {
            let mut state = main_state.borrow_mut();
            let mut editor = std::mem::take(&mut state.skin_editor);
            editor.copy_selection(state.active_skin());
            state.skin_editor = editor;
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        cut.connect_clicked(move |_| {
            let changed = {
                let mut state = main_state.borrow_mut();
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.cut_selection(state.active_skin_mut());
                state.skin_editor = editor;
                changed
            };
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        paste.connect_clicked(move |_| {
            let changed = {
                let mut state = main_state.borrow_mut();
                let editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.paste_clipboard(state.active_skin_mut());
                state.skin_editor = editor;
                changed
            };
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    selection_actions.append(&copy);
    selection_actions.append(&cut);
    selection_actions.append(&paste);
    tools.append(&selection_actions);

    tools.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let colors_title = gtk::Label::new(Some("Skin color swatches"));
    colors_title.set_xalign(0.0);
    tools.append(&colors_title);
    build_skin_color_swatches(
        &tools,
        canvas,
        main_state,
        main_area,
        equalizer_area,
        playlist_area,
    );

    let name = gtk::Entry::new();
    let initial_name = main_state.borrow().skin_editor().working_name.clone();
    name.set_text(&initial_name);
    append_labeled_control(&tools, "Skin name", &name);
    {
        let main_state = Rc::clone(main_state);
        name.connect_changed(move |entry| {
            main_state.borrow_mut().skin_editor_mut().working_name = entry.text().to_string();
        });
    }

    let clone_current = gtk::Button::with_label("Clone Current");
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let name = name.clone();
        clone_current.connect_clicked(move |_| {
            let cloned_name = {
                let mut state = main_state.borrow_mut();
                match state.clone_configured_skin_for_editor() {
                    Ok(()) => Some(state.skin_editor().working_name.clone()),
                    Err(err) => {
                        eprintln!("xmms-rs: failed to clone skin for editor: {err}");
                        None
                    }
                }
            };
            if let Some(cloned_name) = cloned_name {
                name.set_text(&cloned_name);
                refresh_xmms_skin_css(main_state.borrow().active_skin());
                queue_skin_editor_areas(&canvas, &main_area, &equalizer_area, &playlist_area, true);
            }
        });
    }
    tools.append(&clone_current);

    let save = gtk::Button::with_label("Save");
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        save.connect_clicked(move |_| {
            if let Err(err) = main_state.borrow_mut().save_editor_skin_to_user_dir() {
                eprintln!("xmms-rs: failed to save edited skin: {err}");
            }
            refresh_xmms_skin_css(main_state.borrow().active_skin());
            queue_skin_editor_areas(&canvas, &main_area, &equalizer_area, &playlist_area, true);
        });
    }
    tools.append(&save);

    let export = gtk::Button::with_label("Export .wsz");
    {
        let window = window.clone();
        let main_state = Rc::clone(main_state);
        export.connect_clicked(move |_| {
            show_skin_editor_export_dialog(&window, Rc::clone(&main_state));
        });
    }
    tools.append(&export);

    (tools, zoom, color_controls)
}

fn append_skin_editor_tool(
    parent: &gtk::FlowBox,
    group: Option<&gtk::ToggleButton>,
    tool: Tool,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    canvas: &gtk::DrawingArea,
) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::with_label(skin_editor_tool_icon(tool));
    if let Some(group) = group {
        button.set_group(Some(group));
    }
    button.set_active(tool == Tool::Brush);
    button.set_tooltip_text(Some(skin_editor_tool_name(tool)));
    button.set_size_request(
        SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
        SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
    );
    button.set_hexpand(false);
    button.set_vexpand(false);
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        button.connect_toggled(move |button| {
            if button.is_active() {
                main_state.borrow_mut().skin_editor_mut().tool = tool;
                canvas.queue_draw();
            }
        });
    }
    parent.insert(&button, -1);
    button
}

fn skin_editor_tool_icon(tool: Tool) -> &'static str {
    match tool {
        Tool::Brush => "🖌️",
        Tool::SprayCan => "💨",
        Tool::Fill => "🪣",
        Tool::Line => "📏",
        Tool::Rectangle => "🟦",
        Tool::Selection => "🔲",
        Tool::Lighten => "🔆",
        Tool::Darken => "🌙",
        Tool::Dither => "🏁",
        Tool::ColorPicker => "🧪",
        Tool::Drag => "✋",
    }
}

fn skin_editor_tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Brush => "Brush",
        Tool::SprayCan => "Spraycan",
        Tool::Fill => "Color fill",
        Tool::Line => "Line",
        Tool::Rectangle => "Rectangle",
        Tool::Selection => "Select rectangle",
        Tool::Lighten => "Lighten",
        Tool::Darken => "Darken",
        Tool::Dither => "Dither checker brush",
        Tool::ColorPicker => "Color picker",
        Tool::Drag => "Drag canvas",
    }
}

fn set_adjustment_value(adjustment: &gtk::Adjustment, value: f64) {
    let upper = adjustment.upper() - adjustment.page_size();
    adjustment.set_value(value.clamp(adjustment.lower(), upper.max(adjustment.lower())));
}

fn build_skin_editor_color_shelf(
    parent: &gtk::Box,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let label = gtk::Label::new(Some("Color shelf"));
    label.set_xalign(0.0);
    parent.append(&label);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    grid.set_row_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    let initial_shelf = main_state.borrow().skin_editor().color_shelf;
    for index in 0..COLOR_SHELF_SIZE {
        let color = initial_shelf[index];
        let button = gtk::Button::new();
        button.set_size_request(
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
        );
        button.set_tooltip_text(Some(
            "Left click picks; middle or right click stores current color",
        ));
        style_color_shelf_button(&button, color);

        {
            let main_state = Rc::clone(main_state);
            let color_controls = color_controls.clone();
            button.connect_clicked(move |_| {
                let picked = main_state
                    .borrow_mut()
                    .skin_editor_mut()
                    .pick_color_shelf_slot(index);
                if let Some(picked) = picked {
                    set_skin_editor_color_controls(&color_controls, picked);
                }
            });
        }

        for button_number in [2, 3] {
            let click = gtk::GestureClick::new();
            click.set_button(button_number);
            {
                let main_state = Rc::clone(main_state);
                let shelf_button = button.clone();
                click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                    let stored = main_state
                        .borrow_mut()
                        .skin_editor_mut()
                        .store_color_shelf_slot(index);
                    style_color_shelf_button(&shelf_button, stored);
                });
            }
            button.add_controller(click);
        }

        grid.attach(
            &button,
            (index % SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            (index / SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            1,
            1,
        );
    }
    parent.append(&grid);
}

fn build_skin_editor_gradient_control(
    parent: &gtk::Box,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let label = gtk::Label::new(Some("Gradient"));
    label.set_xalign(0.0);
    label.set_margin_top(4);
    parent.append(&label);

    let gradient = gtk::DrawingArea::builder()
        .content_width(SKIN_EDITOR_GRADIENT_WIDTH)
        .content_height(SKIN_EDITOR_GRADIENT_HEIGHT)
        .build();
    gradient.set_size_request(SKIN_EDITOR_GRADIENT_WIDTH, SKIN_EDITOR_GRADIENT_HEIGHT);
    gradient.set_tooltip_text(Some(
        "Click to pick an interpolated color; use + Stop to add editable colors",
    ));
    {
        let main_state = Rc::clone(main_state);
        gradient.set_draw_func(move |_area, cr, width, height| {
            let state = main_state.borrow();
            if let Err(err) =
                draw_skin_editor_gradient(cr, width, height, &state.skin_editor().gradient)
            {
                eprintln!("xmms-rs: failed to draw skin editor gradient: {err}");
            }
        });
    }
    {
        let gradient_for_click = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.connect_pressed(move |_gesture, _n_press, x, _y| {
            let width = f64::from(gradient_for_click.allocated_width().max(1));
            let fraction = (x / (width - 1.0).max(1.0)).clamp(0.0, 1.0);
            let picked = main_state
                .borrow_mut()
                .skin_editor_mut()
                .pick_gradient_color_at(fraction);
            set_skin_editor_color_controls(&color_controls, picked);
        });
        gradient.add_controller(click);
    }
    parent.append(&gradient);

    let stop_list = gtk::Box::new(gtk::Orientation::Vertical, 3);
    build_gradient_stop_list(&stop_list, &gradient, color_controls, main_state);
    parent.append(&stop_list);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let add_stop = gtk::Button::with_label("+ Stop");
    let remove_stop = gtk::Button::with_label("Remove");
    let reverse = gtk::Button::with_label("Reverse");
    actions.append(&add_stop);
    actions.append(&remove_stop);
    actions.append(&reverse);
    parent.append(&actions);

    {
        let stop_list = stop_list.clone();
        let gradient = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        add_stop.connect_clicked(move |_| {
            let color = {
                let mut state = main_state.borrow_mut();
                state.skin_editor_mut().add_gradient_stop(0.5);
                state.skin_editor().color
            };
            set_skin_editor_color_controls(&color_controls, color);
            gradient.queue_draw();
            build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
        });
    }
    {
        let stop_list = stop_list.clone();
        let gradient = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        remove_stop.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                let index = state.skin_editor().selected_gradient_stop();
                state.skin_editor_mut().remove_gradient_stop(index);
            }
            gradient.queue_draw();
            build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
        });
    }
    {
        let stop_list = stop_list.clone();
        let gradient = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        reverse.connect_clicked(move |_| {
            main_state.borrow_mut().skin_editor_mut().reverse_gradient();
            gradient.queue_draw();
            build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
        });
    }

    let shelf_label = gtk::Label::new(Some("Gradient shelf"));
    shelf_label.set_xalign(0.0);
    parent.append(&shelf_label);
    build_gradient_shelf(parent, &gradient, &stop_list, color_controls, main_state);
}

fn build_gradient_stop_list(
    stop_list: &gtk::Box,
    gradient: &gtk::DrawingArea,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    while let Some(child) = stop_list.first_child() {
        stop_list.remove(&child);
    }

    let (stops, selected) = {
        let state = main_state.borrow();
        (
            state.skin_editor().gradient_stops().to_vec(),
            state.skin_editor().selected_gradient_stop(),
        )
    };

    let stop_count = stops.len();
    for (index, stop) in stops.into_iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let label = gtk::Label::new(Some(if index == selected { "●" } else { "○" }));
        row.append(&label);

        let position = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        position.set_value(stop.position);
        position.set_digits(2);
        position.set_draw_value(true);
        position.set_hexpand(true);
        position.set_width_request(82);
        position.set_sensitive(index != 0 && index + 1 != stop_count);
        {
            let gradient = gradient.clone();
            let main_state = Rc::clone(main_state);
            position.connect_value_changed(move |scale| {
                main_state
                    .borrow_mut()
                    .skin_editor_mut()
                    .set_gradient_stop_position(index, scale.value());
                gradient.queue_draw();
            });
        }
        row.append(&position);

        let color_button = gtk::Button::new();
        color_button.set_size_request(28, 24);
        color_button.set_tooltip_text(Some(
            "Left click selects this stop; middle/right click stores current color",
        ));
        style_color_shelf_button(&color_button, Some(stop.color));
        {
            let main_state = Rc::clone(main_state);
            let color_controls = color_controls.clone();
            let stop_list = stop_list.clone();
            let gradient = gradient.clone();
            color_button.connect_clicked(move |_| {
                let picked = {
                    main_state
                        .borrow_mut()
                        .skin_editor_mut()
                        .select_gradient_stop(index)
                };
                if let Some(color) = picked {
                    set_skin_editor_color_controls(&color_controls, color);
                }
                queue_gradient_stop_list_rebuild(
                    &stop_list,
                    &gradient,
                    &color_controls,
                    &main_state,
                );
            });
        }
        for button_number in [2, 3] {
            let click = gtk::GestureClick::new();
            click.set_button(button_number);
            {
                let main_state = Rc::clone(main_state);
                let color_button = color_button.clone();
                let gradient = gradient.clone();
                click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                    {
                        let mut state = main_state.borrow_mut();
                        state.skin_editor_mut().selected_gradient_stop = index;
                        if let Some(color) = state.skin_editor_mut().store_selected_gradient_stop()
                        {
                            style_color_shelf_button(&color_button, Some(color));
                        }
                    }
                    gradient.queue_draw();
                });
            }
            color_button.add_controller(click);
        }
        row.append(&color_button);
        stop_list.append(&row);
    }
}

fn queue_gradient_stop_list_rebuild(
    stop_list: &gtk::Box,
    gradient: &gtk::DrawingArea,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let stop_list = stop_list.clone();
    let gradient = gradient.clone();
    let color_controls = color_controls.clone();
    let main_state = Rc::clone(main_state);
    gtk::glib::idle_add_local_once(move || {
        build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
    });
}

fn build_gradient_shelf(
    parent: &gtk::Box,
    gradient: &gtk::DrawingArea,
    stop_list: &gtk::Box,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let shelf_grid = gtk::Grid::new();
    shelf_grid.set_column_spacing(4);
    shelf_grid.set_row_spacing(4);
    let shelf_areas: Rc<RefCell<Vec<gtk::DrawingArea>>> = Rc::new(RefCell::new(Vec::new()));
    for index in 0..GRADIENT_SHELF_SIZE {
        let area = gtk::DrawingArea::builder()
            .content_width(SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE)
            .content_height(SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE)
            .build();
        area.set_size_request(
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
        );
        area.set_tooltip_text(Some(
            "Left click loads; middle or right click stores current gradient",
        ));
        {
            let main_state = Rc::clone(main_state);
            area.set_draw_func(move |_area, cr, width, height| {
                let state = main_state.borrow();
                let gradient = state.skin_editor().gradient_shelf[index].as_ref();
                if let Err(err) = draw_gradient_shelf_slot(cr, width, height, gradient) {
                    eprintln!("xmms-rs: failed to draw gradient shelf slot: {err}");
                }
            });
        }
        {
            let main_state = Rc::clone(main_state);
            let gradient_area = gradient.clone();
            let stop_list = stop_list.clone();
            let color_controls = color_controls.clone();
            let shelf_areas = Rc::clone(&shelf_areas);
            let click = gtk::GestureClick::new();
            click.set_button(1);
            click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                let picked = main_state
                    .borrow_mut()
                    .skin_editor_mut()
                    .pick_gradient_shelf_slot(index);
                if picked.is_some() {
                    let color = main_state.borrow().skin_editor().color;
                    set_skin_editor_color_controls(&color_controls, color);
                    gradient_area.queue_draw();
                    build_gradient_stop_list(
                        &stop_list,
                        &gradient_area,
                        &color_controls,
                        &main_state,
                    );
                    for area in shelf_areas.borrow().iter() {
                        area.queue_draw();
                    }
                }
            });
            area.add_controller(click);
        }
        for button_number in [2, 3] {
            let click = gtk::GestureClick::new();
            click.set_button(button_number);
            {
                let main_state = Rc::clone(main_state);
                let area = area.clone();
                click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                    main_state
                        .borrow_mut()
                        .skin_editor_mut()
                        .store_gradient_shelf_slot(index);
                    area.queue_draw();
                });
            }
            area.add_controller(click);
        }
        shelf_grid.attach(
            &area,
            (index % SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            (index / SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            1,
            1,
        );
        shelf_areas.borrow_mut().push(area);
    }
    parent.append(&shelf_grid);
}

fn draw_gradient_shelf_slot(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    gradient: Option<&SkinGradient>,
) -> Result<(), cairo::Error> {
    if let Some(gradient) = gradient {
        draw_skin_editor_gradient(cr, width, height, gradient)
    } else {
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        cr.paint()?;
        cr.set_source_rgb(0.45, 0.45, 0.45);
        cr.set_dash(&[3.0, 2.0], 0.0);
        cr.rectangle(
            0.5,
            0.5,
            f64::from(width.max(1)) - 1.0,
            f64::from(height.max(1)) - 1.0,
        );
        let result = cr.stroke();
        cr.set_dash(&[], 0.0);
        result
    }
}

fn draw_skin_editor_gradient(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    skin_gradient: &SkinGradient,
) -> Result<(), cairo::Error> {
    let width = width.max(1);
    let height = height.max(1);
    draw_alpha_checkerboard(cr, width, height)?;

    let gradient = cairo::LinearGradient::new(0.0, 0.0, f64::from(width), 0.0);
    for stop in skin_gradient.stops() {
        let [r, g, b, a] = rgba_to_cairo(stop.color);
        gradient.add_color_stop_rgba(stop.position, r, g, b, a);
    }
    cr.set_source(&gradient)?;
    cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    cr.fill()?;

    cr.set_source_rgb(0.12, 0.12, 0.12);
    cr.rectangle(0.5, 0.5, f64::from(width) - 1.0, f64::from(height) - 1.0);
    cr.stroke()
}

fn draw_alpha_checkerboard(
    cr: &cairo::Context,
    width: i32,
    height: i32,
) -> Result<(), cairo::Error> {
    const CELL: i32 = 6;
    cr.set_source_rgb(0.72, 0.72, 0.72);
    cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    cr.fill()?;
    for y in (0..height).step_by(CELL as usize) {
        for x in (0..width).step_by(CELL as usize) {
            if ((x / CELL) + (y / CELL)) & 1 == 0 {
                cr.set_source_rgb(0.48, 0.48, 0.48);
                cr.rectangle(
                    f64::from(x),
                    f64::from(y),
                    f64::from(CELL.min(width - x)),
                    f64::from(CELL.min(height - y)),
                );
                cr.fill()?;
            }
        }
    }
    Ok(())
}

fn rgba_to_cairo([r, g, b, a]: [u8; 4]) -> [f64; 4] {
    [
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        f64::from(a) / 255.0,
    ]
}

fn set_skin_editor_color_controls(controls: &SkinEditorColorControls, rgba: [u8; 4]) {
    controls.chooser.set_rgba(&rgba_from_u8(rgba));
    style_skin_editor_custom_color_button(&controls.button, rgba);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkinColorTarget {
    PlaylistNormal,
    PlaylistCurrent,
    PlaylistNormalBg,
    PlaylistSelectedBg,
    Visualization(usize),
    TextBackground(usize),
    TextForeground(usize),
}

impl SkinColorTarget {
    fn affects_playlist_colors(self) -> bool {
        matches!(
            self,
            Self::PlaylistNormal
                | Self::PlaylistCurrent
                | Self::PlaylistNormalBg
                | Self::PlaylistSelectedBg
        )
    }
}

fn build_skin_color_swatches(
    parent: &gtk::Box,
    canvas: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) {
    let playlist_grid = gtk::Grid::new();
    playlist_grid.set_column_spacing(4);
    playlist_grid.set_row_spacing(4);
    for (index, (label, target)) in [
        ("PL normal", SkinColorTarget::PlaylistNormal),
        ("PL current", SkinColorTarget::PlaylistCurrent),
        ("PL bg", SkinColorTarget::PlaylistNormalBg),
        ("PL selected", SkinColorTarget::PlaylistSelectedBg),
    ]
    .into_iter()
    .enumerate()
    {
        playlist_grid.attach(
            &skin_color_button(
                label,
                target,
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            (index % 2) as i32,
            (index / 2) as i32,
            1,
            1,
        );
    }
    parent.append(&playlist_grid);

    let vis_label = gtk::Label::new(Some("Visualizer colors"));
    vis_label.set_xalign(0.0);
    parent.append(&vis_label);
    let vis_grid = gtk::Grid::new();
    vis_grid.set_column_spacing(2);
    vis_grid.set_row_spacing(2);
    for index in 0..24 {
        vis_grid.attach(
            &skin_color_button(
                &(index + 1).to_string(),
                SkinColorTarget::Visualization(index),
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            (index % 6) as i32,
            (index / 6) as i32,
            1,
            1,
        );
    }
    parent.append(&vis_grid);

    let text_label = gtk::Label::new(Some("Text colors"));
    text_label.set_xalign(0.0);
    parent.append(&text_label);
    let text_grid = gtk::Grid::new();
    text_grid.set_column_spacing(2);
    text_grid.set_row_spacing(2);
    for index in 0..6 {
        text_grid.attach(
            &skin_color_button(
                &format!("BG{}", index + 1),
                SkinColorTarget::TextBackground(index),
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            0,
            index as i32,
            1,
            1,
        );
        text_grid.attach(
            &skin_color_button(
                &format!("FG{}", index + 1),
                SkinColorTarget::TextForeground(index),
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            1,
            index as i32,
            1,
            1,
        );
    }
    parent.append(&text_grid);
}

fn skin_color_button(
    label: &str,
    target: SkinColorTarget,
    canvas: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_tooltip_text(Some("Apply the current editor color to this skin color"));
    button.set_size_request(58, 28);
    style_skin_color_button(
        &button,
        skin_color_target_rgb(main_state.borrow().active_skin(), target),
    );
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let swatch_button = button.clone();
        button.connect_clicked(move |_| {
            let (changed, rgb) = {
                let mut state = main_state.borrow_mut();
                let color = state.skin_editor().color;
                let rgb = [color[0], color[1], color[2]];
                let changed = apply_skin_color_target(state.active_skin_mut(), target, rgb);
                (changed, rgb)
            };
            style_skin_color_button(&swatch_button, rgb);
            if changed && target.affects_playlist_colors() {
                refresh_xmms_skin_css(main_state.borrow().active_skin());
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    button
}

fn skin_color_target_rgb(skin: &DefaultSkin, target: SkinColorTarget) -> [u8; 3] {
    match target {
        SkinColorTarget::PlaylistNormal => skin.playlist_colors().normal,
        SkinColorTarget::PlaylistCurrent => skin.playlist_colors().current,
        SkinColorTarget::PlaylistNormalBg => skin.playlist_colors().normal_bg,
        SkinColorTarget::PlaylistSelectedBg => skin.playlist_colors().selected_bg,
        SkinColorTarget::Visualization(index) => {
            skin.vis_colors().get(index).copied().unwrap_or([0, 0, 0])
        }
        SkinColorTarget::TextBackground(index) => skin
            .text_colors()
            .background
            .get(index)
            .copied()
            .unwrap_or([0, 0, 0]),
        SkinColorTarget::TextForeground(index) => skin
            .text_colors()
            .foreground
            .get(index)
            .copied()
            .unwrap_or([255, 255, 255]),
    }
}

fn apply_skin_color_target(skin: &mut DefaultSkin, target: SkinColorTarget, rgb: [u8; 3]) -> bool {
    match target {
        SkinColorTarget::PlaylistNormal => {
            let mut colors = skin.playlist_colors();
            colors.normal = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::PlaylistCurrent => {
            let mut colors = skin.playlist_colors();
            colors.current = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::PlaylistNormalBg => {
            let mut colors = skin.playlist_colors();
            colors.normal_bg = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::PlaylistSelectedBg => {
            let mut colors = skin.playlist_colors();
            colors.selected_bg = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::Visualization(index) => skin.set_vis_color(index, rgb),
        SkinColorTarget::TextBackground(index) => {
            let mut colors = skin.text_colors();
            if let Some(color) = colors.background.get_mut(index) {
                *color = rgb;
                skin.set_text_colors(colors)
            } else {
                false
            }
        }
        SkinColorTarget::TextForeground(index) => {
            let mut colors = skin.text_colors();
            if let Some(color) = colors.foreground.get_mut(index) {
                *color = rgb;
                skin.set_text_colors(colors)
            } else {
                false
            }
        }
    }
}

fn append_labeled_control<W: IsA<gtk::Widget>>(parent: &gtk::Box, label: &str, child: &W) {
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    parent.append(&label);
    parent.append(child);
}

fn draw_skin_editor_canvas(cr: &cairo::Context, state: &MainWindowUiState) -> Result<(), String> {
    cr.set_source_rgb(0.12, 0.12, 0.12);
    cr.paint().map_err(|err| err.to_string())?;
    let editor = state.skin_editor();
    let zoom = editor.zoom.max(MIN_ZOOM);
    let slots = editor.layout(state.active_skin());

    cr.save().map_err(|err| err.to_string())?;
    cr.scale(zoom, zoom);
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(8.0);

    for slot in &slots {
        cr.set_source_rgb(0.85, 0.85, 0.85);
        cr.move_to(f64::from(slot.x), f64::from(slot.y - 3));
        cr.show_text(&format!(
            "{}  {}x{}",
            slot.kind.info().file_stem,
            slot.width,
            slot.height
        ))
        .map_err(|err| err.to_string())?;

        cr.set_source_rgb(0.25, 0.25, 0.25);
        cr.rectangle(
            f64::from(slot.x) - 1.0,
            f64::from(slot.y) - 1.0,
            f64::from(slot.width) + 2.0,
            f64::from(slot.height) + 2.0,
        );
        cr.stroke().map_err(|err| err.to_string())?;

        if let Some(image) = state.active_skin().get(slot.kind) {
            let surface = surface_from_xpm(image).map_err(|err| err.to_string())?;
            blit_surface_rect(cr, &surface, 0, 0, slot.x, slot.y, slot.width, slot.height)
                .map_err(|err| err.to_string())?;
        }
    }

    if editor.zoom >= 8.0 {
        draw_skin_editor_grid(cr, &slots).map_err(|err| err.to_string())?;
    }
    draw_skin_editor_line_preview(cr, editor, &slots).map_err(|err| err.to_string())?;
    draw_skin_editor_rectangle_preview(cr, editor, &slots).map_err(|err| err.to_string())?;
    draw_skin_editor_selection_preview(cr, editor, &slots).map_err(|err| err.to_string())?;
    cr.restore().map_err(|err| err.to_string())?;
    Ok(())
}

fn draw_skin_editor_grid(cr: &cairo::Context, slots: &[ElementSlot]) -> Result<(), cairo::Error> {
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
    cr.set_line_width(0.1);
    for slot in slots {
        for x in 0..=slot.width {
            cr.move_to(f64::from(slot.x + x), f64::from(slot.y));
            cr.line_to(f64::from(slot.x + x), f64::from(slot.y + slot.height));
        }
        for y in 0..=slot.height {
            cr.move_to(f64::from(slot.x), f64::from(slot.y + y));
            cr.line_to(f64::from(slot.x + slot.width), f64::from(slot.y + y));
        }
    }
    cr.stroke()
}

fn draw_skin_editor_rectangle_preview(
    cr: &cairo::Context,
    editor: &SkinEditorState,
    slots: &[ElementSlot],
) -> Result<(), cairo::Error> {
    let Some((kind, rect)) = editor.rectangle_preview() else {
        return Ok(());
    };
    let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
        return Ok(());
    };
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
    cr.set_line_width(1.0);
    cr.rectangle(
        f64::from(slot.x + rect.x),
        f64::from(slot.y + rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    );
    cr.stroke()
}

fn draw_skin_editor_line_preview(
    cr: &cairo::Context,
    editor: &SkinEditorState,
    slots: &[ElementSlot],
) -> Result<(), cairo::Error> {
    let Some((kind, start, end)) = editor.line_preview() else {
        return Ok(());
    };
    let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
        return Ok(());
    };
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
    cr.set_line_width(1.0);
    cr.move_to(
        f64::from(slot.x + start.0) + 0.5,
        f64::from(slot.y + start.1) + 0.5,
    );
    cr.line_to(
        f64::from(slot.x + end.0) + 0.5,
        f64::from(slot.y + end.1) + 0.5,
    );
    cr.stroke()
}

fn draw_skin_editor_selection_preview(
    cr: &cairo::Context,
    editor: &SkinEditorState,
    slots: &[ElementSlot],
) -> Result<(), cairo::Error> {
    let Some((kind, rect)) = editor.selection_preview() else {
        return Ok(());
    };
    let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
        return Ok(());
    };
    cr.set_source_rgba(0.2, 0.7, 1.0, 0.9);
    cr.set_line_width(1.0);
    cr.set_dash(&[2.0, 2.0], 0.0);
    cr.rectangle(
        f64::from(slot.x + rect.x),
        f64::from(slot.y + rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    );
    let result = cr.stroke();
    cr.set_dash(&[], 0.0);
    result
}

fn update_skin_editor_canvas_size(canvas: &gtk::DrawingArea, state: &MainWindowUiState) {
    let slots = state.skin_editor().layout(state.active_skin());
    let (width, height) = state.skin_editor().canvas_size(&slots);
    canvas.set_content_width(width);
    canvas.set_content_height(height);
}

fn queue_skin_editor_areas(
    canvas: &gtk::DrawingArea,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
    skin_changed: bool,
) {
    canvas.queue_draw();
    if skin_changed {
        main_area.queue_draw();
        equalizer_area.queue_draw();
        playlist_area.queue_draw();
    }
}

fn rgba_to_u8(rgba: gtk::gdk::RGBA) -> [u8; 4] {
    [
        (rgba.red().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (rgba.green().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (rgba.blue().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (rgba.alpha().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    ]
}

fn rgba_from_u8(rgba: [u8; 4]) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        f32::from(rgba[0]) / 255.0,
        f32::from(rgba[1]) / 255.0,
        f32::from(rgba[2]) / 255.0,
        f32::from(rgba[3]) / 255.0,
    )
}

fn show_skin_editor_export_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Export Skin"),
        Some(parent),
        gtk::FileChooserAction::Save,
        Some("Export"),
        Some("Cancel"),
    );
    dialog.set_current_name("skin.wsz");
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|file| file.path()) {
                let path = ensure_wsz_extension(path);
                if let Err(err) = main_state.borrow().export_editor_skin_wsz(&path) {
                    eprintln!("xmms-rs: failed to export skin '{}': {err}", path.display());
                }
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn connect_skin_browser_selection(
    list: &gtk::ListBox,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
    populating: &Rc<Cell<bool>>,
) {
    let main_state = Rc::clone(main_state);
    let main_area = main_area.clone();
    let equalizer_area = equalizer_area.clone();
    let playlist_area = playlist_area.clone();
    let populating = Rc::clone(populating);
    list.connect_row_selected(move |list, row| {
        if populating.get() {
            return;
        }
        let Some(row) = row else {
            return;
        };
        let selected = row.index().max(0) as usize;
        if main_state.borrow_mut().select_skin_browser_index(selected) {
            refresh_xmms_skin_css(main_state.borrow().active_skin());
            main_area.queue_draw();
            equalizer_area.queue_draw();
            playlist_area.queue_draw();
        } else {
            populating.set(true);
            populate_skin_browser_list(list, &main_state.borrow());
            populating.set(false);
        }
    });
}

fn show_add_skin_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    list: gtk::ListBox,
    populating: Rc<Cell<bool>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Add Skin"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Add"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|file| file.path()) {
                let user_skin_dir = user_skin_import_dir();
                match import_skin_to_user_dir(&path, &user_skin_dir) {
                    Ok(imported) => {
                        let dirs = runtime_skin_browser_dirs();
                        let mut state = main_state.borrow_mut();
                        state.app_state.config.skin = Some(imported.display().to_string());
                        if let Err(err) = state.reload_skin() {
                            eprintln!("xmms-rs: failed to load imported skin: {err}");
                            state.app_state.config.skin = None;
                        }
                        refresh_xmms_skin_css(state.active_skin());
                        populating.set(true);
                        if let Err(err) = refresh_skin_browser_list(&list, &mut state, &dirs) {
                            eprintln!("xmms-rs: failed to refresh skins after import: {err}");
                        }
                        populating.set(false);
                    }
                    Err(err) => eprintln!("xmms-rs: failed to import skin: {err}"),
                }
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn build_skin_browser_content(add: &gtk::Button, close: &gtk::Button) -> (gtk::Box, gtk::ListBox) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 5);
    root.add_css_class("xmms-skinned-window");
    root.set_widget_name(SKIN_BROWSER_ROOT_WIDGET);
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);

    let header = gtk::Label::new(Some("Skins"));
    header.set_widget_name(SKIN_BROWSER_HEADER_WIDGET);
    header.set_xalign(0.0);

    let list = gtk::ListBox::new();
    list.set_widget_name(SKIN_BROWSER_LIST_WIDGET);
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_vexpand(true);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Always);
    scrolled.set_min_content_width(250);
    scrolled.set_min_content_height(200);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&list));

    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    buttons.set_halign(gtk::Align::End);
    buttons.append(add);
    buttons.append(close);

    root.append(&header);
    root.append(&scrolled);
    root.append(&separator);
    root.append(&buttons);
    (root, list)
}

fn user_skin_import_dir() -> PathBuf {
    default_config_dir().join("xmms").join("Skins")
}

fn runtime_skin_browser_dirs() -> Vec<PathBuf> {
    let home_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let system_skin_dir = std::env::var_os("XMMS_RS_SYSTEM_SKIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/xmms/Skins"));
    let skinsdir = std::env::var("SKINSDIR").ok();
    skin_browser_search_dirs(
        &default_config_dir(),
        &home_dir,
        &system_skin_dir,
        skinsdir.as_deref(),
    )
}

fn import_skin_to_user_dir(source: &Path, user_skin_dir: &Path) -> io::Result<PathBuf> {
    if !source.is_dir() && !source.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a skin file or directory: {}", source.display()),
        ));
    }
    if source.is_file() && !is_importable_skin_archive(source) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported skin archive: {}", source.display()),
        ));
    }

    fs::create_dir_all(user_skin_dir)?;
    let name = source.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("skin path has no file name: {}", source.display()),
        )
    })?;
    let destination = unique_import_destination(user_skin_dir, name);
    if source.is_dir() {
        copy_dir_recursive(source, &destination)?;
    } else {
        fs::copy(source, &destination)?;
    }
    Ok(destination)
}

fn is_importable_skin_archive(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    [
        ".zip", ".wsz", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".gz", ".bz2",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
}

fn unique_import_destination(user_skin_dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let candidate = user_skin_dir.join(name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Skin");
    let extension = path.extension().and_then(|extension| extension.to_str());
    for index in 1.. {
        let file_name = match extension {
            Some(extension) => format!("{stem} {index}.{extension}"),
            None => format!("{stem} {index}"),
        };
        let candidate = user_skin_dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn sanitized_skin_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_string();
    if sanitized.is_empty() {
        "Edited Skin".to_string()
    } else {
        sanitized
    }
}

fn ensure_wsz_extension(mut path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wsz"))
    {
        return path;
    }
    path.set_extension("wsz");
    path
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_source = entry.path();
        let entry_destination = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry_source, &entry_destination)?;
        } else {
            fs::copy(entry_source, entry_destination)?;
        }
    }
    Ok(())
}

fn refresh_skin_browser_list<P: AsRef<Path>>(
    list: &gtk::ListBox,
    state: &mut MainWindowUiState,
    dirs: &[P],
) -> io::Result<()> {
    state.scan_skin_browser_dirs(dirs)?;
    populate_skin_browser_list(list, state);
    Ok(())
}

fn populate_skin_browser_list(list: &gtk::ListBox, state: &MainWindowUiState) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    append_skin_browser_row(list, "default");
    for entry in state.skin_browser_entries() {
        append_skin_browser_row(list, &entry.name);
    }

    if let Some(row) = list.row_at_index(state.selected_skin_index() as i32) {
        list.select_row(Some(&row));
    }
}

fn append_skin_browser_row(list: &gtk::ListBox, label: &str) {
    let row_label = gtk::Label::new(Some(label));
    row_label.set_xalign(0.0);
    row_label.set_margin_top(2);
    row_label.set_margin_bottom(2);
    row_label.set_margin_start(4);
    row_label.set_margin_end(4);
    list.append(&row_label);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Equalizer,
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum KeyboardFocus {
    #[default]
    Main,
    Equalizer,
    Playlist,
}

impl From<PanelKind> for KeyboardFocus {
    fn from(kind: PanelKind) -> Self {
        match kind {
            PanelKind::Equalizer => Self::Equalizer,
            PanelKind::Playlist => Self::Playlist,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelState {
    Hidden,
    Docked { shaded: bool },
    Detached { shaded: bool },
}

impl PanelState {
    fn is_detached_visible(self) -> bool {
        matches!(self, PanelState::Detached { .. })
    }

    fn is_docked_visible(self) -> bool {
        matches!(self, PanelState::Docked { .. })
    }

    fn shaded(self) -> bool {
        match self {
            PanelState::Hidden => false,
            PanelState::Docked { shaded } | PanelState::Detached { shaded } => shaded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PanelPlacement {
    visible: bool,
    detached: bool,
    shaded: bool,
    focused: bool,
    dragging_title: bool,
}

impl PanelPlacement {
    fn from_config(visible: bool, detached: bool, shaded: bool) -> Self {
        Self {
            visible,
            detached,
            shaded,
            focused: false,
            dragging_title: false,
        }
    }

    fn state(self) -> PanelState {
        match (self.visible, self.detached) {
            (false, _) => PanelState::Hidden,
            (true, true) => PanelState::Detached {
                shaded: self.shaded,
            },
            (true, false) => PanelState::Docked {
                shaded: self.shaded,
            },
        }
    }

    fn focused(self) -> bool {
        self.focused || self.dragging_title
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistMenuKind {
    Add,
    Remove,
    Select,
    Misc,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PlaylistMenu {
    #[default]
    Closed,
    Open {
        kind: PlaylistMenuKind,
        hover: Option<usize>,
        pressed: bool,
    },
}

impl PlaylistMenu {
    fn kind(self) -> Option<PlaylistMenuKind> {
        match self {
            Self::Closed => None,
            Self::Open { kind, .. } => Some(kind),
        }
    }

    fn hover(self) -> Option<usize> {
        match self {
            Self::Closed => None,
            Self::Open { hover, .. } => hover,
        }
    }

    fn pressed(self) -> bool {
        match self {
            Self::Closed => false,
            Self::Open { pressed, .. } => pressed,
        }
    }

    fn is_open(self) -> bool {
        matches!(self, Self::Open { .. })
    }

    fn open(&mut self, kind: PlaylistMenuKind) {
        *self = Self::Open {
            kind,
            hover: Some(kind.item_count().saturating_sub(1)),
            pressed: false,
        };
    }

    fn close(&mut self) {
        *self = Self::Closed;
    }

    fn set_hover(&mut self, hover: Option<usize>) -> bool {
        let Self::Open { hover: current, .. } = self else {
            return false;
        };
        let changed = *current != hover;
        *current = hover;
        changed
    }

    fn press_item(&mut self, item: usize) -> bool {
        let Self::Open { hover, pressed, .. } = self else {
            return false;
        };
        *hover = Some(item);
        *pressed = true;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaylistMenuCommand {
    OpenLocationWindow,
    OpenDirectoryDialog,
    OpenFileDialog,
    ShowSortMenu,
    ShowFileInfo,
    OpenOptions,
    ClearList,
    CropToSelection,
    RemoveSelectedOrCurrent,
    InvertSelection,
    SelectNone,
    SelectAll,
    SavePlaylist,
    LoadPlaylist,
}

impl PlaylistMenuCommand {
    fn from_menu_item(menu: PlaylistMenuKind, item: usize) -> Option<Self> {
        match (menu, item) {
            (PlaylistMenuKind::Add, 0) => Some(Self::OpenLocationWindow),
            (PlaylistMenuKind::Add, 1) => Some(Self::OpenDirectoryDialog),
            (PlaylistMenuKind::Add, 2) => Some(Self::OpenFileDialog),
            (PlaylistMenuKind::Misc, 0) => Some(Self::ShowSortMenu),
            (PlaylistMenuKind::Misc, 1) => Some(Self::ShowFileInfo),
            (PlaylistMenuKind::Misc, 2) => Some(Self::OpenOptions),
            (PlaylistMenuKind::Remove, 1) => Some(Self::ClearList),
            (PlaylistMenuKind::Remove, 2) => Some(Self::CropToSelection),
            (PlaylistMenuKind::Remove, 3) => Some(Self::RemoveSelectedOrCurrent),
            (PlaylistMenuKind::Select, 0) => Some(Self::InvertSelection),
            (PlaylistMenuKind::Select, 1) => Some(Self::SelectNone),
            (PlaylistMenuKind::Select, 2) => Some(Self::SelectAll),
            (PlaylistMenuKind::List, 0) => Some(Self::ClearList),
            (PlaylistMenuKind::List, 1) => Some(Self::SavePlaylist),
            (PlaylistMenuKind::List, 2) => Some(Self::LoadPlaylist),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistContextAction {
    RemoveSelected,
    RemoveDead,
    PhysicallyDelete,
    SelectAll,
    SelectNone,
    InvertSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistSortAction {
    ListByTitle,
    ListByFilename,
    ListByPath,
    ListByDate,
    SelectionByTitle,
    SelectionByFilename,
    SelectionByPath,
    SelectionByDate,
    RandomizeList,
    ReverseList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerPresetAction {
    LoadPreset,
    LoadAutoPreset,
    LoadDefault,
    LoadZero,
    LoadFromFile,
    LoadFromWinampFile,
    ImportWinampPresets,
    SavePreset,
    SaveAutoPreset,
    SaveDefault,
    SaveToFile,
    SaveToWinampFile,
    DeletePreset,
    DeleteAutoPreset,
    Configure,
}

impl PlaylistMenuKind {
    fn render_kind(self) -> PlaylistMenuRenderKind {
        match self {
            Self::Add => PlaylistMenuRenderKind::Add,
            Self::Remove => PlaylistMenuRenderKind::Remove,
            Self::Select => PlaylistMenuRenderKind::Select,
            Self::Misc => PlaylistMenuRenderKind::Misc,
            Self::List => PlaylistMenuRenderKind::List,
        }
    }

    fn item_count(self) -> usize {
        self.render_kind().item_count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelAction {
    None,
    Changed,
    OpenDirectoryDialog,
    OpenFileDialog,
    OpenLocationWindow,
    OpenPlaylistLoadDialog,
    OpenPlaylistSaveDialog,
    ShowPlaylistSortMenu,
    ShowFileInfo,
    ShowPlaylistMenu(PlaylistMenuKind),
    ShowEqualizerPresets,
}

fn add_panel_click_controller(
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    main_area: gtk::DrawingArea,
    kind: PanelKind,
    equalizer_presets_menu: Option<gtk::Popover>,
    open_location_window: Option<gtk::ApplicationWindow>,
    playlist_sort_menu: Option<gtk::Popover>,
) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        click.connect_pressed(move |gesture, n_press, x, y| {
            area.grab_focus();
            let (base_x, base_y) =
                panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            if n_press >= 2
                && kind == PanelKind::Equalizer
                && main_state
                    .borrow()
                    .panel_title_drag_region(kind, base_x, base_y)
            {
                main_state.borrow_mut().toggle_equalizer_shaded();
                sync_single_panel_window_from_state(kind, &window, &area, &main_state);
                area.queue_draw();
                return;
            }
            if !main_state
                .borrow()
                .panel_title_drag_region(kind, base_x, base_y)
            {
                if kind == PanelKind::Equalizer
                    && main_state.borrow_mut().equalizer_press(base_x, base_y)
                {
                    area.queue_draw();
                } else if kind == PanelKind::Playlist {
                    if n_press >= 2
                        && main_state
                            .borrow_mut()
                            .activate_playlist_entry_at(base_x, base_y)
                    {
                        area.queue_draw();
                        return;
                    }
                    let ctrl_pressed = gesture
                        .current_event_state()
                        .contains(gtk::gdk::ModifierType::CONTROL_MASK);
                    if main_state.borrow_mut().playlist_press_with_ctrl(
                        base_x,
                        base_y,
                        ctrl_pressed,
                    ) {
                        area.queue_draw();
                        return;
                    }
                    if main_state
                        .borrow_mut()
                        .playlist_scrollbar_press(base_x, base_y)
                    {
                        area.queue_draw();
                        return;
                    }
                    if main_state.borrow().playlist_resize_region(base_x, base_y) {
                        let Some(device) = gesture.current_event_device() else {
                            return;
                        };
                        let Some(surface) = window.surface() else {
                            return;
                        };
                        let Ok(toplevel) = surface.downcast::<gtk::gdk::Toplevel>() else {
                            return;
                        };
                        toplevel.begin_resize(
                            gtk::gdk::SurfaceEdge::SouthEast,
                            Some(&device),
                            gesture.current_button() as i32,
                            x,
                            y,
                            gesture.current_event_time(),
                        );
                    }
                }
                return;
            }

            main_state.borrow_mut().set_panel_dragging(kind, true);
            area.queue_draw();
            let Some(device) = gesture.current_event_device() else {
                return;
            };
            let Some(surface) = window.surface() else {
                return;
            };
            let Ok(toplevel) = surface.downcast::<gtk::gdk::Toplevel>() else {
                return;
            };
            toplevel.begin_move(
                &device,
                gesture.current_button() as i32,
                x,
                y,
                gesture.current_event_time(),
            );
        });
    }
    {
        let area = area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        click.connect_released(move |_gesture, _n_press, x, y| {
            let (x, y) = panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            main_state.borrow_mut().set_panel_dragging(kind, false);
            area.queue_draw();
            let action = if kind == PanelKind::Equalizer {
                let title_action = main_state.borrow_mut().panel_click(kind, x, y);
                if title_action == PanelAction::None {
                    main_state.borrow_mut().equalizer_release(x, y)
                } else {
                    title_action
                }
            } else if main_state.borrow_mut().playlist_scrollbar_release() {
                PanelAction::Changed
            } else if main_state.borrow().playlist_menu_pressed() {
                main_state.borrow_mut().playlist_release(x, y)
            } else if main_state.borrow_mut().playlist_entry_release() {
                PanelAction::Changed
            } else {
                main_state.borrow_mut().panel_click(kind, x, y)
            };
            match action {
                PanelAction::None => {}
                PanelAction::Changed => {
                    sync_single_panel_window_from_state(kind, &window, &area, &main_state);
                    main_area.queue_draw();
                }
                PanelAction::OpenDirectoryDialog => {
                    main_state.borrow_mut().set_directory_dialog_visible(true);
                    show_playlist_add_directory_dialog(
                        &window,
                        Rc::clone(&main_state),
                        area.clone(),
                    );
                }
                PanelAction::OpenFileDialog => {
                    main_state.borrow_mut().set_file_dialog_visible(true);
                    show_playlist_add_file_dialog(&window, Rc::clone(&main_state), area.clone());
                }
                PanelAction::OpenLocationWindow => {
                    main_state.borrow_mut().set_open_location_visible(true);
                    if let Some(open_location_window) = open_location_window.as_ref() {
                        open_location_window.present();
                    }
                }
                PanelAction::OpenPlaylistLoadDialog => {
                    main_state
                        .borrow_mut()
                        .set_playlist_load_dialog_visible(true);
                    show_playlist_load_dialog(&window, Rc::clone(&main_state), area.clone());
                }
                PanelAction::OpenPlaylistSaveDialog => {
                    main_state
                        .borrow_mut()
                        .set_playlist_save_dialog_visible(true);
                    show_playlist_save_dialog(&window, Rc::clone(&main_state));
                }
                PanelAction::ShowFileInfo => {
                    show_file_info_dialog(&window, Rc::clone(&main_state));
                }
                PanelAction::ShowPlaylistSortMenu => {
                    if let Some(popover) = playlist_sort_menu.as_ref() {
                        show_playlist_sort_menu(popover, &area);
                    }
                    area.queue_draw();
                }
                PanelAction::ShowPlaylistMenu(menu) => {
                    let _ = menu;
                    area.queue_draw();
                }
                PanelAction::ShowEqualizerPresets => {
                    if let Some(popover) = equalizer_presets_menu.as_ref() {
                        show_equalizer_presets_menu(popover, &area);
                    }
                    area.queue_draw();
                }
            }
        });
    }
    window.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    let panel_hover_base = Rc::new(Cell::new(None::<(i32, i32)>));
    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        let panel_hover_base = Rc::clone(&panel_hover_base);
        motion.connect_motion(move |_motion, x, y| {
            let (x, y) = panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            panel_hover_base.set(Some((x, y)));
            match kind {
                PanelKind::Equalizer => {
                    if main_state.borrow_mut().equalizer_motion(x, y) {
                        area.queue_draw();
                    }
                }
                PanelKind::Playlist => {
                    let scrolled = main_state.borrow_mut().playlist_scrollbar_motion(x, y);
                    let menu_changed = main_state.borrow_mut().playlist_motion(x, y);
                    if scrolled || menu_changed {
                        area.queue_draw();
                    }
                }
            }
        });
    }
    window.add_controller(motion);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        let panel_hover_base = Rc::clone(&panel_hover_base);
        scroll.connect_scroll(move |scroll, _dx, dy| {
            let hover = panel_hover_base.get().or_else(|| {
                scroll
                    .current_event()
                    .and_then(|event| event.position())
                    .map(|(event_x, event_y)| {
                        panel_event_to_base_coords(
                            kind,
                            &area,
                            &main_state.borrow(),
                            event_x,
                            event_y,
                        )
                    })
            });
            let Some((x, y)) = hover else {
                return gtk::glib::Propagation::Proceed;
            };
            let changed = match kind {
                PanelKind::Equalizer => main_state.borrow_mut().equalizer_scroll(x, y, dy),
                PanelKind::Playlist => main_state.borrow_mut().playlist_scroll(dy),
            };
            if changed {
                area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(scroll);

    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        window.connect_is_active_notify(move |window| {
            main_state
                .borrow_mut()
                .set_panel_focused(kind, window.is_active());
            area.queue_draw();
        });
    }
}

fn show_equalizer_presets_menu(popover: &gtk::Popover, area: &gtk::DrawingArea) {
    let scale_x = area.allocated_width().max(1) as f64 / f64::from(EQUALIZER_WINDOW_WIDTH);
    let scale_y = area.allocated_height().max(1) as f64 / f64::from(EQUALIZER_WINDOW_HEIGHT);
    let rect = gtk::gdk::Rectangle::new(
        (217.0 * scale_x) as i32,
        (30.0 * scale_y) as i32,
        (44.0 * scale_x).max(1.0) as i32,
        1,
    );
    popover.set_pointing_to(Some(&rect));
    popover.popup();
}

fn show_docked_equalizer_presets_menu(
    popover: &gtk::Popover,
    area: &gtk::DrawingArea,
    state: &MainWindowUiState,
) {
    let (base_width, base_height) = state.docked_panel_size();
    let scale_x = area.allocated_width().max(1) as f64 / f64::from(base_width);
    let scale_y = area.allocated_height().max(1) as f64 / f64::from(base_height);
    let y_offset = main_window_height(state.shaded);
    let rect = gtk::gdk::Rectangle::new(
        (217.0 * scale_x) as i32,
        (f64::from(y_offset + 30) * scale_y) as i32,
        (44.0 * scale_x).max(1.0) as i32,
        1,
    );
    popover.set_pointing_to(Some(&rect));
    popover.popup();
}

fn activate_equalizer_preset_action(
    action: EqualizerPresetAction,
    parent: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    match action {
        EqualizerPresetAction::LoadPreset => show_equalizer_preset_list_dialog(
            parent,
            "Load preset",
            false,
            false,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::LoadAutoPreset => show_equalizer_preset_list_dialog(
            parent,
            "Load auto-preset",
            true,
            false,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::LoadDefault => {
            main_state.borrow_mut().load_equalizer_default_preset();
            queue_equalizer_areas(&equalizer_area, &main_area);
        }
        EqualizerPresetAction::LoadZero => {
            main_state.borrow_mut().load_equalizer_zero_preset();
            queue_equalizer_areas(&equalizer_area, &main_area);
        }
        EqualizerPresetAction::LoadFromFile => show_equalizer_file_dialog(
            parent,
            "Load equalizer preset",
            gtk::FileChooserAction::Open,
            "Open",
            move |state, path| state.load_equalizer_preset_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::LoadFromWinampFile => show_equalizer_file_dialog(
            parent,
            "Load WinAMP equalizer preset",
            gtk::FileChooserAction::Open,
            "Open",
            move |state, path| state.load_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::ImportWinampPresets => show_equalizer_file_dialog(
            parent,
            "Import WinAMP equalizer presets",
            gtk::FileChooserAction::Open,
            "Import",
            move |state, path| state.import_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::SavePreset => show_equalizer_save_name_dialog(
            parent,
            "Save preset",
            None,
            false,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::SaveAutoPreset => {
            let default_name = main_state.borrow().current_playlist_basename();
            show_equalizer_save_name_dialog(
                parent,
                "Save auto-preset",
                default_name,
                true,
                main_state,
                equalizer_area,
                main_area,
            );
        }
        EqualizerPresetAction::SaveDefault => {
            if let Err(err) = main_state.borrow_mut().save_equalizer_default_preset() {
                eprintln!("xmms-rs: failed to save default equalizer preset: {err}");
            }
        }
        EqualizerPresetAction::SaveToFile => show_equalizer_file_dialog(
            parent,
            "Save equalizer preset",
            gtk::FileChooserAction::Save,
            "Save",
            move |state, path| state.save_equalizer_preset_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::SaveToWinampFile => show_equalizer_file_dialog(
            parent,
            "Save WinAMP equalizer preset",
            gtk::FileChooserAction::Save,
            "Save",
            move |state, path| state.save_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::DeletePreset => show_equalizer_preset_list_dialog(
            parent,
            "Delete preset",
            false,
            true,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::DeleteAutoPreset => show_equalizer_preset_list_dialog(
            parent,
            "Delete auto-preset",
            true,
            true,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::Configure => {
            show_equalizer_configure_dialog(parent, main_state, equalizer_area, main_area);
        }
    }
}

fn queue_equalizer_areas(equalizer_area: &gtk::DrawingArea, main_area: &gtk::DrawingArea) {
    equalizer_area.queue_draw();
    main_area.queue_draw();
}

fn area_window(parent: &gtk::DrawingArea) -> Option<gtk::Window> {
    parent
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
}

fn show_equalizer_file_dialog(
    parent: &gtk::DrawingArea,
    title: &'static str,
    action: gtk::FileChooserAction,
    accept: &'static str,
    handler: impl Fn(&mut MainWindowUiState, &Path) -> io::Result<()> + 'static,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let parent_window = area_window(parent);
    let dialog = gtk::FileChooserNative::new(
        Some(title),
        parent_window.as_ref(),
        action,
        Some(accept),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|file| file.path()) {
                if let Err(err) = handler(&mut main_state.borrow_mut(), &path) {
                    eprintln!(
                        "xmms-rs: equalizer file action failed for {}: {err}",
                        path.display()
                    );
                }
            }
        }
        queue_equalizer_areas(&equalizer_area, &main_area);
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_equalizer_save_name_dialog(
    parent: &gtk::DrawingArea,
    title: &'static str,
    default_name: Option<String>,
    automatic: bool,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window(title, 320, 90, &[]);
    window.set_modal(true);
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    let entry = gtk::Entry::new();
    entry.set_text(default_name.as_deref().unwrap_or(""));
    layout.append(&entry);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let ok = gtk::Button::with_label("Ok");
    let cancel = gtk::Button::with_label("Cancel");
    {
        let window = window.clone();
        let entry = entry.clone();
        ok.connect_clicked(move |_| {
            let name = entry.text().trim().to_string();
            if !name.is_empty() {
                if let Err(err) = main_state
                    .borrow_mut()
                    .save_named_equalizer_preset(name, automatic)
                {
                    eprintln!("xmms-rs: failed to save equalizer preset: {err}");
                }
            }
            queue_equalizer_areas(&equalizer_area, &main_area);
            window.close();
        });
    }
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    buttons.append(&ok);
    buttons.append(&cancel);
    layout.append(&buttons);
    window.set_child(Some(&layout));
    window.present();
}

fn show_equalizer_preset_list_dialog(
    parent: &gtk::DrawingArea,
    title: &'static str,
    automatic: bool,
    delete_mode: bool,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window(title, 350, 300, &[]);
    window.set_modal(true);
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    let presets = main_state.borrow().sorted_equalizer_presets(automatic);
    if delete_mode {
        let mut checks = Vec::new();
        for preset in presets {
            let check = gtk::CheckButton::with_label(&preset.name);
            layout.append(&check);
            checks.push((preset.name, check));
        }
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let delete = gtk::Button::with_label("Delete");
        let close = gtk::Button::with_label("Close");
        {
            let window = window.clone();
            delete.connect_clicked(move |_| {
                let names: Vec<String> = checks
                    .iter()
                    .filter(|(_, check)| check.is_active())
                    .map(|(name, _)| name.clone())
                    .collect();
                if let Err(err) = main_state
                    .borrow_mut()
                    .delete_named_equalizer_presets(names, automatic)
                {
                    eprintln!("xmms-rs: failed to delete equalizer presets: {err}");
                }
                queue_equalizer_areas(&equalizer_area, &main_area);
                window.close();
            });
        }
        {
            let window = window.clone();
            close.connect_clicked(move |_| window.close());
        }
        buttons.append(&delete);
        buttons.append(&close);
        layout.append(&buttons);
    } else {
        for preset in presets {
            let button = gtk::Button::with_label(&preset.name);
            {
                let window = window.clone();
                let name = preset.name.clone();
                let main_state = Rc::clone(&main_state);
                let equalizer_area = equalizer_area.clone();
                let main_area = main_area.clone();
                button.connect_clicked(move |_| {
                    main_state
                        .borrow_mut()
                        .load_named_equalizer_preset(&name, automatic);
                    queue_equalizer_areas(&equalizer_area, &main_area);
                    window.close();
                });
            }
            layout.append(&button);
        }
        let close = gtk::Button::with_label("Cancel");
        {
            let window = window.clone();
            close.connect_clicked(move |_| window.close());
        }
        layout.append(&close);
    }
    window.set_child(Some(&layout));
    window.present();
}

fn show_equalizer_configure_dialog(
    parent: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window("Configure Equalizer", 360, 140, &[]);
    window.set_modal(true);
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    let default_file = gtk::Entry::new();
    let extension = gtk::Entry::new();
    {
        let state = main_state.borrow();
        default_file.set_text(&state.app_state.config.eqpreset_default_file);
        extension.set_text(&state.app_state.config.eqpreset_extension);
    }
    layout.append(&gtk::Label::new(Some("Directory preset file:")));
    layout.append(&default_file);
    layout.append(&gtk::Label::new(Some("File preset extension:")));
    layout.append(&extension);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let ok = gtk::Button::with_label("Ok");
    let cancel = gtk::Button::with_label("Cancel");
    {
        let window = window.clone();
        ok.connect_clicked(move |_| {
            let mut state = main_state.borrow_mut();
            state.app_state.config.eqpreset_default_file = default_file
                .text()
                .trim()
                .trim_start_matches('.')
                .to_string();
            state.app_state.config.eqpreset_extension =
                extension.text().trim().trim_start_matches('.').to_string();
            queue_equalizer_areas(&equalizer_area, &main_area);
            window.close();
        });
    }
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    buttons.append(&ok);
    buttons.append(&cancel);
    layout.append(&buttons);
    window.set_child(Some(&layout));
    window.present();
}

fn panel_event_to_base_coords(
    kind: PanelKind,
    area: &gtk::DrawingArea,
    state: &MainWindowUiState,
    x: f64,
    y: f64,
) -> (i32, i32) {
    let (base_width, base_height) = match kind {
        PanelKind::Equalizer => (
            EQUALIZER_WINDOW_WIDTH,
            if state.equalizer.panel.shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                EQUALIZER_WINDOW_HEIGHT
            },
        ),
        PanelKind::Playlist => (
            state.playlist_ui.width,
            if state.playlist_ui.panel.shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                state.playlist_ui.height
            },
        ),
    };
    let width = area.allocated_width().max(1) as f64;
    let height = area.allocated_height().max(1) as f64;
    scale_event_coords(width, height, base_width, base_height, x, y)
}

fn handle_panel_action_for_main_window(
    action: PanelAction,
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    panel_windows: &Rc<PanelWindows>,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    playlist_sort_menu: &gtk::Popover,
    equalizer_presets_menu: &gtk::Popover,
) {
    match action {
        PanelAction::None => {}
        PanelAction::Changed => {
            sync_panel_windows(panel_windows, &main_state.borrow());
            resize_main_window(window, area, &main_state.borrow());
        }
        PanelAction::OpenDirectoryDialog => {
            main_state.borrow_mut().set_directory_dialog_visible(true);
            show_playlist_add_directory_dialog(window, Rc::clone(main_state), area.clone());
        }
        PanelAction::OpenFileDialog => {
            main_state.borrow_mut().set_file_dialog_visible(true);
            show_playlist_add_file_dialog(window, Rc::clone(main_state), area.clone());
        }
        PanelAction::OpenLocationWindow => {
            main_state.borrow_mut().set_open_location_visible(true);
            panel_windows.open_location.present();
        }
        PanelAction::OpenPlaylistLoadDialog => {
            main_state
                .borrow_mut()
                .set_playlist_load_dialog_visible(true);
            show_playlist_load_dialog(window, Rc::clone(main_state), area.clone());
        }
        PanelAction::OpenPlaylistSaveDialog => {
            main_state
                .borrow_mut()
                .set_playlist_save_dialog_visible(true);
            show_playlist_save_dialog(window, Rc::clone(main_state));
        }
        PanelAction::ShowFileInfo => {
            show_file_info_dialog(window, Rc::clone(main_state));
        }
        PanelAction::ShowPlaylistSortMenu => {
            show_playlist_sort_menu(playlist_sort_menu, area);
            area.queue_draw();
        }
        PanelAction::ShowPlaylistMenu(_) => {
            area.queue_draw();
        }
        PanelAction::ShowEqualizerPresets => {
            show_docked_equalizer_presets_menu(equalizer_presets_menu, area, &main_state.borrow());
            area.queue_draw();
        }
    }
}

fn sync_single_panel_window_from_state(
    kind: PanelKind,
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    state: &Rc<RefCell<MainWindowUiState>>,
) {
    let (visible, shaded, width, full_height, scale) = {
        let state = state.borrow();
        let (visible, shaded, width, full_height) = panel_window_values(kind, &state);
        (visible, shaded, width, full_height, state.scale_factor())
    };
    sync_single_panel_window_values(
        window,
        area,
        visible,
        shaded,
        width,
        full_height,
        scale,
        kind == PanelKind::Playlist && !shaded,
    );
}

fn panel_window_values(kind: PanelKind, state: &MainWindowUiState) -> (bool, bool, i32, i32) {
    match kind {
        PanelKind::Equalizer => (
            state.panel_state(kind).is_detached_visible(),
            state.panel_state(kind).shaded(),
            EQUALIZER_WINDOW_WIDTH,
            EQUALIZER_WINDOW_HEIGHT,
        ),
        PanelKind::Playlist => (
            state.panel_state(kind).is_detached_visible(),
            state.panel_state(kind).shaded(),
            state.playlist_ui.width,
            state.playlist_ui.height,
        ),
    }
}

fn sync_single_panel_window_values(
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    visible: bool,
    shaded: bool,
    width: i32,
    full_height: i32,
    scale: f64,
    resizable: bool,
) {
    if !visible {
        window.hide();
        return;
    }
    let height = if shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        full_height
    };
    area.set_content_width(scale_dim(width, scale));
    area.set_content_height(scale_dim(height, scale));
    window.set_resizable(resizable);
    window.set_default_size(scale_dim(width, scale), scale_dim(height, scale));
    area.queue_resize();
    window.queue_resize();
    present_if_hidden(window);
    area.queue_draw();
}

fn present_if_hidden(window: &gtk::ApplicationWindow) {
    if !window.is_visible() {
        window.present();
    }
}

fn present_visible_panel_windows(windows: &PanelWindows, state: &MainWindowUiState) {
    let visibility = state.panel_visibility();
    if visibility.equalizer {
        windows.equalizer.present();
    }
    if visibility.playlist {
        windows.playlist.present();
    }
}

fn sync_panel_windows(windows: &PanelWindows, state: &MainWindowUiState) {
    let visibility = state.panel_visibility();
    let scale = state.scale_factor();
    if visibility.equalizer {
        let height = if state.equalizer.panel.shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        };
        windows
            .equalizer_area
            .set_content_width(scale_dim(EQUALIZER_WINDOW_WIDTH, scale));
        windows
            .equalizer_area
            .set_content_height(scale_dim(height, scale));
        windows.equalizer.set_resizable(false);
        windows.equalizer.set_default_size(
            scale_dim(EQUALIZER_WINDOW_WIDTH, scale),
            scale_dim(height, scale),
        );
        present_if_hidden(&windows.equalizer);
        windows.equalizer_area.queue_draw();
    } else {
        windows.equalizer.hide();
    }

    if visibility.playlist {
        let height = if state.playlist_ui.panel.shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            state.playlist_ui.height
        };
        windows
            .playlist_area
            .set_content_width(scale_dim(state.playlist_ui.width, scale));
        windows
            .playlist_area
            .set_content_height(scale_dim(height, scale));
        windows
            .playlist
            .set_resizable(!state.playlist_ui.panel.shaded);
        windows.playlist.set_default_size(
            scale_dim(state.playlist_ui.width, scale),
            scale_dim(height, scale),
        );
        windows.playlist_area.queue_resize();
        windows.playlist.queue_resize();
        present_if_hidden(&windows.playlist);
        windows.playlist_area.queue_draw();
    } else {
        windows.playlist.hide();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum PlaylistSearch {
    #[default]
    Inactive,
    Active {
        query: String,
    },
}

impl PlaylistSearch {
    fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    fn query(&self) -> &str {
        match self {
            Self::Inactive => "",
            Self::Active { query } => query,
        }
    }

    fn active_query(&self) -> Option<&str> {
        match self {
            Self::Inactive => None,
            Self::Active { query } => Some(query),
        }
    }

    fn start(&mut self) {
        *self = Self::Active {
            query: String::new(),
        };
    }

    fn stop(&mut self) {
        *self = Self::Inactive;
    }

    fn push_char(&mut self, ch: char) -> bool {
        let Self::Active { query } = self else {
            return false;
        };
        if ch.is_control() {
            return false;
        }
        query.push(ch);
        true
    }

    fn pop_char(&mut self) -> bool {
        let Self::Active { query } = self else {
            return false;
        };
        query.pop();
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MainPointer {
    #[default]
    Idle,
    PressedButton {
        control: MainControl,
        inside: bool,
    },
    DraggingSlider {
        slider: MainSlider,
        offset: i32,
    },
}

impl MainPointer {
    fn pressed_control(self) -> Option<MainControl> {
        match self {
            Self::PressedButton {
                control,
                inside: true,
            } => Some(control),
            _ => None,
        }
    }

    fn pressed_slider(self) -> Option<MainSlider> {
        match self {
            Self::DraggingSlider { slider, .. } => Some(slider),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EqualizerPointer {
    #[default]
    Idle,
    PressedControl {
        control: EqualizerControl,
        inside: bool,
    },
    DraggingSlider {
        slider: EqualizerSlider,
        offset: i32,
    },
}

impl EqualizerPointer {
    fn pressed_control(self) -> Option<EqualizerControl> {
        match self {
            Self::PressedControl {
                control,
                inside: true,
            } => Some(control),
            _ => None,
        }
    }

    fn dragging_slider(self) -> Option<EqualizerSlider> {
        match self {
            Self::DraggingSlider { slider, .. } => Some(slider),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PlaylistPointer {
    #[default]
    Idle,
    DraggingEntry {
        index: usize,
        moved: bool,
    },
    DraggingScrollbar {
        offset: i32,
    },
    Resizing {
        offset_y: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainControl {
    Push(MainPushButton),
    Toggle(MainToggleButton),
    Slider(MainSlider),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackControlEvent {
    Play,
    Pause,
    PauseToggle,
    Stop,
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackTransitionState {
    Idle,
    StoppedAt(i64),
    PendingBackendSeek(i64),
    WaitingBetweenSongs {
        remaining_ms: i64,
    },
    FadingOut {
        remaining_ms: i64,
        start_volume: i32,
    },
}

impl PlaybackTransitionState {
    fn stopped_at_or_idle(position_ms: i64) -> Self {
        if position_ms > 0 {
            Self::StoppedAt(position_ms)
        } else {
            Self::Idle
        }
    }

    fn start_playback() -> Self {
        Self::Idle
    }

    fn stop_playback() -> Self {
        Self::Idle
    }

    fn request_backend_seek(position_ms: i64) -> Self {
        Self::PendingBackendSeek(position_ms)
    }

    fn start_fadeout(start_volume: i32) -> Self {
        Self::FadingOut {
            remaining_ms: STOP_FADE_DURATION_MS,
            start_volume,
        }
    }

    fn tick_fadeout(self, elapsed_ms: u32) -> Option<(Self, i32)> {
        let (remaining_ms, start_volume) = self.fadeout()?;
        let remaining_ms = (remaining_ms - i64::from(elapsed_ms)).max(0);
        let volume =
            ((i64::from(start_volume) * remaining_ms) / STOP_FADE_DURATION_MS).clamp(0, 100) as i32;
        Some((
            Self::FadingOut {
                remaining_ms,
                start_volume,
            },
            volume,
        ))
    }

    fn wait_between_songs(remaining_ms: i64) -> Self {
        Self::WaitingBetweenSongs { remaining_ms }
    }

    fn tick_eof_pause(self, elapsed_ms: u32) -> Option<(Self, bool)> {
        let remaining = self.eof_pause_remaining_ms()? - i64::from(elapsed_ms);
        if remaining > 0 {
            Some((
                Self::WaitingBetweenSongs {
                    remaining_ms: remaining,
                },
                false,
            ))
        } else {
            Some((Self::Idle, true))
        }
    }

    fn eof_pause_remaining_ms(self) -> Option<i64> {
        match self {
            PlaybackTransitionState::WaitingBetweenSongs { remaining_ms } => Some(remaining_ms),
            _ => None,
        }
    }

    fn pending_backend_seek_ms(self) -> Option<i64> {
        match self {
            PlaybackTransitionState::PendingBackendSeek(position_ms) => Some(position_ms),
            _ => None,
        }
    }

    fn fadeout(self) -> Option<(i64, i32)> {
        match self {
            PlaybackTransitionState::FadingOut {
                remaining_ms,
                start_volume,
            } => Some((remaining_ms, start_volume)),
            _ => None,
        }
    }

    fn play_start_position_ms(self, fallback_ms: i64) -> i64 {
        match self {
            PlaybackTransitionState::StoppedAt(position_ms) => position_ms,
            PlaybackTransitionState::WaitingBetweenSongs { .. } => 0,
            PlaybackTransitionState::Idle
            | PlaybackTransitionState::PendingBackendSeek(_)
            | PlaybackTransitionState::FadingOut { .. } => fallback_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiAction {
    None,
    Quit,
    Minimize,
    Resize,
    ShowMenu,
    OpenFileDialog,
}

struct EqualizerUiState {
    panel: PanelPlacement,
    active: bool,
    automatic: bool,
    pointer: EqualizerPointer,
    keyboard_slider: Option<EqualizerSlider>,
    preamp_position: i32,
    band_positions: [i32; 10],
    preset_dir: PathBuf,
    presets: Vec<EqualizerPreset>,
    auto_presets: Vec<EqualizerPreset>,
}

impl EqualizerUiState {
    fn from_config(config: &Config) -> Self {
        Self {
            panel: PanelPlacement::from_config(
                config.equalizer_visible,
                config.equalizer_detached,
                config.equalizer_shaded,
            ),
            active: true,
            automatic: false,
            pointer: EqualizerPointer::default(),
            keyboard_slider: None,
            preamp_position: 50,
            band_positions: [50; 10],
            preset_dir: default_config_dir().join("xmms-renascene"),
            presets: Vec::new(),
            auto_presets: Vec::new(),
        }
    }
}

struct PlaylistUiState {
    panel: PanelPlacement,
    width: i32,
    height: i32,
    menu: PlaylistMenu,
    scroll_offset: usize,
    pointer: PlaylistPointer,
    last_click: Option<(usize, Instant)>,
    pending_double_click: Option<usize>,
    search: PlaylistSearch,
}

impl PlaylistUiState {
    fn from_config(config: &Config) -> Self {
        Self {
            panel: PanelPlacement::from_config(
                config.playlist_visible,
                config.playlist_detached,
                config.playlist_shaded,
            ),
            width: PLAYLIST_DEFAULT_WIDTH,
            height: PLAYLIST_DEFAULT_HEIGHT,
            menu: PlaylistMenu::default(),
            scroll_offset: 0,
            pointer: PlaylistPointer::default(),
            last_click: None,
            pending_double_click: None,
            search: PlaylistSearch::default(),
        }
    }
}

#[derive(Default)]
struct DialogVisibility {
    playlist_load: bool,
    playlist_save: bool,
    file_info: bool,
    preferences: bool,
    open_location: bool,
    jump_time: bool,
    skin_browser: bool,
    skin_editor: bool,
    output_device_picker: bool,
    file: bool,
    directory: bool,
}

#[derive(Default)]
struct SkinBrowserState {
    entries: Vec<SkinEntry>,
    selected_index: usize,
    reload_count: u32,
}

pub(crate) struct MainWindowUiState {
    app_state: AppState,
    playback_backend: Option<Rc<RefCell<GStreamerBackend>>>,
    duration_index_sender: Sender<DurationIndexResult>,
    duration_index_receiver: Receiver<DurationIndexResult>,
    playback_requests: Vec<String>,
    shaded: bool,
    menu_visible: bool,
    docked_focus: KeyboardFocus,
    equalizer: EqualizerUiState,
    playlist_ui: PlaylistUiState,
    dialogs: DialogVisibility,
    last_playlist_file_info: Option<String>,
    active_skin: DefaultSkin,
    playlist_options_opened: bool,
    playlist_queue: Vec<usize>,
    queue_manager_opened: bool,
    preferences_page: PreferencesPage,
    preferences_saved: bool,
    skin_browser: SkinBrowserState,
    skin_editor: SkinEditorState,
    output_device_groups: OutputDeviceGroups,
    output_switch_count: u32,
    mpris_events: Vec<MprisEvent>,
    mpris_quit_requested: bool,
    playback_transition: PlaybackTransitionState,
    main_keyboard_slider: Option<MainSlider>,
    last_open_location: Option<String>,
    last_jump_time_ms: Option<i64>,
    position_position: i32,
    playback_position_ms: i64,
    visualization: Visualization,
    visualization_tick_counter: i32,
    main_pointer: MainPointer,
}

impl fmt::Debug for MainWindowUiState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MainWindowUiState")
            .field("app_state", &self.app_state)
            .field("shaded", &self.shaded)
            .field("playlist_shaded", &self.playlist_ui.panel.shaded)
            .field("preferences_visible", &self.dialogs.preferences)
            .field("preferences_page", &self.preferences_page)
            .field("player_state", &self.app_state.player.state())
            .finish_non_exhaustive()
    }
}

impl Default for MainWindowUiState {
    fn default() -> Self {
        Self::from_app_state(AppState::default())
    }
}

impl MainWindowUiState {
    pub(crate) fn from_app_state(app_state: AppState) -> Self {
        let (duration_index_sender, duration_index_receiver) = mpsc::channel();
        let main_shaded = app_state.config.main_shaded;
        let equalizer = EqualizerUiState::from_config(&app_state.config);
        let playlist_ui = PlaylistUiState::from_config(&app_state.config);
        let active_skin = load_skin_from_config(&app_state.config).unwrap_or_else(|err| {
            eprintln!("xmms-rs: failed to load configured skin: {err}");
            DefaultSkin::load_bundled().expect("bundled default skin should load")
        });
        let mut state = Self {
            app_state,
            playback_backend: None,
            duration_index_sender,
            duration_index_receiver,
            playback_requests: Vec::new(),
            shaded: main_shaded,
            menu_visible: false,
            docked_focus: KeyboardFocus::default(),
            equalizer,
            playlist_ui,
            dialogs: DialogVisibility::default(),
            last_playlist_file_info: None,
            active_skin,
            playlist_options_opened: false,
            playlist_queue: Vec::new(),
            queue_manager_opened: false,
            preferences_page: PreferencesPage::Options,
            preferences_saved: false,
            skin_browser: SkinBrowserState::default(),
            skin_editor: SkinEditorState::default(),
            output_device_groups: OutputDeviceGroups::default(),
            output_switch_count: 0,
            mpris_events: Vec::new(),
            mpris_quit_requested: false,
            playback_transition: PlaybackTransitionState::Idle,
            main_keyboard_slider: None,
            last_open_location: None,
            last_jump_time_ms: None,
            position_position: 0,
            playback_position_ms: 0,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
            main_pointer: MainPointer::default(),
        };
        state.apply_config_to_ui_state();
        state
    }

    pub(crate) fn app_state_mut(&mut self) -> &mut AppState {
        &mut self.app_state
    }

    pub(crate) fn active_skin(&self) -> &DefaultSkin {
        &self.active_skin
    }

    pub(crate) fn active_skin_mut(&mut self) -> &mut DefaultSkin {
        &mut self.active_skin
    }

    pub(crate) fn skin_editor(&self) -> &SkinEditorState {
        &self.skin_editor
    }

    pub(crate) fn skin_editor_mut(&mut self) -> &mut SkinEditorState {
        &mut self.skin_editor
    }

    fn load_configured_skin(&mut self) -> io::Result<()> {
        self.active_skin = load_skin_from_config(&self.app_state.config)?;
        Ok(())
    }

    fn set_equalizer_preset_dir(&mut self, dir: PathBuf) {
        self.equalizer.preset_dir = dir;
        if let Err(err) = self.load_equalizer_preset_stores() {
            eprintln!("xmms-rs: failed to load equalizer presets: {err}");
        }
    }

    fn load_equalizer_preset_stores(&mut self) -> io::Result<()> {
        self.equalizer.presets =
            load_preset_store(&preset_store_path(&self.equalizer.preset_dir, "eq.preset"))?;
        if self.equalizer.presets.is_empty() {
            self.equalizer.presets = default_equalizer_presets();
        }
        self.equalizer.auto_presets = load_preset_store(&preset_store_path(
            &self.equalizer.preset_dir,
            "eq.auto_preset",
        ))?;
        Ok(())
    }

    fn save_equalizer_presets(&self) -> io::Result<()> {
        save_preset_store(
            &preset_store_path(&self.equalizer.preset_dir, "eq.preset"),
            &self.equalizer.presets,
        )
    }

    fn save_equalizer_auto_presets(&self) -> io::Result<()> {
        save_preset_store(
            &preset_store_path(&self.equalizer.preset_dir, "eq.auto_preset"),
            &self.equalizer.auto_presets,
        )
    }

    fn current_equalizer_preset(&self, name: impl Into<String>) -> EqualizerPreset {
        EqualizerPreset::from_positions(
            name,
            self.equalizer.preamp_position,
            self.equalizer.band_positions,
        )
    }

    fn apply_equalizer_preset_values(&mut self, preset: &EqualizerPreset) {
        self.equalizer.preamp_position = preset.preamp_position();
        self.equalizer.band_positions = preset.band_positions();
        self.sync_equalizer_to_backend();
    }

    fn load_named_equalizer_preset(&mut self, name: &str, automatic: bool) -> bool {
        let preset = if automatic {
            find_preset(&self.equalizer.auto_presets, name)
        } else {
            find_preset(&self.equalizer.presets, name)
        }
        .cloned();
        if let Some(preset) = preset {
            self.apply_equalizer_preset_values(&preset);
            true
        } else {
            false
        }
    }

    fn save_named_equalizer_preset(&mut self, name: String, automatic: bool) -> io::Result<()> {
        let preset = self.current_equalizer_preset(name);
        if automatic {
            upsert_preset(&mut self.equalizer.auto_presets, preset);
            self.save_equalizer_auto_presets()
        } else {
            upsert_preset(&mut self.equalizer.presets, preset);
            self.save_equalizer_presets()
        }
    }

    fn delete_named_equalizer_presets(
        &mut self,
        names: Vec<String>,
        automatic: bool,
    ) -> io::Result<()> {
        if automatic {
            remove_presets(&mut self.equalizer.auto_presets, &names);
            self.save_equalizer_auto_presets()
        } else {
            remove_presets(&mut self.equalizer.presets, &names);
            self.save_equalizer_presets()
        }
    }

    fn load_equalizer_zero_preset(&mut self) {
        self.apply_equalizer_preset_values(&EqualizerPreset::zero("Zero"));
    }

    fn load_equalizer_default_preset(&mut self) {
        self.load_named_equalizer_preset("Default", false);
    }

    fn save_equalizer_default_preset(&mut self) -> io::Result<()> {
        self.save_named_equalizer_preset("Default".to_string(), false)
    }

    fn load_equalizer_preset_file(&mut self, path: &Path) -> io::Result<()> {
        if let Some(preset) = load_xmms_preset_file(path)? {
            self.apply_equalizer_preset_values(&preset);
        }
        Ok(())
    }

    fn save_equalizer_preset_file(&self, path: &Path) -> io::Result<()> {
        save_xmms_preset_file(path, &self.current_equalizer_preset("File"))
    }

    fn load_equalizer_winamp_file(&mut self, path: &Path) -> io::Result<()> {
        if let Some(preset) = load_winamp_eqf_first(path)? {
            self.apply_equalizer_preset_values(&preset);
        }
        Ok(())
    }

    fn import_equalizer_winamp_file(&mut self, path: &Path) -> io::Result<usize> {
        let imported = import_winamp_eqf(path)?;
        let count = imported.len();
        for preset in imported {
            upsert_preset(&mut self.equalizer.presets, preset);
        }
        self.save_equalizer_presets()?;
        Ok(count)
    }

    fn save_equalizer_winamp_file(&self, path: &Path) -> io::Result<()> {
        save_winamp_eqf(path, &self.current_equalizer_preset("Entry1"))
    }

    fn sorted_equalizer_presets(&self, automatic: bool) -> Vec<EqualizerPreset> {
        let mut presets = if automatic {
            self.equalizer.auto_presets.clone()
        } else {
            self.equalizer.presets.clone()
        };
        sort_presets(&mut presets);
        presets
    }

    pub(crate) fn scale_factor(&self) -> f64 {
        self.app_state.config.scale_factor
    }

    fn save_runtime_snapshot(
        &mut self,
        config_path: &Path,
        playlist_path: &Path,
    ) -> io::Result<()> {
        self.sync_config_from_ui_state();
        save_fallback_state(&mut self.app_state, config_path, playlist_path)
    }

    pub(crate) fn set_playback_backend(&mut self, backend: Rc<RefCell<GStreamerBackend>>) {
        {
            let player = &self.app_state.player;
            let backend = backend.borrow();
            backend.set_volume_percent(player.volume());
            backend.set_balance_percent(player.balance());
            backend.set_equalizer_from_positions(
                self.equalizer.active,
                self.equalizer.preamp_position,
                self.equalizer.band_positions,
            );
        }
        self.playback_backend = Some(backend);
    }

    fn render_state(&self) -> MainWindowRenderState {
        MainWindowRenderState {
            focused: self.main_focused(),
            title: self
                .equalizer_drag_info_text()
                .unwrap_or_else(|| self.formatted_current_title()),
            shaded: self.shaded,
            volume_position: volume_to_position(self.app_state.player.volume()),
            balance_position: balance_to_position(self.app_state.player.balance()),
            position_position: self.position_slider_position(),
            shaded_position_position: self.shaded_position_slider_position(),
            shaded_position_visible: self.shaded_position_slider_visible(),
            time_digits: self.time_digits(),
            shaded_time_min: self.shaded_time_min_text(),
            shaded_time_sec: self.shaded_time_sec_text(),
            bitrate_text: self.bitrate_text(),
            frequency_text: self.frequency_text(),
            shuffle_selected: self.app_state.playlist.shuffle(),
            repeat_selected: self.app_state.playlist.repeat(),
            equalizer_selected: self.equalizer.panel.visible,
            playlist_selected: self.playlist_ui.panel.visible,
            pressed_push: self.pressed_push(),
            pressed_toggle: self.pressed_toggle(),
            pressed_slider: self.pressed_slider(),
            play_status: match self.app_state.player.state() {
                PlayerState::Stopped => PlayStatusValue::Stopped,
                PlayerState::Paused => PlayStatusValue::Paused,
                PlayerState::Playing => PlayStatusValue::Playing,
            },
            channels: self.app_state.player.channels(),
            visualization: self.make_visualization_render_state(),
        }
    }

    fn playlist_rows_render_state(&self) -> PlaylistRowsRenderState {
        let current = self.app_state.playlist.position();
        let entries = self
            .app_state
            .playlist
            .entries()
            .iter()
            .enumerate()
            .map(|(index, entry)| PlaylistRowRenderEntry {
                title: self.formatted_playlist_entry_title(entry),
                length_ms: entry.length_ms,
                selected: entry.selected,
                current: current == Some(index),
            })
            .collect();

        PlaylistRowsRenderState {
            entries,
            scroll_offset: self.playlist_ui.scroll_offset,
            scrollbar_dragging: matches!(
                self.playlist_ui.pointer,
                PlaylistPointer::DraggingScrollbar { .. }
            ),
            search_query: self.playlist_ui.search.active_query().map(str::to_owned),
            show_numbers: self.app_state.config.show_numbers_in_pl,
            font_family: self.app_state.config.playlist_font.clone(),
            width: self.playlist_ui.width,
            height: self.playlist_ui.height,
        }
    }

    fn bitrate_text(&self) -> String {
        let bitrate = self.app_state.player.bitrate();
        if bitrate <= 0 {
            return "   ".to_string();
        }
        if bitrate < 1000 {
            format!("{bitrate:>3}")
        } else {
            format!("{:>2}H", bitrate / 100)
        }
    }

    fn frequency_text(&self) -> String {
        let frequency = self.app_state.player.frequency();
        if frequency <= 0 {
            return "  ".to_string();
        }
        let khz = if frequency >= 1000 {
            (frequency + 500) / 1000
        } else {
            frequency
        };
        format!("{khz:>2}")
    }

    pub(crate) fn formatted_current_title(&self) -> String {
        let Some(position) = self.app_state.playlist.position() else {
            return "XMMS Renascene".to_string();
        };
        let Some(entry) = self.app_state.playlist.entries().get(position) else {
            return "XMMS Renascene".to_string();
        };
        self.formatted_playlist_entry_title(entry)
    }

    fn equalizer_drag_info_text(&self) -> Option<String> {
        let slider = self.equalizer.pointer.dragging_slider()?;
        let (label, position) = match slider {
            EqualizerSlider::Preamp => ("PREAMP", self.equalizer.preamp_position),
            EqualizerSlider::Band(0) => ("60HZ", self.equalizer.band_positions[0]),
            EqualizerSlider::Band(1) => ("170HZ", self.equalizer.band_positions[1]),
            EqualizerSlider::Band(2) => ("310HZ", self.equalizer.band_positions[2]),
            EqualizerSlider::Band(3) => ("600HZ", self.equalizer.band_positions[3]),
            EqualizerSlider::Band(4) => ("1KHZ", self.equalizer.band_positions[4]),
            EqualizerSlider::Band(5) => ("3KHZ", self.equalizer.band_positions[5]),
            EqualizerSlider::Band(6) => ("6KHZ", self.equalizer.band_positions[6]),
            EqualizerSlider::Band(7) => ("12KHZ", self.equalizer.band_positions[7]),
            EqualizerSlider::Band(8) => ("14KHZ", self.equalizer.band_positions[8]),
            EqualizerSlider::Band(9) => ("16KHZ", self.equalizer.band_positions[9]),
            EqualizerSlider::Band(_)
            | EqualizerSlider::ShadedVolume
            | EqualizerSlider::ShadedBalance => return None,
        };
        Some(format!(
            "EQ: {label}: {:+.1} DB",
            equalizer_position_to_db(position)
        ))
    }

    fn formatted_playlist_entry_title(&self, entry: &crate::playlist::PlaylistEntry) -> String {
        format_title_for_preferences(
            &self.app_state.config.title_format,
            &entry.filename,
            &entry.title,
            &self.app_state.config,
        )
    }

    pub(crate) fn shaded_playlist_info(&self) -> String {
        let Some(position) = self.app_state.playlist.position() else {
            return String::new();
        };
        let Some(entry) = self.app_state.playlist.entries().get(position) else {
            return String::new();
        };

        let title = self.formatted_playlist_entry_title(entry);
        let prefix = if self.app_state.config.show_numbers_in_pl {
            format!("{}. ", position + 1)
        } else {
            String::new()
        };
        let suffix = if entry.length_ms >= 0 {
            format!(" {}", format_duration(entry.length_ms))
        } else {
            String::new()
        };
        let max_len = ((self.playlist_ui.width - 35) / 5)
            .saturating_sub(prefix.len() as i32)
            .saturating_sub(suffix.len() as i32)
            .max(0) as usize;
        let title = ellipsize_chars(&title, max_len);
        format!("{prefix}{title:<max_len$}{suffix}")
    }

    pub(crate) fn playlist_footer_info(&self) -> String {
        let mut selected_ms = 0_i64;
        let mut total_ms = 0_i64;
        let mut selected_more = false;
        let mut total_more = false;
        let selected_index = self.selected_playlist_index();

        for (index, entry) in self.app_state.playlist.entries().iter().enumerate() {
            if entry.length_ms >= 0 {
                total_ms += entry.length_ms;
            } else {
                total_more = true;
            }

            if entry.selected || selected_index == Some(index) {
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

    fn playlist_footer_time_parts(&self) -> (String, String) {
        if self.app_state.player.state() == PlayerState::Stopped {
            return ("   ".to_string(), "  ".to_string());
        }
        let display_ms = self.display_time_ms();
        let mut seconds = (display_ms / 1000).max(0);
        if seconds > i64::from(99 * 60) {
            seconds /= 60;
        }
        let prefix = if self.app_state.config.timer_mode == TimerMode::Remaining
            && self
                .current_duration_ms()
                .is_some_and(|duration| duration > 0)
        {
            '-'
        } else {
            ' '
        };
        (
            format!("{prefix}{:02}", seconds / 60),
            format!("{:02}", seconds % 60),
        )
    }

    fn playlist_footer_time_min_text(&self) -> String {
        self.playlist_footer_time_parts().0
    }

    fn playlist_footer_time_sec_text(&self) -> String {
        self.playlist_footer_time_parts().1
    }

    fn current_duration_ms(&self) -> Option<i64> {
        self.app_state.player.duration_ms().or_else(|| {
            self.app_state
                .playlist
                .position()
                .and_then(|position| self.playlist_entry_length_ms(position))
                .filter(|duration| *duration > 0)
        })
    }

    fn ensure_current_playlist_position_for_seek(&mut self) {
        if self.app_state.playlist.position().is_none() && !self.app_state.playlist.is_empty() {
            self.app_state.playlist.set_position(0);
        }
    }

    fn position_slider_position(&self) -> i32 {
        let Some(duration_ms) = self.current_duration_ms().filter(|duration| *duration > 0) else {
            return 0;
        };
        let position_slider = main_slider_layout(MainSlider::Position, false);
        ((self.playback_position_ms.clamp(0, duration_ms) * i64::from(position_slider.max))
            / duration_ms) as i32
    }

    fn shaded_position_slider_visible(&self) -> bool {
        self.app_state.player.state() != PlayerState::Stopped
            && self
                .current_duration_ms()
                .is_some_and(|duration| duration > 0)
    }

    fn shaded_position_slider_position(&self) -> i32 {
        let Some(duration_ms) = self.current_duration_ms().filter(|duration| *duration > 0) else {
            return 1;
        };
        (((self.playback_position_ms.clamp(0, duration_ms) * 12) / duration_ms) as i32 + 1)
            .clamp(1, 13)
    }

    fn display_time_ms(&self) -> i64 {
        if let Some(remaining) = self.playback_transition.eof_pause_remaining_ms() {
            return remaining.max(0);
        }
        let elapsed = self.playback_position_ms.max(0);
        if self.app_state.config.timer_mode == TimerMode::Remaining {
            if let Some(duration) = self.current_duration_ms().filter(|duration| *duration > 0) {
                return (duration - elapsed).max(0);
            }
        }
        elapsed
    }

    fn time_digits(&self) -> [i32; 5] {
        if self.app_state.player.state() == PlayerState::Stopped
            && self.playback_transition.eof_pause_remaining_ms().is_none()
        {
            return [NumberDisplay::BLANK; 5];
        }
        let display_ms = self.display_time_ms();
        let mut seconds = (display_ms / 1000).max(0);
        if seconds > i64::from(99 * 60) {
            seconds /= 60;
        }
        let minutes = seconds / 60;
        [
            if self.app_state.config.timer_mode == TimerMode::Remaining
                && self
                    .current_duration_ms()
                    .is_some_and(|duration| duration > 0)
            {
                NumberDisplay::DASH
            } else {
                NumberDisplay::BLANK
            },
            ((minutes / 10) % 10) as i32,
            (minutes % 10) as i32,
            ((seconds % 60) / 10) as i32,
            (seconds % 10) as i32,
        ]
    }

    fn shaded_time_parts(&self) -> (String, String) {
        if self.app_state.player.state() == PlayerState::Stopped {
            return ("   ".to_string(), "  ".to_string());
        }
        let display_ms = self.display_time_ms();
        let mut seconds = (display_ms / 1000).max(0);
        if seconds > i64::from(99 * 60) {
            seconds /= 60;
        }
        let prefix = if self.app_state.config.timer_mode == TimerMode::Remaining
            && self
                .current_duration_ms()
                .is_some_and(|duration| duration > 0)
        {
            '-'
        } else {
            ' '
        };
        (
            format!("{prefix}{:02}", seconds / 60),
            format!("{:02}", seconds % 60),
        )
    }

    fn shaded_time_min_text(&self) -> String {
        self.shaded_time_parts().0
    }

    fn shaded_time_sec_text(&self) -> String {
        self.shaded_time_parts().1
    }

    fn make_visualization_render_state(&self) -> VisualizationRenderState {
        VisualizationRenderState {
            mode: self.visualization.mode(),
            analyzer_style: self.visualization.analyzer_style(),
            analyzer_mode: self.visualization.analyzer_mode(),
            scope_mode: self.visualization.scope_mode(),
            peaks_enabled: self.visualization.peaks_enabled(),
            vu_mode: self.app_state.config.vis_vu_mode,
            data: *self.visualization.data(),
            peak: *self.visualization.peak(),
            milkdrop_energy: self.visualization.milkdrop_energy(),
            milkdrop_phase: self.visualization.milkdrop_phase(),
        }
    }

    fn main_focused(&self) -> bool {
        self.selected_docked_panel().is_none()
    }

    fn equalizer_focused(&self) -> bool {
        if self.equalizer.panel.detached {
            self.equalizer.panel.focused()
        } else {
            self.docked_focus == KeyboardFocus::Equalizer || self.equalizer.panel.dragging_title
        }
    }

    fn playlist_focused(&self) -> bool {
        if self.playlist_ui.panel.detached {
            self.playlist_ui.panel.focused()
        } else {
            self.docked_focus == KeyboardFocus::Playlist || self.playlist_ui.panel.dragging_title
        }
    }

    fn select_focus_target(&mut self, target: KeyboardFocus) {
        self.docked_focus = target;
        self.equalizer.panel.focused = target == KeyboardFocus::Equalizer;
        self.playlist_ui.panel.focused = target == KeyboardFocus::Playlist;
    }

    pub(crate) fn select_docked_main(&mut self) {
        self.select_focus_target(KeyboardFocus::Main);
    }

    pub(crate) fn select_docked_panel(&mut self, kind: PanelKind) {
        if self.panel_state(kind).is_docked_visible() {
            self.select_focus_target(kind.into());
        }
    }

    pub(crate) fn cycle_visible_focus(&mut self) {
        let mut targets = vec![KeyboardFocus::Main];
        if self.panel_state(PanelKind::Equalizer) != PanelState::Hidden {
            targets.push(KeyboardFocus::Equalizer);
        }
        if self.panel_state(PanelKind::Playlist) != PanelState::Hidden {
            targets.push(KeyboardFocus::Playlist);
        }
        let current = if self.equalizer_focused() {
            KeyboardFocus::Equalizer
        } else if self.playlist_focused() {
            KeyboardFocus::Playlist
        } else {
            KeyboardFocus::Main
        };
        let position = targets
            .iter()
            .position(|target| *target == current)
            .unwrap_or(0);
        let next = targets[(position + 1) % targets.len()];
        self.select_focus_target(next);
    }

    fn current_keyboard_focus(&self) -> KeyboardFocus {
        match self.selected_docked_panel() {
            Some(PanelKind::Playlist) => KeyboardFocus::Playlist,
            Some(PanelKind::Equalizer) => KeyboardFocus::Equalizer,
            None => KeyboardFocus::Main,
        }
    }

    fn arrow_key_command(&self, focus: KeyboardFocus, arrow: ArrowKey) -> KeyCommand {
        match (focus, arrow) {
            (KeyboardFocus::Main, ArrowKey::Up) if self.shaded => KeyCommand::NextTrack,
            (KeyboardFocus::Main, ArrowKey::Down) if self.shaded => KeyCommand::PreviousTrack,
            (KeyboardFocus::Main, ArrowKey::Up) => KeyCommand::Volume(4),
            (KeyboardFocus::Main, ArrowKey::Down) => KeyCommand::Volume(-4),
            (KeyboardFocus::Main, ArrowKey::Left)
                if self.main_keyboard_slider == Some(MainSlider::Balance) =>
            {
                KeyCommand::Balance(-4)
            }
            (KeyboardFocus::Main, ArrowKey::Right)
                if self.main_keyboard_slider == Some(MainSlider::Balance) =>
            {
                KeyCommand::Balance(4)
            }
            (KeyboardFocus::Main, ArrowKey::Left) => KeyCommand::Seek(-4),
            (KeyboardFocus::Main, ArrowKey::Right) => KeyCommand::Seek(4),
            (KeyboardFocus::Playlist, ArrowKey::Up) => KeyCommand::PlaylistMove(-1),
            (KeyboardFocus::Playlist, ArrowKey::Down) => KeyCommand::PlaylistMove(1),
            (KeyboardFocus::Playlist, ArrowKey::Left) => KeyCommand::Seek(-4),
            (KeyboardFocus::Playlist, ArrowKey::Right) => KeyCommand::Seek(4),
            (KeyboardFocus::Equalizer, ArrowKey::Up) => KeyCommand::EqualizerAdjust(-4),
            (KeyboardFocus::Equalizer, ArrowKey::Down) => KeyCommand::EqualizerAdjust(4),
            (KeyboardFocus::Equalizer, ArrowKey::Left) if self.equalizer.panel.shaded => {
                KeyCommand::Volume(-4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Right) if self.equalizer.panel.shaded => {
                KeyCommand::Volume(4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Left)
                if self.equalizer.keyboard_slider == Some(EqualizerSlider::ShadedBalance) =>
            {
                KeyCommand::Balance(-4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Right)
                if self.equalizer.keyboard_slider == Some(EqualizerSlider::ShadedBalance) =>
            {
                KeyCommand::Balance(4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Left) => KeyCommand::Seek(-4),
            (KeyboardFocus::Equalizer, ArrowKey::Right) => KeyCommand::Seek(4),
        }
    }

    fn apply_key_command(&mut self, command: KeyCommand) -> bool {
        match command {
            KeyCommand::Volume(diff) => self.adjust_volume_by(diff),
            KeyCommand::Balance(diff) => self.adjust_balance_by(diff),
            KeyCommand::Seek(diff) => self.adjust_main_seek(diff),
            KeyCommand::PreviousTrack => {
                self.activate_push(MainPushButton::Previous);
                true
            }
            KeyCommand::NextTrack => {
                self.activate_push(MainPushButton::Next);
                true
            }
            KeyCommand::PlaylistMove(delta) => self.move_playlist_arrow_selection(delta),
            KeyCommand::EqualizerAdjust(diff) => self.adjust_selected_equalizer_slider(diff),
        }
    }

    pub(crate) fn handle_docked_vertical_arrow(&mut self, delta: isize) -> bool {
        let arrow = if delta < 0 {
            ArrowKey::Up
        } else {
            ArrowKey::Down
        };
        self.apply_key_command(self.arrow_key_command(self.current_keyboard_focus(), arrow))
    }

    pub(crate) fn handle_docked_horizontal_arrow(&mut self, diff: i32) -> bool {
        let arrow = if diff < 0 {
            ArrowKey::Left
        } else {
            ArrowKey::Right
        };
        self.apply_key_command(self.arrow_key_command(self.current_keyboard_focus(), arrow))
    }

    pub(crate) fn docked_focus_is_main(&self) -> bool {
        self.main_focused()
    }

    pub(crate) fn docked_focus_is_panel(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Equalizer => self.equalizer_focused(),
            PanelKind::Playlist => self.playlist_focused(),
        }
    }

    fn selected_docked_panel(&self) -> Option<PanelKind> {
        match self.docked_focus {
            KeyboardFocus::Main => None,
            KeyboardFocus::Equalizer => self
                .panel_state(PanelKind::Equalizer)
                .is_docked_visible()
                .then_some(PanelKind::Equalizer),
            KeyboardFocus::Playlist => self
                .panel_state(PanelKind::Playlist)
                .is_docked_visible()
                .then_some(PanelKind::Playlist),
        }
    }

    fn equalizer_render_state(&self) -> EqualizerRenderState {
        EqualizerRenderState {
            focused: self.equalizer_focused(),
            shaded: self.equalizer.panel.shaded,
            active: self.equalizer.active,
            automatic: self.equalizer.automatic,
            pressed_control: self.equalizer.pointer.pressed_control(),
            pressed_slider: self.equalizer.pointer.dragging_slider(),
            preamp_position: self.equalizer.preamp_position,
            band_positions: self.equalizer.band_positions,
            volume_position: volume_to_eq_shaded_position(self.app_state.player.volume()),
            balance_position: balance_to_eq_shaded_position(self.app_state.player.balance()),
        }
    }

    fn panel_state(&self, kind: PanelKind) -> PanelState {
        self.panel_placement(kind).state()
    }

    fn panel_placement(&self, kind: PanelKind) -> PanelPlacement {
        match kind {
            PanelKind::Equalizer => self.equalizer.panel,
            PanelKind::Playlist => self.playlist_ui.panel,
        }
    }

    fn panel_placement_mut(&mut self, kind: PanelKind) -> &mut PanelPlacement {
        match kind {
            PanelKind::Equalizer => &mut self.equalizer.panel,
            PanelKind::Playlist => &mut self.playlist_ui.panel,
        }
    }

    fn sync_panel_config_from_placement(&mut self) {
        self.app_state.config.equalizer_visible = self.equalizer.panel.visible;
        self.app_state.config.equalizer_detached = self.equalizer.panel.detached;
        self.app_state.config.equalizer_shaded = self.equalizer.panel.shaded;
        self.app_state.config.playlist_visible = self.playlist_ui.panel.visible;
        self.app_state.config.playlist_detached = self.playlist_ui.panel.detached;
        self.app_state.config.playlist_shaded = self.playlist_ui.panel.shaded;
    }

    pub(crate) fn panel_visibility(&self) -> PanelVisibility {
        PanelVisibility {
            equalizer: self.panel_state(PanelKind::Equalizer).is_detached_visible(),
            playlist: self.panel_state(PanelKind::Playlist).is_detached_visible(),
        }
    }

    pub(crate) fn docked_panel_state(&self) -> DockedPanelState {
        DockedPanelState {
            main_focused: self.main_focused(),
            main_shaded: self.shaded,
            equalizer_visible: self.equalizer.panel.visible,
            equalizer_detached: self.equalizer.panel.detached,
            equalizer_focused: self.equalizer_focused(),
            equalizer_shaded: self.equalizer.panel.shaded,
            playlist_visible: self.playlist_ui.panel.visible,
            playlist_detached: self.playlist_ui.panel.detached,
            playlist_focused: self.playlist_focused(),
            playlist_shaded: self.playlist_ui.panel.shaded,
            playlist_width: self.playlist_ui.width,
            playlist_height: self.playlist_ui.height,
        }
    }

    pub(crate) fn docked_panel_size(&self) -> (i32, i32) {
        docked_panel_size(self.docked_panel_state())
    }

    pub(crate) fn docked_panel_at(&self, x: i32, y: i32) -> Option<(PanelKind, i32, i32)> {
        let mut offset_y = main_window_height(self.shaded);
        if self.panel_state(PanelKind::Equalizer).is_docked_visible() {
            let height = equalizer_window_height(self.equalizer.panel.shaded);
            if (0..EQUALIZER_WINDOW_WIDTH).contains(&x) && y >= offset_y && y < offset_y + height {
                return Some((PanelKind::Equalizer, x, y - offset_y));
            }
            offset_y += height;
        }

        if self.panel_state(PanelKind::Playlist).is_docked_visible() {
            let height =
                playlist_window_height(self.playlist_ui.panel.shaded, self.playlist_ui.height);
            if x >= 0 && x < self.playlist_ui.width && y >= offset_y && y < offset_y + height {
                return Some((PanelKind::Playlist, x, y - offset_y));
            }
        }

        None
    }

    fn docked_playlist_local_y(&self, y: i32) -> Option<i32> {
        if !self.panel_state(PanelKind::Playlist).is_docked_visible() {
            return None;
        }
        let mut offset_y = main_window_height(self.shaded);
        if self.panel_state(PanelKind::Equalizer).is_docked_visible() {
            offset_y += equalizer_window_height(self.equalizer.panel.shaded);
        }
        Some(y - offset_y)
    }

    pub(crate) fn set_panel_detached(&mut self, kind: PanelKind, detached: bool) {
        self.panel_placement_mut(kind).detached = detached;
        self.sync_panel_config_from_placement();
    }

    pub(crate) fn is_panel_detached(&self, kind: PanelKind) -> bool {
        self.panel_placement(kind).detached
    }

    pub(crate) fn is_shaded(&self) -> bool {
        self.shaded
    }

    pub(crate) fn is_menu_visible(&self) -> bool {
        self.menu_visible
    }

    pub(crate) fn set_menu_visible(&mut self, visible: bool) {
        self.menu_visible = visible;
    }

    pub(crate) fn is_equalizer_shaded(&self) -> bool {
        self.equalizer.panel.shaded
    }

    pub(crate) fn is_playlist_shaded(&self) -> bool {
        self.playlist_ui.panel.shaded
    }

    pub(crate) fn playlist_menu(&self) -> Option<PlaylistMenuKind> {
        self.playlist_ui.menu.kind()
    }

    pub(crate) fn playlist_menu_hover(&self) -> Option<usize> {
        self.playlist_ui.menu.hover()
    }

    pub(crate) fn playlist_menu_pressed(&self) -> bool {
        self.playlist_ui.menu.pressed()
    }

    pub(crate) fn playlist_size(&self) -> (i32, i32) {
        (self.playlist_ui.width, self.playlist_ui.height)
    }

    pub(crate) fn playlist_scroll_offset(&self) -> usize {
        self.playlist_ui.scroll_offset
    }

    pub(crate) fn playlist_scrollbar_visible(&self) -> bool {
        self.playlist_scrollbar_geometry().is_some()
    }

    pub(crate) fn playlist_search_active(&self) -> bool {
        self.playlist_ui.search.is_active()
    }

    pub(crate) fn playlist_search_query(&self) -> &str {
        self.playlist_ui.search.query()
    }

    pub(crate) fn set_playlist_visible(&mut self, visible: bool) {
        self.playlist_ui.panel.visible = visible;
        self.sync_panel_config_from_placement();
    }

    pub(crate) fn is_preferences_visible(&self) -> bool {
        self.dialogs.preferences
    }

    pub(crate) fn set_preferences_visible(&mut self, visible: bool) {
        self.dialogs.preferences = visible;
    }

    pub(crate) fn preferences_page(&self) -> PreferencesPage {
        self.preferences_page
    }

    pub(crate) fn set_preferences_page(&mut self, page: PreferencesPage) {
        self.preferences_page = page;
    }

    pub(crate) fn preferences_saved(&self) -> bool {
        self.preferences_saved
    }

    fn mark_preferences_saved(&mut self) {
        self.preferences_saved = true;
    }

    fn apply_config_to_ui_state(&mut self) {
        self.equalizer.panel = PanelPlacement::from_config(
            self.app_state.config.equalizer_visible,
            self.app_state.config.equalizer_detached,
            self.app_state.config.equalizer_shaded,
        );
        self.playlist_ui.panel = PanelPlacement::from_config(
            self.app_state.config.playlist_visible,
            self.app_state.config.playlist_detached,
            self.app_state.config.playlist_shaded,
        );
        self.equalizer.active = self.app_state.config.equalizer_active;
        self.equalizer.automatic = self.app_state.config.equalizer_auto;
        self.equalizer.preamp_position = self.app_state.config.equalizer_preamp_pos;
        self.equalizer.band_positions = self.app_state.config.equalizer_band_pos;
        self.playback_position_ms = self.app_state.config.playback_position_ms.max(0);
        self.playback_transition =
            PlaybackTransitionState::stopped_at_or_idle(self.playback_position_ms);
        self.position_position = self.position_slider_position();
        self.apply_visualization_preferences();
    }

    fn sync_config_from_ui_state(&mut self) {
        self.app_state.config.playback_position_ms = self.playback_position_ms.max(0);
        self.app_state.config.main_shaded = self.shaded;
        self.sync_panel_config_from_placement();
        self.app_state.config.equalizer_active = self.equalizer.active;
        self.app_state.config.equalizer_auto = self.equalizer.automatic;
        self.app_state.config.equalizer_preamp_pos = self.equalizer.preamp_position;
        self.app_state.config.equalizer_band_pos = self.equalizer.band_positions;
    }

    pub(crate) fn reset_preferences_to_defaults(&mut self) {
        self.app_state.config = Config::default();
        self.app_state.apply_config_to_runtime();
        self.apply_config_to_ui_state();
        self.mark_preferences_saved();
    }

    pub(crate) fn is_open_location_visible(&self) -> bool {
        self.dialogs.open_location
    }

    pub(crate) fn set_open_location_visible(&mut self, visible: bool) {
        self.dialogs.open_location = visible;
    }

    pub(crate) fn is_jump_time_visible(&self) -> bool {
        self.dialogs.jump_time
    }

    pub(crate) fn set_jump_time_visible(&mut self, visible: bool) {
        self.dialogs.jump_time = visible;
    }

    pub(crate) fn is_skin_browser_visible(&self) -> bool {
        self.dialogs.skin_browser
    }

    pub(crate) fn set_skin_browser_visible(&mut self, visible: bool) {
        self.dialogs.skin_browser = visible;
    }

    pub(crate) fn set_skin_editor_visible(&mut self, visible: bool) {
        self.dialogs.skin_editor = visible;
    }

    pub(crate) fn is_output_device_picker_visible(&self) -> bool {
        self.dialogs.output_device_picker
    }

    pub(crate) fn set_output_device_picker_visible(&mut self, visible: bool) {
        self.dialogs.output_device_picker = visible;
    }

    pub(crate) fn set_output_devices(&mut self, system_devices: Vec<OutputDevice>) {
        self.output_device_groups = group_output_devices(system_devices);
    }

    pub(crate) fn output_device_groups(&self) -> &OutputDeviceGroups {
        &self.output_device_groups
    }

    pub(crate) fn selected_output_device(&self) -> Option<&str> {
        self.app_state.config.output_device.as_deref()
    }

    pub(crate) fn select_output_device(&mut self, selection: OutputDeviceSelection<'_>) -> bool {
        match selection {
            OutputDeviceSelection::Automatic => {
                self.app_state.config.output_device = None;
                self.output_switch_count = self.output_switch_count.saturating_add(1);
                true
            }
            OutputDeviceSelection::System(id) => {
                let found = self
                    .output_device_groups
                    .local
                    .iter()
                    .chain(self.output_device_groups.network.iter())
                    .any(|device| device.id == id);
                if !found {
                    return false;
                }
                self.app_state.config.output_device = Some(id.to_string());
                self.output_switch_count = self.output_switch_count.saturating_add(1);
                true
            }
        }
    }

    pub(crate) fn output_switch_count(&self) -> u32 {
        self.output_switch_count
    }

    pub(crate) fn mpris_root_properties(&self) -> MprisRootProperties {
        MprisRootProperties::default()
    }

    pub(crate) fn mpris_player_properties(&self) -> MprisPlayerProperties {
        MprisPlayerProperties {
            playback_status: mpris_playback_status(self.app_state.player.state()),
            rate: 1.0,
            metadata: self.mpris_metadata(),
            volume: f64::from(self.app_state.player.volume()) / 100.0,
            position_us: self.playback_position_ms * 1_000,
            can_go_next: true,
            can_go_previous: true,
            can_play: true,
            can_pause: true,
            can_seek: true,
            can_control: true,
        }
    }

    fn mpris_metadata(&self) -> MprisMetadata {
        let position = self.app_state.playlist.position().unwrap_or(0);
        MprisMetadata {
            track_id: format!("/org/xmms/Track/{position}"),
            title: self.playlist_entry_title(position).map(ToString::to_string),
            url: self.playlist_entry_uri(position).map(ToString::to_string),
            length_us: self
                .playlist_entry_length_ms(position)
                .filter(|length| *length > 0)
                .map(|length| length * 1000),
        }
    }

    pub(crate) fn mpris_events(&self) -> &[MprisEvent] {
        &self.mpris_events
    }

    pub(crate) fn take_mpris_events(&mut self) -> Vec<MprisEvent> {
        std::mem::take(&mut self.mpris_events)
    }

    pub(crate) fn mpris_quit_requested(&self) -> bool {
        self.mpris_quit_requested
    }

    pub(crate) fn set_mpris_volume(&mut self, volume: f64) {
        let percent = (volume * 100.0) as i32;
        self.app_state.player.set_volume(percent);
        self.app_state.config.volume = self.app_state.player.volume();
    }

    pub(crate) fn execute_mpris_command(&mut self, command: MprisCommand) {
        match command {
            MprisCommand::Raise => self.mpris_events.push(MprisEvent::Raised),
            MprisCommand::Quit => {
                self.mpris_quit_requested = true;
                self.mpris_events.push(MprisEvent::QuitRequested);
            }
            MprisCommand::Next => {
                let _ = self.activate_push(MainPushButton::Next);
                self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
            }
            MprisCommand::Previous => {
                let _ = self.activate_push(MainPushButton::Previous);
                self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
            }
            MprisCommand::Pause => {
                if self.handle_playback_control_event(PlaybackControlEvent::Pause) {
                    self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
                }
            }
            MprisCommand::PlayPause => {
                let _ = self.activate_push(MainPushButton::Pause);
                self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
            }
            MprisCommand::Stop => {
                let _ = self.activate_push(MainPushButton::Stop);
                self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
            }
            MprisCommand::Play => {
                self.handle_playback_control_event(PlaybackControlEvent::Play);
                self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
            }
            MprisCommand::Seek { offset_us } => {
                let position_us = (self.playback_position_ms * 1_000 + offset_us).max(0);
                self.set_playback_position_ms(position_us / 1_000);
                self.mpris_events
                    .push(MprisEvent::Seeked(self.playback_position_ms * 1_000));
            }
            MprisCommand::SetPosition {
                track_id: _,
                position_us,
            } => {
                self.set_playback_position_ms(position_us.max(0) / 1_000);
                self.mpris_events
                    .push(MprisEvent::Seeked(self.playback_position_ms * 1_000));
            }
            MprisCommand::OpenUri(uri) => {
                self.accept_dropped_uris([uri.as_str()], true, true);
                self.mpris_events.push(MprisEvent::MetadataChanged);
                self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
            }
        }
    }

    pub(crate) fn scan_skin_browser_dirs<P: AsRef<Path>>(&mut self, dirs: &[P]) -> io::Result<()> {
        self.skin_browser.entries = discover_skins_in_dirs(dirs)?;
        self.skin_browser.selected_index = self
            .app_state
            .config
            .skin
            .as_deref()
            .and_then(|current| {
                self.skin_browser
                    .entries
                    .iter()
                    .position(|entry| entry.path == Path::new(current))
                    .map(|index| index + 1)
            })
            .unwrap_or(0);
        Ok(())
    }

    pub(crate) fn skin_browser_entries(&self) -> &[SkinEntry] {
        &self.skin_browser.entries
    }

    pub(crate) fn selected_skin_index(&self) -> usize {
        self.skin_browser.selected_index
    }

    pub(crate) fn selected_skin(&self) -> Option<&str> {
        self.app_state.config.skin.as_deref()
    }

    pub(crate) fn select_skin_browser_index(&mut self, index: usize) -> bool {
        let previous_skin = self.app_state.config.skin.clone();
        let previous_index = self.skin_browser.selected_index;
        if index == 0 {
            self.app_state.config.skin = None;
            self.skin_browser.selected_index = 0;
        } else {
            let Some(entry) = self.skin_browser.entries.get(index - 1) else {
                return false;
            };
            self.app_state.config.skin = Some(entry.path.display().to_string());
            self.skin_browser.selected_index = index;
        }

        if let Err(err) = self.reload_skin() {
            eprintln!("xmms-rs: failed to load selected skin: {err}");
            self.app_state.config.skin = previous_skin;
            self.skin_browser.selected_index = previous_index;
            return false;
        }
        true
    }

    pub(crate) fn reload_skin(&mut self) -> io::Result<()> {
        self.load_configured_skin()?;
        self.skin_browser.reload_count = self.skin_browser.reload_count.saturating_add(1);
        Ok(())
    }

    pub(crate) fn skin_reload_count(&self) -> u32 {
        self.skin_browser.reload_count
    }

    pub(crate) fn clone_configured_skin_for_editor(&mut self) -> io::Result<()> {
        self.load_configured_skin()?;
        let name = self
            .app_state
            .config
            .skin
            .as_deref()
            .and_then(|path| Path::new(path).file_stem())
            .and_then(|name| name.to_str())
            .map(|name| format!("{name} copy"))
            .unwrap_or_else(|| "Default Skin Copy".to_string());
        self.skin_editor.working_name = name;
        Ok(())
    }

    pub(crate) fn save_editor_skin_to_user_dir(&mut self) -> io::Result<PathBuf> {
        let user_skin_dir = user_skin_import_dir();
        fs::create_dir_all(&user_skin_dir)?;
        let name = sanitized_skin_name(&self.skin_editor.working_name);
        let destination = unique_import_destination(&user_skin_dir, std::ffi::OsStr::new(&name));
        self.active_skin.save_to_dir(&destination)?;
        self.app_state.config.skin = Some(destination.display().to_string());
        self.reload_skin()?;
        self.scan_skin_browser_dirs(&runtime_skin_browser_dirs())?;
        Ok(destination)
    }

    pub(crate) fn export_editor_skin_wsz(&self, path: &Path) -> io::Result<()> {
        self.active_skin.export_wsz(path)
    }

    pub(crate) fn active_skin_pixel_argb(
        &self,
        kind: SkinPixmapKind,
        x: usize,
        y: usize,
    ) -> Option<u32> {
        self.active_skin
            .get(kind)
            .and_then(|image| image.pixel_argb(x, y))
    }

    pub(crate) fn toggle_sticky(&mut self) {
        self.app_state.config.sticky = !self.app_state.config.sticky;
    }

    pub(crate) fn sticky(&self) -> bool {
        self.app_state.config.sticky
    }

    pub(crate) fn toggle_double_size(&mut self) {
        self.double_fractional_scale();
    }

    pub(crate) fn double_fractional_scale(&mut self) {
        let scale = (self.app_state.config.scale_factor * 2.0).clamp(1.0, 5.0);
        self.app_state.config.scale_factor = scale;
        self.app_state.config.doublesize = scale > 1.0;
        self.mark_preferences_saved();
    }

    pub(crate) fn halve_fractional_scale(&mut self) {
        let scale = (self.app_state.config.scale_factor / 2.0).clamp(1.0, 5.0);
        self.app_state.config.scale_factor = scale;
        self.app_state.config.doublesize = scale > 1.0;
        self.mark_preferences_saved();
    }

    pub(crate) fn double_size(&self) -> bool {
        self.app_state.config.doublesize
    }

    pub(crate) fn toggle_easy_move(&mut self) {
        self.app_state.config.easy_move = !self.app_state.config.easy_move;
        self.mark_preferences_saved();
    }

    pub(crate) fn show_selected_or_current_file_info(&mut self) {
        self.last_playlist_file_info = self
            .selected_or_current_file_info_details()
            .map(|details| details.title);
    }

    pub(crate) fn selected_or_current_file_info_details(&mut self) -> Option<FileInfoDetails> {
        let details = self
            .selected_playlist_index()
            .or_else(|| self.app_state.playlist.position())
            .and_then(|index| self.app_state.playlist.entries().get(index))
            .or_else(|| self.app_state.playlist.entries().first())
            .map(file_info_details_for_entry);
        self.last_playlist_file_info = details.as_ref().map(|details| details.title.clone());
        self.dialogs.file_info = details.is_some();
        details
    }

    pub(crate) fn is_file_info_dialog_visible(&self) -> bool {
        self.dialogs.file_info
    }

    pub(crate) fn set_file_info_dialog_visible(&mut self, visible: bool) {
        self.dialogs.file_info = visible;
    }

    pub(crate) fn select_first_playlist_entry(&mut self) -> bool {
        if self.app_state.playlist.is_empty() {
            return false;
        }
        self.select_single_playlist_entry(0);
        self.scroll_playlist_entry_into_view(0);
        true
    }

    pub(crate) fn play_first_playlist_entry(&mut self) {
        if !self.app_state.playlist.is_empty() {
            self.app_state.playlist.set_position(0);
            self.app_state.player.mark_playing();
        }
    }

    pub(crate) fn is_file_dialog_visible(&self) -> bool {
        self.dialogs.file
    }

    pub(crate) fn set_file_dialog_visible(&mut self, visible: bool) {
        self.dialogs.file = visible;
    }

    pub(crate) fn is_directory_dialog_visible(&self) -> bool {
        self.dialogs.directory
    }

    pub(crate) fn set_directory_dialog_visible(&mut self, visible: bool) {
        self.dialogs.directory = visible;
    }

    pub(crate) fn is_playlist_load_dialog_visible(&self) -> bool {
        self.dialogs.playlist_load
    }

    pub(crate) fn set_playlist_load_dialog_visible(&mut self, visible: bool) {
        self.dialogs.playlist_load = visible;
    }

    pub(crate) fn is_playlist_save_dialog_visible(&self) -> bool {
        self.dialogs.playlist_save
    }

    pub(crate) fn set_playlist_save_dialog_visible(&mut self, visible: bool) {
        self.dialogs.playlist_save = visible;
    }

    pub(crate) fn last_playlist_file_info(&self) -> Option<&str> {
        self.last_playlist_file_info.as_deref()
    }

    pub(crate) fn update_playlist_title_for_uri(&mut self, uri: &str, title: &str) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        for entry in self.app_state.playlist.entries_mut() {
            if entry.filename == uri {
                entry.title = title.to_string();
            }
        }
        self.last_playlist_file_info = Some(title.to_string());
    }

    pub(crate) fn playlist_options_opened(&self) -> bool {
        self.playlist_options_opened
    }

    pub(crate) fn load_playlist_file(&mut self, path: &Path) -> std::io::Result<()> {
        self.app_state.playlist = Playlist::load_m3u_file(path)?;
        self.playlist_ui.scroll_offset = 0;
        self.playlist_ui.search.stop();
        self.schedule_missing_local_playlist_durations();
        Ok(())
    }

    pub(crate) fn save_playlist_file(&self, path: &Path) -> std::io::Result<()> {
        self.app_state.playlist.save_m3u_file(path)
    }

    pub(crate) fn last_open_location(&self) -> Option<&str> {
        self.last_open_location.as_deref()
    }

    pub(crate) fn last_jump_time_ms(&self) -> Option<i64> {
        self.last_jump_time_ms
    }

    pub(crate) fn playlist_len(&self) -> usize {
        self.app_state.playlist.len()
    }

    pub(crate) fn playlist_entry_uri(&self, index: usize) -> Option<&str> {
        self.app_state
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.filename.as_str())
    }

    pub(crate) fn playlist_entry_title(&self, index: usize) -> Option<&str> {
        self.app_state
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.title.as_str())
    }

    pub(crate) fn playlist_entry_length_ms(&self, index: usize) -> Option<i64> {
        self.app_state
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.length_ms)
    }

    pub(crate) fn playlist_entry_selected(&self, index: usize) -> Option<bool> {
        self.app_state
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.selected)
    }

    pub(crate) fn visible_playlist_entry_uri(&self, row: usize) -> Option<&str> {
        self.playlist_ui
            .scroll_offset
            .checked_add(row)
            .and_then(|index| self.playlist_entry_uri(index))
    }

    pub(crate) fn visible_playlist_entry_title(&self, row: usize) -> Option<String> {
        self.playlist_ui
            .scroll_offset
            .checked_add(row)
            .and_then(|index| self.app_state.playlist.entries().get(index))
            .map(|entry| self.formatted_playlist_entry_title(entry))
    }

    pub(crate) fn playlist_position(&self) -> Option<usize> {
        self.app_state.playlist.position()
    }

    pub(crate) fn current_playlist_entry_uri(&self) -> Option<&str> {
        self.app_state
            .playlist
            .position()
            .and_then(|position| self.playlist_entry_uri(position))
    }

    fn current_playlist_basename(&self) -> Option<String> {
        self.current_playlist_entry_uri().and_then(|uri| {
            file_uri_to_path(uri)
                .and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                })
                .or_else(|| uri.rsplit('/').next().map(ToString::to_string))
        })
    }

    fn start_current_playlist_playback(&mut self) {
        self.start_current_playlist_playback_at(
            self.playback_transition
                .play_start_position_ms(self.playback_position_ms),
        );
    }

    fn start_current_playlist_playback_from_beginning(&mut self) {
        self.start_current_playlist_playback_at(0);
    }

    fn start_current_playlist_playback_at(&mut self, position_ms: i64) {
        self.playback_transition = PlaybackTransitionState::start_playback();
        self.set_runtime_volume(self.app_state.config.volume);
        if self.app_state.playlist.position().is_none() && !self.app_state.playlist.is_empty() {
            self.app_state.playlist.set_position(0);
        }
        let Some(position) = self.app_state.playlist.position() else {
            self.stop_playback();
            return;
        };
        let Some(uri) = self.playlist_entry_uri(position).map(ToString::to_string) else {
            self.stop_playback();
            return;
        };
        self.playback_position_ms = position_ms.max(0);
        self.position_position = self.position_slider_position();
        self.load_equalizer_auto_preset_for_uri(&uri);
        self.playback_requests.push(uri.clone());
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().play_uri(&uri) {
                eprintln!("xmms-rs: failed to play {uri}: {err}");
                self.app_state.player.stop();
                return;
            }
            if self.playback_position_ms > 0 {
                self.playback_transition =
                    PlaybackTransitionState::request_backend_seek(self.playback_position_ms);
            }
        }
        self.app_state.player.mark_playing();
    }

    fn load_equalizer_auto_preset_for_uri(&mut self, uri: &str) {
        if !self.equalizer.automatic {
            return;
        }
        let Some(path) = file_uri_to_path(uri) else {
            self.load_equalizer_default_preset();
            return;
        };

        if !self.app_state.config.eqpreset_extension.is_empty() {
            let per_file = PathBuf::from(format!(
                "{}.{}",
                path.to_string_lossy(),
                self.app_state.config.eqpreset_extension
            ));
            match load_xmms_preset_file(&per_file) {
                Ok(Some(preset)) => {
                    self.apply_equalizer_preset_values(&preset);
                    return;
                }
                Ok(None) => {}
                Err(err) => eprintln!(
                    "xmms-rs: failed to load equalizer preset {}: {err}",
                    per_file.display()
                ),
            }
        }

        if !self.app_state.config.eqpreset_default_file.is_empty() {
            if let Some(parent) = path.parent() {
                let directory_preset = parent.join(&self.app_state.config.eqpreset_default_file);
                match load_xmms_preset_file(&directory_preset) {
                    Ok(Some(preset)) => {
                        self.apply_equalizer_preset_values(&preset);
                        return;
                    }
                    Ok(None) => {}
                    Err(err) => eprintln!(
                        "xmms-rs: failed to load equalizer preset {}: {err}",
                        directory_preset.display()
                    ),
                }
            }
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| self.load_named_equalizer_preset(name, true))
        {
            return;
        }
        self.load_equalizer_default_preset();
    }

    fn pause_playback(&mut self) {
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().pause() {
                eprintln!("xmms-rs: failed to pause playback: {err}");
            }
        }
        self.app_state.player.pause();
    }

    fn unpause_playback(&mut self) {
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().unpause() {
                eprintln!("xmms-rs: failed to resume playback: {err}");
            }
        }
        self.app_state.player.unpause();
    }

    fn handle_playback_control_event(&mut self, event: PlaybackControlEvent) -> bool {
        match event {
            PlaybackControlEvent::Play => match self.app_state.player.state() {
                PlayerState::Paused => {
                    self.unpause_playback();
                    true
                }
                PlayerState::Stopped => {
                    self.start_current_playlist_playback();
                    true
                }
                PlayerState::Playing => false,
            },
            PlaybackControlEvent::Pause => {
                if self.app_state.player.state() == PlayerState::Playing {
                    self.pause_playback();
                    true
                } else {
                    false
                }
            }
            PlaybackControlEvent::PauseToggle => match self.app_state.player.state() {
                PlayerState::Playing => {
                    self.pause_playback();
                    true
                }
                PlayerState::Paused => {
                    self.unpause_playback();
                    true
                }
                PlayerState::Stopped => false,
            },
            PlaybackControlEvent::Stop => {
                self.request_stop_playback();
                true
            }
            PlaybackControlEvent::Previous => {
                if self.app_state.playlist.previous() {
                    self.start_current_playlist_playback_from_beginning();
                }
                self.position_position = 0;
                self.playback_position_ms = 0;
                true
            }
            PlaybackControlEvent::Next => {
                if self.app_state.playlist.advance() {
                    self.start_current_playlist_playback_from_beginning();
                }
                self.position_position = 0;
                self.playback_position_ms = 0;
                true
            }
        }
    }

    fn stop_playback(&mut self) {
        self.playback_transition = PlaybackTransitionState::stop_playback();
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().stop() {
                eprintln!("xmms-rs: failed to stop playback: {err}");
            }
        }
        self.app_state.player.stop();
        self.app_state.player.clear_visualization_data();
        self.visualization.clear_data();
        self.position_position = 0;
        self.playback_position_ms = 0;
    }

    fn request_stop_playback(&mut self) {
        if self.app_state.config.stop_with_fadeout {
            self.stop_with_fade();
        } else {
            self.stop_playback();
        }
    }

    fn stop_with_fade(&mut self) {
        if self.app_state.player.state() == PlayerState::Stopped {
            self.stop_playback();
            return;
        }
        let start_volume = self.app_state.player.volume().max(0);
        if start_volume == 0 {
            self.stop_playback();
            self.set_runtime_volume(self.app_state.config.volume);
            return;
        }
        self.playback_transition = PlaybackTransitionState::start_fadeout(start_volume);
    }

    fn set_runtime_volume(&mut self, volume: i32) {
        let volume = volume.clamp(0, 100);
        self.app_state.player.set_volume(volume);
        if let Some(backend) = &self.playback_backend {
            backend.borrow().set_volume_percent(volume);
        }
    }

    pub(crate) fn playback_position_ms(&self) -> i64 {
        self.playback_position_ms
    }

    pub(crate) fn last_playback_request(&self) -> Option<&str> {
        self.playback_requests.last().map(String::as_str)
    }

    pub(crate) fn add_timed_entry(&mut self, uri: &str, title: &str, duration_ms: i64) {
        self.app_state
            .playlist
            .add_timed_uri(uri, title, duration_ms);
    }

    pub(crate) fn set_stream_channels_for_e2e(&mut self, channels: i32) {
        self.app_state
            .player
            .set_stream_info(None, None, Some(channels));
    }

    pub(crate) fn save_runtime_snapshot_for_e2e(
        &mut self,
        config_path: &Path,
        playlist_path: &Path,
    ) -> io::Result<()> {
        self.save_runtime_snapshot(config_path, playlist_path)
    }

    pub(crate) fn add_podcast_entry(
        &mut self,
        uri: &str,
        title: Option<String>,
        feed: Option<String>,
        guid: Option<String>,
    ) {
        self.app_state
            .playlist
            .add_podcast_entry(uri, title, feed, guid);
    }

    pub(crate) fn set_playlist_entry_selected(&mut self, index: usize, selected: bool) {
        if let Some(entry) = self.app_state.playlist.entries_mut().get_mut(index) {
            entry.selected = selected;
        }
    }

    pub(crate) fn start_playlist_search(&mut self) -> bool {
        if !self.app_state.config.vim_playlist_navigation {
            return false;
        }
        self.playlist_ui.menu.close();
        self.playlist_ui.search.start();
        true
    }

    pub(crate) fn stop_playlist_search(&mut self) {
        self.playlist_ui.search.stop();
    }

    pub(crate) fn push_playlist_search_char(&mut self, ch: char) {
        if self.playlist_ui.search.push_char(ch) {
            self.update_playlist_search_match();
        }
    }

    pub(crate) fn pop_playlist_search_char(&mut self) {
        if self.playlist_ui.search.pop_char() {
            self.update_playlist_search_match();
        }
    }

    pub(crate) fn sort_playlist_by(&mut self, key: PlaylistSortKey) {
        self.app_state.playlist.sort_by(key);
    }

    pub(crate) fn sort_selected_playlist_by(&mut self, key: PlaylistSortKey) {
        self.app_state.playlist.sort_selected_by(key);
    }

    pub(crate) fn remove_selected_playlist_entries(&mut self) -> bool {
        self.app_state.playlist.remove_selected()
    }

    pub(crate) fn reverse_playlist(&mut self) {
        self.app_state.playlist.reverse();
    }

    pub(crate) fn randomize_playlist(&mut self) {
        self.app_state.playlist.randomize();
    }

    pub(crate) fn index_missing_playlist_durations_for_e2e(&mut self) {
        let _ = self
            .app_state
            .playlist
            .index_missing_durations_with(|item| {
                Ok::<_, std::convert::Infallible>(Some(DurationIndexResult {
                    index: item.index,
                    uri: item.uri.clone(),
                    length_ms: ((item.index + 1) as i64) * 1_000,
                    title: Some(format!("Indexed {}", item.index + 1)),
                }))
            });
    }

    pub(crate) fn queue_playlist_duration_result_for_e2e(
        &mut self,
        index: usize,
        length_ms: i64,
        title: Option<String>,
    ) {
        let Some(uri) = self.playlist_entry_uri(index).map(ToString::to_string) else {
            return;
        };
        let _ = self.duration_index_sender.send(DurationIndexResult {
            index,
            uri,
            length_ms,
            title,
        });
    }

    fn schedule_missing_local_playlist_durations(&mut self) {
        let items = self
            .app_state
            .playlist
            .missing_duration_items()
            .into_iter()
            .filter(|item| file_uri_to_path(&item.uri).is_some_and(|path| path.exists()))
            .collect::<Vec<_>>();
        if items.is_empty() {
            return;
        }

        let sender = self.duration_index_sender.clone();
        thread::spawn(move || {
            if let Err(err) = gstreamer::init() {
                eprintln!("xmms-rs: failed to initialize GStreamer for playlist durations: {err}");
                return;
            }
            let discoverer =
                match gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5)) {
                    Ok(discoverer) => discoverer,
                    Err(err) => {
                        eprintln!("xmms-rs: failed to create playlist duration discoverer: {err}");
                        return;
                    }
                };

            for item in items {
                let Some(path) = file_uri_to_path(&item.uri).filter(|path| path.exists()) else {
                    continue;
                };
                let info = match discoverer.discover_uri(&item.uri) {
                    Ok(info) => info,
                    Err(err) => {
                        eprintln!(
                            "xmms-rs: failed to discover playlist item {}: {err}",
                            path.display()
                        );
                        continue;
                    }
                };
                let length_ms = info
                    .duration()
                    .map(|duration| duration.mseconds() as i64)
                    .unwrap_or(-1);
                if sender
                    .send(DurationIndexResult {
                        index: item.index,
                        uri: item.uri,
                        length_ms,
                        title: None,
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
    }

    fn poll_duration_index_results(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.duration_index_receiver.try_recv() {
            changed |= self.app_state.playlist.apply_duration_index_result(result);
        }
        changed
    }

    pub(crate) fn accept_open_location(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.last_open_location = Some(text.to_string());
        match self.app_state.playlist.add_location(text) {
            Ok(added) => {
                if added > 0 {
                    self.schedule_missing_local_playlist_durations();
                    if self.app_state.playlist.position().is_none() {
                        self.app_state.playlist.set_position(0);
                    }
                    self.start_current_playlist_playback_from_beginning();
                }
            }
            Err(err) => eprintln!("xmms-rs: failed to add open location {text}: {err}"),
        }
        self.dialogs.open_location = false;
    }

    pub(crate) fn accept_dropped_uris<I, S>(
        &mut self,
        uris: I,
        clear_first: bool,
        start_playback: bool,
    ) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut accepted = false;
        if clear_first {
            self.app_state.playlist.clear();
        }
        for location in uris {
            let location = location.as_ref();
            if location.is_empty() {
                continue;
            }
            match self.app_state.playlist.add_location(location) {
                Ok(added) => accepted |= added > 0,
                Err(err) => eprintln!("xmms-rs: failed to add playlist location {location}: {err}"),
            }
        }
        if accepted && clear_first {
            self.app_state.playlist.set_position(0);
        }
        if accepted {
            self.schedule_missing_local_playlist_durations();
        }
        if accepted && start_playback {
            self.start_current_playlist_playback_from_beginning();
        }
        accepted
    }

    pub(crate) fn accept_opened_uris<I, S>(&mut self, uris: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.accept_dropped_uris(uris, true, true)
    }

    pub(crate) fn accept_jump_time(&mut self, text: &str) {
        let Some(ms) = parse_time_ms(text) else {
            return;
        };
        self.last_jump_time_ms = Some(ms);
        self.set_playback_position_ms(ms);
        self.dialogs.jump_time = false;
    }

    pub(crate) fn set_playlist_size(&mut self, width: i32, height: i32) -> bool {
        let size = snap_playlist_size(width, height);
        let (width, height) = (size.width, size.height);
        let changed = self.playlist_ui.width != width || self.playlist_ui.height != height;
        self.playlist_ui.width = width;
        self.playlist_ui.height = height;
        self.clamp_playlist_scroll_offset();
        changed
    }

    pub(crate) fn set_panel_dragging(&mut self, kind: PanelKind, dragging: bool) {
        self.panel_placement_mut(kind).dragging_title = dragging;
    }

    pub(crate) fn set_panel_focused(&mut self, kind: PanelKind, focused: bool) {
        self.panel_placement_mut(kind).focused = focused;
    }

    pub(crate) fn is_panel_focused(&self, kind: PanelKind) -> bool {
        self.panel_placement(kind).focused
    }

    pub(crate) fn equalizer_active(&self) -> bool {
        self.equalizer.active
    }

    pub(crate) fn equalizer_automatic(&self) -> bool {
        self.equalizer.automatic
    }

    pub(crate) fn equalizer_preamp_position(&self) -> i32 {
        self.equalizer.preamp_position
    }

    pub(crate) fn equalizer_band_position(&self, band: usize) -> Option<i32> {
        self.equalizer.band_positions.get(band).copied()
    }

    pub(crate) fn equalizer_preamp_db(&self) -> f64 {
        equalizer_position_to_db(self.equalizer.preamp_position)
    }

    pub(crate) fn equalizer_band_db(&self, band: usize) -> Option<f64> {
        self.equalizer
            .band_positions
            .get(band)
            .map(|position| equalizer_position_to_db(*position))
    }

    pub(crate) fn equalizer_gstreamer_band_db_values(&self) -> [f64; 10] {
        if self.equalizer.active {
            self.equalizer.band_positions.map(equalizer_position_to_db)
        } else {
            [0.0; 10]
        }
    }

    pub(crate) fn equalizer_presets_pressed(&self) -> bool {
        self.equalizer.pointer.pressed_control() == Some(EqualizerControl::Presets)
    }

    pub(crate) fn equalizer_press(&mut self, x: i32, y: i32) -> bool {
        if self.equalizer.panel.shaded {
            if let Some(slider) = equalizer_shaded_slider_at(x, y) {
                self.equalizer.keyboard_slider = Some(slider);
                self.equalizer.pointer = EqualizerPointer::DraggingSlider {
                    slider,
                    offset: self.begin_equalizer_slider_drag(slider, x, y),
                };
                return true;
            }
            return false;
        }

        if let Some(control) = equalizer_control_at(x, y) {
            self.equalizer.keyboard_slider = None;
            self.equalizer.pointer = EqualizerPointer::PressedControl {
                control,
                inside: true,
            };
            return true;
        }

        if let Some(slider) = equalizer_slider_at(x, y) {
            self.equalizer.keyboard_slider = Some(slider);
            self.equalizer.pointer = EqualizerPointer::DraggingSlider {
                slider,
                offset: self.begin_equalizer_slider_drag(slider, x, y),
            };
            return true;
        }

        false
    }

    pub(crate) fn equalizer_motion(&mut self, x: i32, y: i32) -> bool {
        match self.equalizer.pointer {
            EqualizerPointer::Idle => false,
            EqualizerPointer::PressedControl { control, inside } => {
                let next_inside = equalizer_control_at(x, y) == Some(control);
                let changed = inside != next_inside;
                self.equalizer.pointer = EqualizerPointer::PressedControl {
                    control,
                    inside: next_inside,
                };
                changed
            }
            EqualizerPointer::DraggingSlider { slider, offset } => {
                let coordinate = match slider {
                    EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => x,
                    EqualizerSlider::Preamp | EqualizerSlider::Band(_) => y,
                };
                self.set_equalizer_slider_position(slider, coordinate, offset)
            }
        }
    }

    pub(crate) fn equalizer_scroll(&mut self, x: i32, y: i32, dy: f64) -> bool {
        let slider = if self.equalizer.panel.shaded {
            equalizer_shaded_slider_at(x, y)
        } else {
            equalizer_slider_at(x, y)
        };
        let Some(slider) = slider else {
            return false;
        };
        match slider {
            EqualizerSlider::ShadedVolume => self.scroll_volume(dy),
            EqualizerSlider::ShadedBalance => self.scroll_balance(dy),
            EqualizerSlider::Preamp | EqualizerSlider::Band(_) => {
                let diff = if dy < 0.0 {
                    -4
                } else if dy > 0.0 {
                    4
                } else {
                    return false;
                };
                self.adjust_equalizer_slider(slider, diff)
            }
        }
    }

    pub(crate) fn adjust_selected_equalizer_slider(&mut self, diff: i32) -> bool {
        let slider = self
            .equalizer
            .keyboard_slider
            .unwrap_or(EqualizerSlider::Preamp);
        self.adjust_equalizer_slider(slider, diff)
    }

    fn adjust_equalizer_slider(&mut self, slider: EqualizerSlider, diff: i32) -> bool {
        match slider {
            EqualizerSlider::Preamp => {
                let next = (self.equalizer.preamp_position + diff).clamp(0, 100);
                let changed = self.equalizer.preamp_position != next;
                self.equalizer.preamp_position = next;
                if changed {
                    self.sync_equalizer_to_backend();
                }
                changed
            }
            EqualizerSlider::Band(band) => {
                let Some(value) = self.equalizer.band_positions.get_mut(band) else {
                    return false;
                };
                let next = (*value + diff).clamp(0, 100);
                let changed = *value != next;
                *value = next;
                if changed {
                    self.sync_equalizer_to_backend();
                }
                changed
            }
            EqualizerSlider::ShadedVolume => self.adjust_volume_by(-diff),
            EqualizerSlider::ShadedBalance => self.adjust_balance_by(-diff),
        }
    }

    pub(crate) fn equalizer_release(&mut self, x: i32, y: i32) -> PanelAction {
        match std::mem::take(&mut self.equalizer.pointer) {
            EqualizerPointer::PressedControl { control, inside } => {
                let activated = inside && equalizer_control_at(x, y) == Some(control);
                if activated {
                    match control {
                        EqualizerControl::On => {
                            self.equalizer.active = !self.equalizer.active;
                            self.sync_equalizer_to_backend();
                        }
                        EqualizerControl::Auto => {
                            self.equalizer.automatic = !self.equalizer.automatic
                        }
                        EqualizerControl::Presets => return PanelAction::ShowEqualizerPresets,
                    }
                }
                PanelAction::Changed
            }
            EqualizerPointer::DraggingSlider { .. } => PanelAction::Changed,
            EqualizerPointer::Idle => PanelAction::None,
        }
    }

    pub(crate) fn apply_equalizer_preset(&mut self, preset: i32) {
        self.equalizer.preamp_position = 50;
        self.equalizer.band_positions = [50; 10];
        match preset {
            1 => {
                self.equalizer.band_positions[0] = 25;
                self.equalizer.band_positions[1] = 30;
                self.equalizer.band_positions[2] = 40;
            }
            2 => {
                self.equalizer.band_positions[7] = 40;
                self.equalizer.band_positions[8] = 30;
                self.equalizer.band_positions[9] = 25;
            }
            3 => {
                self.equalizer.band_positions[0] = 30;
                self.equalizer.band_positions[1] = 35;
                self.equalizer.band_positions[4] = 60;
                self.equalizer.band_positions[5] = 60;
                self.equalizer.band_positions[8] = 35;
                self.equalizer.band_positions[9] = 30;
            }
            _ => {}
        }
        self.sync_equalizer_to_backend();
    }

    fn set_equalizer_slider_position(
        &mut self,
        slider: EqualizerSlider,
        coordinate: i32,
        offset: i32,
    ) -> bool {
        let changed = match slider {
            EqualizerSlider::Preamp => {
                let position = eq_slider_pixel_to_position(
                    coordinate - equalizer_slider_layout(slider).rect.y - offset,
                );
                let changed = self.equalizer.preamp_position != position;
                self.equalizer.preamp_position = position;
                changed
            }
            EqualizerSlider::Band(band) => {
                let position = eq_slider_pixel_to_position(
                    coordinate - equalizer_slider_layout(slider).rect.y - offset,
                );
                let Some(value) = self.equalizer.band_positions.get_mut(band) else {
                    return false;
                };
                let changed = *value != position;
                *value = position;
                changed
            }
            EqualizerSlider::ShadedVolume => {
                let position =
                    (coordinate - equalizer_slider_layout(slider).rect.x - offset).clamp(0, 94);
                let volume = eq_shaded_position_to_volume(position);
                let changed = self.app_state.player.volume() != volume;
                self.app_state.player.set_volume(volume);
                changed
            }
            EqualizerSlider::ShadedBalance => {
                let position =
                    (coordinate - equalizer_slider_layout(slider).rect.x - offset).clamp(0, 39);
                let balance = eq_shaded_position_to_balance(position);
                let changed = self.app_state.player.balance() != balance;
                self.app_state.player.set_balance(balance);
                changed
            }
        };
        if changed {
            match slider {
                EqualizerSlider::Preamp | EqualizerSlider::Band(_) => {
                    self.sync_equalizer_to_backend()
                }
                EqualizerSlider::ShadedVolume => {
                    if let Some(backend) = &self.playback_backend {
                        backend
                            .borrow()
                            .set_volume_percent(self.app_state.player.volume());
                    }
                }
                EqualizerSlider::ShadedBalance => {
                    if let Some(backend) = &self.playback_backend {
                        backend
                            .borrow()
                            .set_balance_percent(self.app_state.player.balance());
                    }
                }
            }
        }
        changed
    }

    fn begin_equalizer_slider_drag(&mut self, slider: EqualizerSlider, x: i32, y: i32) -> i32 {
        let layout = equalizer_slider_layout(slider);
        match slider {
            EqualizerSlider::Preamp | EqualizerSlider::Band(_) => {
                let position = self.equalizer_slider_pixel_position(slider);
                let local_y = y - layout.rect.y;
                if local_y >= position && local_y < position + 11 {
                    local_y - position
                } else {
                    let offset = 5;
                    self.set_equalizer_slider_position(slider, y, offset);
                    offset
                }
            }
            EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => {
                let position = self.equalizer_slider_pixel_position(slider);
                let local_x = x - layout.rect.x;
                if local_x >= position && local_x < position + layout.knob_size.width {
                    local_x - position
                } else {
                    let offset = layout.knob_size.width / 2;
                    self.set_equalizer_slider_position(slider, x, offset);
                    offset
                }
            }
        }
    }

    fn equalizer_slider_pixel_position(&self, slider: EqualizerSlider) -> i32 {
        match slider {
            EqualizerSlider::Preamp => eq_slider_position_to_pixel(self.equalizer.preamp_position),
            EqualizerSlider::Band(band) => self
                .equalizer
                .band_positions
                .get(band)
                .copied()
                .map(eq_slider_position_to_pixel)
                .unwrap_or(25),
            EqualizerSlider::ShadedVolume => {
                volume_to_eq_shaded_position(self.app_state.player.volume())
            }
            EqualizerSlider::ShadedBalance => {
                balance_to_eq_shaded_position(self.app_state.player.balance())
            }
        }
    }

    fn sync_equalizer_to_backend(&self) {
        if let Some(backend) = &self.playback_backend {
            backend.borrow().set_equalizer_from_positions(
                self.equalizer.active,
                self.equalizer.preamp_position,
                self.equalizer.band_positions,
            );
        }
    }

    pub(crate) fn panel_title_drag_region(&self, kind: PanelKind, x: i32, y: i32) -> bool {
        let title_height = match kind {
            PanelKind::Equalizer => MAIN_TITLEBAR_HEIGHT,
            PanelKind::Playlist => 20,
        };
        y >= 0
            && y < title_height
            && !self.panel_title_button_hit(kind, x, y)
            && !(kind == PanelKind::Equalizer
                && self.equalizer.panel.shaded
                && equalizer_shaded_slider_at(x, y).is_some())
    }

    pub(crate) fn main_title_drag_region(&self, x: i32, y: i32) -> bool {
        (0..MAIN_TITLEBAR_HEIGHT).contains(&y) && self.hit_test(x, y).is_none()
    }

    pub(crate) fn playlist_resize_region(&self, x: i32, y: i32) -> bool {
        !self.playlist_ui.panel.shaded
            && x > self.playlist_ui.width - 20
            && y > self.playlist_ui.height - 20
    }

    pub(crate) fn begin_docked_playlist_resize(&mut self, local_y: i32) -> bool {
        if !self.playlist_resize_region(self.playlist_ui.width - 1, local_y) {
            return false;
        }
        self.playlist_ui.pointer = PlaylistPointer::Resizing {
            offset_y: self.playlist_ui.height - local_y,
        };
        true
    }

    pub(crate) fn docked_playlist_resize_motion(&mut self, main_y: i32) -> bool {
        let PlaylistPointer::Resizing { offset_y } = self.playlist_ui.pointer else {
            return false;
        };
        let Some(local_y) = self.docked_playlist_local_y(main_y) else {
            return false;
        };
        let height = local_y + offset_y;
        self.set_playlist_size(PLAYLIST_MIN_WIDTH, height)
    }

    pub(crate) fn end_docked_playlist_resize(&mut self) -> bool {
        if matches!(self.playlist_ui.pointer, PlaylistPointer::Resizing { .. }) {
            self.playlist_ui.pointer = PlaylistPointer::Idle;
            true
        } else {
            false
        }
    }

    pub(crate) fn is_docked_playlist_resizing(&self) -> bool {
        matches!(self.playlist_ui.pointer, PlaylistPointer::Resizing { .. })
    }

    pub(crate) fn playlist_scrollbar_press(&mut self, x: i32, y: i32) -> bool {
        let Some((thumb_y, thumb_h)) = self.playlist_scrollbar_geometry() else {
            return false;
        };
        if !self.playlist_scrollbar_region(x, y) {
            return false;
        }
        let offset = if y >= thumb_y && y < thumb_y + thumb_h {
            y - thumb_y
        } else {
            thumb_h / 2
        };
        self.playlist_ui.pointer = PlaylistPointer::DraggingScrollbar { offset };
        self.update_playlist_scroll_from_thumb_y(y - offset);
        true
    }

    pub(crate) fn playlist_scrollbar_motion(&mut self, x: i32, y: i32) -> bool {
        let PlaylistPointer::DraggingScrollbar { offset } = self.playlist_ui.pointer else {
            return false;
        };
        let old = self.playlist_ui.scroll_offset;
        let _ = x;
        self.update_playlist_scroll_from_thumb_y(y - offset);
        old != self.playlist_ui.scroll_offset
    }

    pub(crate) fn playlist_scrollbar_release(&mut self) -> bool {
        if matches!(
            self.playlist_ui.pointer,
            PlaylistPointer::DraggingScrollbar { .. }
        ) {
            self.playlist_ui.pointer = PlaylistPointer::Idle;
            true
        } else {
            false
        }
    }

    pub(crate) fn playlist_scroll(&mut self, dy: f64) -> bool {
        let rows = if dy < 0.0 {
            -3
        } else if dy > 0.0 {
            3
        } else {
            return false;
        };
        self.scroll_playlist_rows(rows)
    }

    fn scroll_playlist_rows(&mut self, rows: i32) -> bool {
        let old = self.playlist_ui.scroll_offset;
        if rows < 0 {
            self.playlist_ui.scroll_offset = self
                .playlist_ui
                .scroll_offset
                .saturating_sub(rows.unsigned_abs() as usize);
        } else {
            self.playlist_ui.scroll_offset =
                self.playlist_ui.scroll_offset.saturating_add(rows as usize);
            self.clamp_playlist_scroll_offset();
        }
        old != self.playlist_ui.scroll_offset
    }

    pub(crate) fn playlist_press(&mut self, x: i32, y: i32) -> bool {
        self.playlist_press_with_ctrl(x, y, false)
    }

    pub(crate) fn playlist_press_with_ctrl(&mut self, x: i32, y: i32, ctrl_pressed: bool) -> bool {
        if let Some(item) = self.playlist_menu_item_at(x, y) {
            return self.playlist_ui.menu.press_item(item);
        }
        if self.playlist_ui.menu.is_open() {
            return false;
        }

        let Some(index) = self.playlist_entry_at(x, y) else {
            return false;
        };
        if ctrl_pressed {
            if let Some(entry) = self.app_state.playlist.entries_mut().get_mut(index) {
                entry.selected = !entry.selected;
            }
            self.playlist_ui.last_click = None;
            self.playlist_ui.pending_double_click = None;
            self.playlist_ui.pointer = PlaylistPointer::Idle;
            return true;
        }

        let now = Instant::now();
        let is_double_click = self
            .playlist_ui
            .last_click
            .is_some_and(|(last_index, last_time)| {
                last_index == index && now.duration_since(last_time) <= Duration::from_millis(500)
            });

        self.playlist_ui.last_click = Some((index, now));
        self.playlist_ui.pending_double_click = is_double_click.then_some(index);
        self.select_single_playlist_entry(index);
        self.playlist_ui.pointer = PlaylistPointer::DraggingEntry {
            index,
            moved: false,
        };
        true
    }

    pub(crate) fn activate_playlist_entry_at(&mut self, x: i32, y: i32) -> bool {
        if self.playlist_ui.menu.is_open() {
            return false;
        }
        let Some(index) = self.playlist_entry_at(x, y) else {
            return false;
        };
        self.activate_playlist_entry(index);
        true
    }

    fn activate_playlist_entry(&mut self, index: usize) {
        self.select_single_playlist_entry(index);
        self.playlist_ui.last_click = None;
        self.playlist_ui.pending_double_click = None;
        self.playlist_ui.pointer = PlaylistPointer::Idle;
        self.app_state.playlist.set_position(index);
        self.start_current_playlist_playback_from_beginning();
    }

    pub(crate) fn playlist_motion(&mut self, x: i32, y: i32) -> bool {
        if let PlaylistPointer::DraggingEntry { index: from, .. } = self.playlist_ui.pointer {
            let Some(to) = self.playlist_entry_at(x, y) else {
                return false;
            };
            if self.app_state.playlist.move_entry(from, to) {
                self.playlist_ui.pending_double_click = None;
                self.playlist_ui.pointer = PlaylistPointer::DraggingEntry {
                    index: to,
                    moved: true,
                };
                self.scroll_playlist_entry_into_view(to);
                return true;
            }
            return false;
        }

        if !self.playlist_ui.menu.is_open() {
            return false;
        }
        let item = self.playlist_menu_item_at(x, y);
        self.playlist_ui.menu.set_hover(item)
    }

    pub(crate) fn playlist_entry_release(&mut self) -> bool {
        let PlaylistPointer::DraggingEntry { moved, .. } = self.playlist_ui.pointer else {
            return false;
        };
        self.playlist_ui.pointer = PlaylistPointer::Idle;
        if let Some(index) = self.playlist_ui.pending_double_click.take() {
            if !moved {
                self.activate_playlist_entry(index);
            }
        }
        true
    }

    pub(crate) fn playlist_release(&mut self, x: i32, y: i32) -> PanelAction {
        let menu = self.playlist_ui.menu.kind();
        let item = self.playlist_menu_item_at(x, y);
        let activated = item == self.playlist_ui.menu.hover();
        self.playlist_ui.menu.close();
        if activated {
            if let (Some(menu), Some(item)) = (menu, item) {
                self.activate_playlist_menu_item(menu, item)
            } else {
                PanelAction::Changed
            }
        } else {
            PanelAction::None
        }
    }

    fn activate_playlist_menu_item(&mut self, menu: PlaylistMenuKind, item: usize) -> PanelAction {
        let Some(command) = PlaylistMenuCommand::from_menu_item(menu, item) else {
            return PanelAction::None;
        };
        let changed = match command {
            PlaylistMenuCommand::OpenLocationWindow => return PanelAction::OpenLocationWindow,
            PlaylistMenuCommand::OpenDirectoryDialog => return PanelAction::OpenDirectoryDialog,
            PlaylistMenuCommand::OpenFileDialog => return PanelAction::OpenFileDialog,
            PlaylistMenuCommand::ShowSortMenu => return PanelAction::ShowPlaylistSortMenu,
            PlaylistMenuCommand::ShowFileInfo => return PanelAction::ShowFileInfo,
            PlaylistMenuCommand::OpenOptions => {
                self.playlist_options_opened = true;
                true
            }
            PlaylistMenuCommand::ClearList => {
                self.app_state.playlist.clear();
                true
            }
            PlaylistMenuCommand::CropToSelection => {
                self.app_state.playlist.crop_to_selected_or_current()
            }
            PlaylistMenuCommand::RemoveSelectedOrCurrent => {
                self.app_state.playlist.remove_selected_or_current()
            }
            PlaylistMenuCommand::InvertSelection => {
                self.app_state.playlist.invert_selection();
                true
            }
            PlaylistMenuCommand::SelectNone => {
                self.app_state.playlist.select_all(false);
                true
            }
            PlaylistMenuCommand::SelectAll => {
                self.app_state.playlist.select_all(true);
                true
            }
            PlaylistMenuCommand::SavePlaylist => return PanelAction::OpenPlaylistSaveDialog,
            PlaylistMenuCommand::LoadPlaylist => return PanelAction::OpenPlaylistLoadDialog,
        };
        if changed {
            self.clamp_playlist_scroll_offset();
            PanelAction::Changed
        } else {
            PanelAction::None
        }
    }

    pub(crate) fn activate_playlist_context_action(
        &mut self,
        action: PlaylistContextAction,
    ) -> bool {
        let changed = match action {
            PlaylistContextAction::RemoveSelected => {
                self.app_state.playlist.remove_selected_or_current()
            }
            PlaylistContextAction::RemoveDead => self.app_state.playlist.remove_dead_files(),
            PlaylistContextAction::PhysicallyDelete => {
                match self.app_state.playlist.physically_delete_selected() {
                    Ok(deleted) => deleted > 0,
                    Err(err) => {
                        eprintln!("xmms-rs: failed to physically delete playlist entry: {err}");
                        false
                    }
                }
            }
            PlaylistContextAction::SelectAll => {
                self.app_state.playlist.select_all(true);
                true
            }
            PlaylistContextAction::SelectNone => {
                self.app_state.playlist.select_all(false);
                true
            }
            PlaylistContextAction::InvertSelection => {
                self.app_state.playlist.invert_selection();
                true
            }
        };
        if changed {
            self.clamp_playlist_scroll_offset();
        }
        changed
    }

    pub(crate) fn activate_playlist_sort_action(&mut self, action: PlaylistSortAction) -> bool {
        match action {
            PlaylistSortAction::ListByTitle => {
                self.app_state.playlist.sort_by(PlaylistSortKey::Title)
            }
            PlaylistSortAction::ListByFilename => {
                self.app_state.playlist.sort_by(PlaylistSortKey::Filename)
            }
            PlaylistSortAction::ListByPath => {
                self.app_state.playlist.sort_by(PlaylistSortKey::Path)
            }
            PlaylistSortAction::ListByDate => {
                self.app_state.playlist.sort_by(PlaylistSortKey::Date)
            }
            PlaylistSortAction::SelectionByTitle => self
                .app_state
                .playlist
                .sort_selected_by(PlaylistSortKey::Title),
            PlaylistSortAction::SelectionByFilename => self
                .app_state
                .playlist
                .sort_selected_by(PlaylistSortKey::Filename),
            PlaylistSortAction::SelectionByPath => self
                .app_state
                .playlist
                .sort_selected_by(PlaylistSortKey::Path),
            PlaylistSortAction::SelectionByDate => self
                .app_state
                .playlist
                .sort_selected_by(PlaylistSortKey::Date),
            PlaylistSortAction::RandomizeList => self.app_state.playlist.randomize(),
            PlaylistSortAction::ReverseList => self.app_state.playlist.reverse(),
        }
        self.clamp_playlist_scroll_offset();
        true
    }

    fn update_playlist_search_match(&mut self) {
        let query = self.playlist_ui.search.query();
        if query.is_empty() {
            return;
        }
        let total = self.app_state.playlist.len();
        if total == 0 {
            return;
        }
        let query = query.to_lowercase();
        let start = self
            .selected_playlist_index()
            .or_else(|| self.app_state.playlist.position())
            .unwrap_or(0)
            .min(total);

        for index in (start..total).chain(0..start) {
            let Some(entry) = self.app_state.playlist.entries().get(index) else {
                continue;
            };
            let text = if entry.title.is_empty() {
                &entry.filename
            } else {
                &entry.title
            };
            if text.to_lowercase().contains(&query) {
                self.select_single_playlist_entry(index);
                self.scroll_playlist_entry_into_view(index);
                return;
            }
        }
    }

    fn selected_playlist_index(&self) -> Option<usize> {
        self.app_state
            .playlist
            .entries()
            .iter()
            .position(|entry| entry.selected)
    }

    pub(crate) fn move_playlist_selection(&mut self, delta: isize) -> bool {
        if !self.app_state.config.vim_playlist_navigation {
            return false;
        }
        self.move_playlist_selection_by(delta)
    }

    pub(crate) fn move_playlist_arrow_selection(&mut self, delta: isize) -> bool {
        self.move_playlist_selection_by(delta)
    }

    fn move_playlist_selection_by(&mut self, delta: isize) -> bool {
        let len = self.app_state.playlist.len();
        if len == 0 {
            return false;
        }
        let current = self
            .selected_playlist_index()
            .or_else(|| self.app_state.playlist.position())
            .unwrap_or(if delta < 0 { len - 1 } else { 0 });
        let next = current.saturating_add_signed(delta).min(len - 1);
        self.select_single_playlist_entry(next);
        self.scroll_playlist_entry_into_view(next);
        true
    }

    pub(crate) fn move_playlist_page(&mut self, direction: isize) -> bool {
        let visible = self.playlist_visible_entries().max(1) as isize;
        self.move_playlist_selection_by(direction.signum() * visible)
    }

    pub(crate) fn move_playlist_to_start(&mut self) -> bool {
        self.select_first_playlist_entry()
    }

    pub(crate) fn move_playlist_to_end(&mut self) -> bool {
        let Some(last) = self.app_state.playlist.len().checked_sub(1) else {
            return false;
        };
        self.select_single_playlist_entry(last);
        self.scroll_playlist_entry_into_view(last);
        true
    }

    pub(crate) fn crop_playlist_to_selected_or_current(&mut self) -> bool {
        self.app_state.playlist.crop_to_selected_or_current()
    }

    pub(crate) fn toggle_queue_selected_playlist_entries(&mut self) -> bool {
        let selected = self
            .app_state
            .playlist
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.selected.then_some(index))
            .collect::<Vec<_>>();
        let targets = if selected.is_empty() {
            self.selected_playlist_index()
                .or_else(|| self.app_state.playlist.position())
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            selected
        };
        if targets.is_empty() {
            return false;
        }
        for index in targets {
            if let Some(position) = self
                .playlist_queue
                .iter()
                .position(|queued| *queued == index)
            {
                self.playlist_queue.remove(position);
            } else {
                self.playlist_queue.push(index);
            }
        }
        true
    }

    pub(crate) fn clear_playlist_queue(&mut self) -> bool {
        let changed = !self.playlist_queue.is_empty();
        self.playlist_queue.clear();
        changed
    }

    pub(crate) fn open_queue_manager(&mut self) -> bool {
        self.queue_manager_opened = true;
        true
    }

    pub(crate) fn play_selected_playlist_entry(&mut self) -> bool {
        if !self.app_state.config.vim_playlist_navigation {
            return false;
        }
        self.activate_selected_or_current_playlist_entry()
    }

    pub(crate) fn activate_selected_or_current_playlist_entry(&mut self) -> bool {
        let Some(index) = self
            .selected_playlist_index()
            .or_else(|| self.app_state.playlist.position())
            .or_else(|| (!self.app_state.playlist.is_empty()).then_some(0))
        else {
            return false;
        };
        self.activate_playlist_entry(index);
        true
    }

    fn select_single_playlist_entry(&mut self, index: usize) {
        for (entry_index, entry) in self.app_state.playlist.entries_mut().iter_mut().enumerate() {
            entry.selected = entry_index == index;
        }
    }

    fn scroll_playlist_entry_into_view(&mut self, index: usize) {
        let visible = self.playlist_visible_entries();
        if visible == 0 {
            return;
        }
        if index < self.playlist_ui.scroll_offset {
            self.playlist_ui.scroll_offset = index;
        } else if index >= self.playlist_ui.scroll_offset + visible {
            self.playlist_ui.scroll_offset = index + 1 - visible;
        }
        self.clamp_playlist_scroll_offset();
    }

    fn playlist_visible_entries(&self) -> usize {
        ((self.playlist_ui.height - 58).max(0) / 11) as usize
    }

    fn playlist_entry_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.playlist_ui.panel.shaded || !(12..self.playlist_ui.width - 19).contains(&x) {
            return None;
        }
        if !(20..self.playlist_ui.height - 38).contains(&y) {
            return None;
        }
        let row = ((y - 20) / 11) as usize;
        if row >= self.playlist_visible_entries() {
            return None;
        }
        let index = self.playlist_ui.scroll_offset + row;
        (index < self.app_state.playlist.len()).then_some(index)
    }

    fn playlist_max_scroll(&self) -> usize {
        self.app_state
            .playlist
            .len()
            .saturating_sub(self.playlist_visible_entries())
    }

    fn clamp_playlist_scroll_offset(&mut self) {
        self.playlist_ui.scroll_offset = self
            .playlist_ui
            .scroll_offset
            .min(self.playlist_max_scroll());
    }

    fn playlist_scrollbar_region(&self, x: i32, y: i32) -> bool {
        !self.playlist_ui.panel.shaded
            && x >= self.playlist_ui.width - 15
            && x < self.playlist_ui.width - 7
            && y >= 20
            && y < self.playlist_ui.height - 38
    }

    fn playlist_scrollbar_geometry(&self) -> Option<(i32, i32)> {
        let visible = self.playlist_visible_entries();
        let total = self.app_state.playlist.len();
        if total <= visible || visible == 0 {
            return None;
        }
        let list_h = self.playlist_ui.height - 58;
        let thumb_h = 18;
        let max_scroll = total - visible;
        let max_thumb_pos = (list_h - thumb_h).max(0);
        let thumb_y = 20
            + ((self.playlist_ui.scroll_offset.min(max_scroll) as i32 * max_thumb_pos)
                / max_scroll.max(1) as i32);
        Some((thumb_y, thumb_h))
    }

    fn update_playlist_scroll_from_thumb_y(&mut self, thumb_y: i32) {
        let visible = self.playlist_visible_entries();
        let total = self.app_state.playlist.len();
        if total <= visible || visible == 0 {
            self.playlist_ui.scroll_offset = 0;
            return;
        }
        let list_h = self.playlist_ui.height - 58;
        let thumb_h = 18;
        let max_scroll = total - visible;
        let max_thumb_pos = (list_h - thumb_h).max(0);
        if max_thumb_pos <= 0 {
            self.playlist_ui.scroll_offset = 0;
            return;
        }
        let thumb_pos = (thumb_y - 20).clamp(0, max_thumb_pos);
        self.playlist_ui.scroll_offset = ((thumb_pos as usize * max_scroll)
            + (max_thumb_pos as usize / 2))
            / max_thumb_pos as usize;
    }

    fn playlist_menu_item_at(&self, x: i32, y: i32) -> Option<usize> {
        let menu = self.playlist_ui.menu.kind()?;
        let (menu_x, menu_y, menu_width, menu_height) =
            playlist_menu_rect(menu, self.playlist_ui.width, self.playlist_ui.height);
        if x < menu_x || x >= menu_x + menu_width || y < menu_y || y >= menu_y + menu_height {
            return None;
        }
        Some(((y - menu_y) / 18) as usize)
    }

    pub(crate) fn panel_click(&mut self, kind: PanelKind, x: i32, y: i32) -> PanelAction {
        if kind == PanelKind::Playlist {
            self.playlist_ui.menu.close();
            if matches!(
                self.playlist_ui.pointer,
                PlaylistPointer::DraggingEntry { .. }
            ) {
                self.playlist_ui.pointer = PlaylistPointer::Idle;
            }
        }

        if self.panel_title_button_hit(kind, x, y) {
            if self.panel_close_button_hit(kind, x) {
                self.panel_placement_mut(kind).visible = false;
                self.sync_panel_config_from_placement();
                return PanelAction::Changed;
            }

            if self.panel_shade_button_hit(kind, x) {
                self.toggle_panel_shaded(kind);
                return PanelAction::Changed;
            }
        }

        if kind == PanelKind::Playlist && !self.playlist_ui.panel.shaded {
            if let Some(menu) =
                playlist_menu_at(x, y, self.playlist_ui.width, self.playlist_ui.height)
            {
                self.playlist_ui.menu.open(menu);
                return PanelAction::ShowPlaylistMenu(menu);
            }
            if let Some(button) =
                playlist_footer_button_at(x, y, self.playlist_ui.width, self.playlist_ui.height)
            {
                return self.activate_playlist_footer_button(button);
            }
        }

        PanelAction::None
    }

    fn activate_playlist_footer_button(&mut self, button: PlaylistFooterButton) -> PanelAction {
        match button {
            PlaylistFooterButton::Previous => {
                self.handle_playback_control_event(PlaybackControlEvent::Previous);
                PanelAction::Changed
            }
            PlaylistFooterButton::Play => {
                self.handle_playback_control_event(PlaybackControlEvent::Play);
                PanelAction::Changed
            }
            PlaylistFooterButton::Pause => {
                self.handle_playback_control_event(PlaybackControlEvent::PauseToggle);
                PanelAction::Changed
            }
            PlaylistFooterButton::Stop => {
                self.handle_playback_control_event(PlaybackControlEvent::Stop);
                PanelAction::Changed
            }
            PlaylistFooterButton::Next => {
                self.handle_playback_control_event(PlaybackControlEvent::Next);
                PanelAction::Changed
            }
            PlaylistFooterButton::Eject => PanelAction::OpenFileDialog,
            PlaylistFooterButton::ScrollUp => {
                self.scroll_playlist_rows(-1);
                PanelAction::Changed
            }
            PlaylistFooterButton::ScrollDown => {
                self.scroll_playlist_rows(1);
                PanelAction::Changed
            }
        }
    }

    fn panel_title_button_hit(&self, kind: PanelKind, x: i32, y: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, y, self.playlist_ui.width).is_some()
    }

    fn panel_shade_button_hit(&self, kind: PanelKind, x: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, 7, self.playlist_ui.width)
            == Some(PanelTitleButton::Shade)
    }

    fn panel_close_button_hit(&self, kind: PanelKind, x: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, 7, self.playlist_ui.width)
            == Some(PanelTitleButton::Close)
    }

    pub(crate) fn player_state(&self) -> PlayerState {
        self.app_state.player.state()
    }

    pub(crate) fn shuffle(&self) -> bool {
        self.app_state.playlist.shuffle()
    }

    pub(crate) fn repeat(&self) -> bool {
        self.app_state.playlist.repeat()
    }

    pub(crate) fn no_advance(&self) -> bool {
        self.app_state.playlist.no_advance()
    }

    pub(crate) fn set_no_advance(&mut self, enabled: bool) {
        self.app_state.playlist.set_no_advance(enabled);
    }

    pub(crate) fn toggle_shaded(&mut self) {
        self.shaded = !self.shaded;
    }

    pub(crate) fn toggle_selected_window_shade(&mut self) -> Option<PanelKind> {
        match self.selected_docked_panel() {
            Some(kind) => {
                self.toggle_panel_shaded(kind);
                Some(kind)
            }
            None => {
                self.toggle_shaded();
                None
            }
        }
    }

    pub(crate) fn toggle_playlist_shaded(&mut self) {
        self.toggle_panel_shaded(PanelKind::Playlist);
    }

    pub(crate) fn toggle_equalizer_shaded(&mut self) {
        self.toggle_panel_shaded(PanelKind::Equalizer);
    }

    fn toggle_panel_shaded(&mut self, kind: PanelKind) {
        let placement = self.panel_placement_mut(kind);
        placement.shaded = !placement.shaded;
        self.sync_panel_config_from_placement();
    }

    pub(crate) fn volume(&self) -> i32 {
        self.app_state.player.volume()
    }

    pub(crate) fn balance(&self) -> i32 {
        self.app_state.player.balance()
    }

    pub(crate) fn position(&self) -> i32 {
        self.position_slider_position()
    }

    pub(crate) fn main_time_digits(&self) -> [i32; 5] {
        self.time_digits()
    }

    pub(crate) fn shaded_main_time_text(&self) -> (String, String) {
        self.shaded_time_parts()
    }

    pub(crate) fn shaded_main_position_visible(&self) -> bool {
        self.shaded_position_slider_visible()
    }

    pub(crate) fn shaded_main_position(&self) -> i32 {
        self.shaded_position_slider_position()
    }

    pub(crate) fn main_channels(&self) -> i32 {
        self.render_state().channels
    }

    pub(crate) fn set_preference_output_device(&mut self, device: Option<String>) {
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend
                .borrow_mut()
                .rebuild_output_sink("autoaudiosink", device.as_deref())
            {
                eprintln!("xmms-rs: failed to switch output device: {err}");
            }
        }
        self.sync_equalizer_to_backend();
        self.app_state.config.output_device = device;
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_output_device(&self) -> Option<&str> {
        self.app_state.config.output_device.as_deref()
    }

    pub(crate) fn set_preference_volume(&mut self, volume: i32) {
        let volume = volume.clamp(0, 100);
        self.app_state.config.volume = volume;
        self.app_state.player.set_volume(volume);
        if let Some(backend) = &self.playback_backend {
            backend.borrow().set_volume_percent(volume);
        }
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_balance(&mut self, balance: i32) {
        let balance = balance.clamp(-100, 100);
        self.app_state.config.balance = balance;
        self.app_state.player.set_balance(balance);
        if let Some(backend) = &self.playback_backend {
            backend.borrow().set_balance_percent(balance);
        }
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_scale_factor(&mut self, scale: f64) {
        let scale = scale.clamp(1.0, 5.0);
        self.app_state.config.scale_factor = scale;
        self.app_state.config.doublesize = scale > 1.0;
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_repeat(&mut self, enabled: bool) {
        self.app_state.config.repeat = enabled;
        self.app_state.playlist.set_repeat(enabled);
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_shuffle(&mut self, enabled: bool) {
        self.app_state.config.shuffle = enabled;
        self.app_state.playlist.set_shuffle(enabled);
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_no_playlist_advance(&mut self, enabled: bool) {
        self.app_state.config.no_playlist_advance = enabled;
        self.app_state.playlist.set_no_advance(enabled);
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_no_playlist_advance(&self) -> bool {
        self.app_state.playlist.no_advance()
    }

    pub(crate) fn set_preference_pause_between_songs(&mut self, enabled: bool) {
        self.app_state.config.pause_between_songs = enabled;
        if !enabled {
            self.playback_transition = PlaybackTransitionState::stop_playback();
        }
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_pause_between_songs(&self) -> bool {
        self.app_state.config.pause_between_songs
    }

    pub(crate) fn set_preference_stop_with_fadeout(&mut self, enabled: bool) {
        self.app_state.config.stop_with_fadeout = enabled;
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_stop_with_fadeout(&self) -> bool {
        self.app_state.config.stop_with_fadeout
    }

    pub(crate) fn set_preference_pause_between_songs_time(&mut self, seconds: i32) {
        self.app_state.config.pause_between_songs_time = seconds.clamp(0, 1000);
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_pause_between_songs_time(&self) -> i32 {
        self.app_state.config.pause_between_songs_time
    }

    pub(crate) fn set_preference_mouse_wheel_change(&mut self, percent: i32) {
        self.app_state.config.mouse_wheel_change = percent.clamp(1, 100);
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_mouse_wheel_change(&self) -> i32 {
        self.app_state.config.mouse_wheel_change
    }

    pub(crate) fn set_preference_timer_remaining(&mut self, enabled: bool) {
        self.app_state.config.timer_mode = if enabled {
            TimerMode::Remaining
        } else {
            TimerMode::Elapsed
        };
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_timer_remaining(&self) -> bool {
        self.app_state.config.timer_mode == TimerMode::Remaining
    }

    pub(crate) fn set_preference_playlist_docked(&mut self, docked: bool) {
        self.playlist_ui.panel.detached = !docked;
        self.sync_panel_config_from_placement();
        if docked {
            self.playlist_ui.width = PLAYLIST_MIN_WIDTH;
            self.clamp_playlist_scroll_offset();
        }
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_equalizer_docked(&mut self, docked: bool) {
        self.equalizer.panel.detached = !docked;
        self.sync_panel_config_from_placement();
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_convert_underscore(&mut self, enabled: bool) {
        self.app_state.config.convert_underscore = enabled;
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_convert_underscore(&self) -> bool {
        self.app_state.config.convert_underscore
    }

    pub(crate) fn set_preference_convert_twenty(&mut self, enabled: bool) {
        self.app_state.config.convert_twenty = enabled;
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_convert_twenty(&self) -> bool {
        self.app_state.config.convert_twenty
    }

    pub(crate) fn set_preference_show_numbers_in_playlist(&mut self, enabled: bool) {
        self.app_state.config.show_numbers_in_pl = enabled;
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_show_numbers_in_playlist(&self) -> bool {
        self.app_state.config.show_numbers_in_pl
    }

    pub(crate) fn set_preference_vim_playlist_navigation(&mut self, enabled: bool) {
        self.app_state.config.vim_playlist_navigation = enabled;
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_vim_playlist_navigation(&self) -> bool {
        self.app_state.config.vim_playlist_navigation
    }

    pub(crate) fn set_preference_playlist_font(&mut self, font: &str) {
        self.app_state.config.playlist_font = if font.trim().is_empty() {
            "Helvetica".to_string()
        } else {
            font.trim().to_string()
        };
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_playlist_font(&self) -> &str {
        &self.app_state.config.playlist_font
    }

    pub(crate) fn set_preference_mainwin_font(&mut self, font: &str) {
        self.app_state.config.mainwin_font = if font.trim().is_empty() {
            "Skin bitmap font".to_string()
        } else {
            font.trim().to_string()
        };
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_mainwin_font(&self) -> &str {
        &self.app_state.config.mainwin_font
    }

    pub(crate) fn set_preference_title_format(&mut self, format: &str) {
        self.app_state.config.title_format = if format.trim().is_empty() {
            "%p - %t".to_string()
        } else {
            format.trim().to_string()
        };
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_title_format(&self) -> &str {
        &self.app_state.config.title_format
    }

    pub(crate) fn set_preference_podcast_cache_ttl_days(&mut self, days: i32) {
        self.app_state.config.podcast_cache_ttl_days = if days < 1 { 60 } else { days };
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_podcast_cache_ttl_days(&self) -> i32 {
        self.app_state.config.podcast_cache_ttl_days
    }

    pub(crate) fn set_preference_podcast_refresh_interval_minutes(&mut self, minutes: i32) {
        self.app_state.config.podcast_refresh_interval_minutes =
            if minutes < 1 { 60 } else { minutes };
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_podcast_refresh_interval_minutes(&self) -> i32 {
        self.app_state.config.podcast_refresh_interval_minutes
    }

    pub(crate) fn set_visualization_mode(&mut self, mode: VisMode) {
        self.app_state.config.vis_mode = mode;
        self.apply_visualization_preferences();
        self.mark_preferences_saved();
    }

    pub(crate) fn visualization_mode(&self) -> VisMode {
        self.visualization.mode()
    }

    pub(crate) fn set_visualization_analyzer_style(&mut self, style: VisAnalyzerStyle) {
        self.app_state.config.vis_analyzer_style = style;
        self.apply_visualization_preferences();
        self.mark_preferences_saved();
    }

    pub(crate) fn visualization_analyzer_style(&self) -> VisAnalyzerStyle {
        self.visualization.analyzer_style()
    }

    pub(crate) fn set_visualization_analyzer_mode(&mut self, mode: VisAnalyzerMode) {
        self.app_state.config.vis_analyzer_mode = mode;
        self.apply_visualization_preferences();
        self.mark_preferences_saved();
    }

    pub(crate) fn visualization_analyzer_mode(&self) -> VisAnalyzerMode {
        self.visualization.analyzer_mode()
    }

    pub(crate) fn set_visualization_scope_mode(&mut self, mode: VisScopeMode) {
        self.app_state.config.vis_scope_mode = mode;
        self.apply_visualization_preferences();
        self.mark_preferences_saved();
    }

    pub(crate) fn visualization_scope_mode(&self) -> VisScopeMode {
        self.visualization.scope_mode()
    }

    pub(crate) fn set_visualization_peaks_enabled(&mut self, enabled: bool) {
        self.app_state.config.vis_peaks_enabled = enabled;
        self.apply_visualization_preferences();
        self.mark_preferences_saved();
    }

    pub(crate) fn visualization_peaks_enabled(&self) -> bool {
        self.visualization.peaks_enabled()
    }

    pub(crate) fn visualization_analyzer_falloff(&self) -> VisFalloffSpeed {
        self.app_state.config.vis_analyzer_falloff
    }

    pub(crate) fn visualization_peaks_falloff(&self) -> VisFalloffSpeed {
        self.app_state.config.vis_peaks_falloff
    }

    pub(crate) fn set_visualization_falloff(
        &mut self,
        analyzer: VisFalloffSpeed,
        peaks: VisFalloffSpeed,
    ) {
        self.app_state.config.vis_analyzer_falloff = analyzer;
        self.app_state.config.vis_peaks_falloff = peaks;
        self.apply_visualization_preferences();
        self.mark_preferences_saved();
    }

    pub(crate) fn set_visualization_vu_mode(&mut self, mode: VisVuMode) {
        self.app_state.config.vis_vu_mode = mode;
        self.mark_preferences_saved();
    }

    pub(crate) fn visualization_vu_mode(&self) -> VisVuMode {
        self.app_state.config.vis_vu_mode
    }

    pub(crate) fn set_visualization_refresh_divisor(&mut self, divisor: i32) {
        self.app_state.config.vis_refresh_divisor = divisor.clamp(1, 8);
        self.mark_preferences_saved();
    }

    pub(crate) fn visualization_refresh_divisor(&self) -> i32 {
        self.app_state.config.vis_refresh_divisor.clamp(1, 8)
    }

    pub(crate) fn visualization_render_state(&self) -> VisualizationRenderState {
        self.make_visualization_render_state()
    }

    fn apply_visualization_preferences(&mut self) {
        self.visualization.set_mode(self.app_state.config.vis_mode);
        self.visualization
            .set_analyzer_mode(self.app_state.config.vis_analyzer_mode);
        self.visualization
            .set_analyzer_style(self.app_state.config.vis_analyzer_style);
        self.visualization
            .set_scope_mode(self.app_state.config.vis_scope_mode);
        self.visualization
            .set_peaks_enabled(self.app_state.config.vis_peaks_enabled);
        self.visualization.set_falloff(
            self.app_state.config.vis_analyzer_falloff,
            self.app_state.config.vis_peaks_falloff,
        );
    }

    fn set_playback_position_ms(&mut self, position_ms: i64) {
        self.ensure_current_playlist_position_for_seek();
        let duration = self.current_duration_ms();
        self.playback_position_ms =
            if let Some(duration) = duration.filter(|duration| *duration > 0) {
                position_ms.clamp(0, duration)
            } else {
                position_ms.max(0)
            };
        self.position_position = self.position_slider_position();
        if self.app_state.player.state() == PlayerState::Stopped {
            self.playback_transition =
                PlaybackTransitionState::stopped_at_or_idle(self.playback_position_ms);
            return;
        }
        if self.playback_transition.pending_backend_seek_ms().is_some() {
            self.playback_transition =
                PlaybackTransitionState::request_backend_seek(self.playback_position_ms);
            return;
        }
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().seek_to_ms(self.playback_position_ms) {
                eprintln!("xmms-rs: failed to seek playback: {err}");
            }
        }
    }

    pub(crate) fn update_timer_tick(&mut self, elapsed_ms: u32) -> bool {
        let duration_changed = self.poll_duration_index_results();
        self.poll_playback_backend();
        let fading = self.update_stop_fade(elapsed_ms);
        let eof_waiting = self.update_pending_eof_advance(elapsed_ms);
        if self.app_state.player.state() != PlayerState::Playing {
            self.visualization_tick_counter = 0;
            return duration_changed || fading || eof_waiting;
        }

        if self.playback_backend.is_none() {
            self.playback_position_ms = self
                .playback_position_ms
                .saturating_add(i64::from(elapsed_ms));
        }
        self.position_position = self.position_slider_position();
        self.visualization_tick_counter += 1;
        if self.visualization_tick_counter >= self.visualization_refresh_divisor() {
            self.visualization_tick_counter = 0;
            let data = self
                .app_state
                .player
                .visualization_data_valid()
                .then_some(self.app_state.player.visualization_data() as &[f32]);
            self.visualization.tick(data);
        }
        true
    }

    fn update_stop_fade(&mut self, elapsed_ms: u32) -> bool {
        let Some((next_transition, volume)) = self.playback_transition.tick_fadeout(elapsed_ms)
        else {
            return false;
        };
        if next_transition
            .fadeout()
            .is_some_and(|(remaining_ms, _)| remaining_ms == 0)
        {
            let restore_volume = self.app_state.config.volume;
            self.stop_playback();
            self.set_runtime_volume(restore_volume);
            return true;
        }
        self.playback_transition = next_transition;
        self.set_runtime_volume(volume);
        true
    }

    fn update_pending_eof_advance(&mut self, elapsed_ms: u32) -> bool {
        let Some((next_transition, should_advance)) =
            self.playback_transition.tick_eof_pause(elapsed_ms)
        else {
            return false;
        };
        self.playback_transition = next_transition;
        if !should_advance {
            return true;
        }
        self.advance_playlist_after_eof();
        true
    }

    fn poll_playback_backend(&mut self) {
        let Some(backend) = self.playback_backend.as_ref().map(Rc::clone) else {
            return;
        };
        let mut applied_pending_seek = false;
        match backend.borrow().poll_bus_events() {
            Ok(events) => {
                let mut end_of_stream = false;
                let mut backend_ready = false;
                for event in events {
                    if matches!(event, PlaybackEvent::EndOfStream) {
                        end_of_stream = true;
                    }
                    if matches!(
                        event,
                        PlaybackEvent::AsyncDone | PlaybackEvent::DurationChanged(_)
                    ) {
                        backend_ready = true;
                    }
                    self.app_state.player.apply_playback_event(&event);
                }
                if backend_ready {
                    applied_pending_seek |= self.apply_pending_backend_seek(&backend, false);
                }
                if end_of_stream {
                    self.playlist_eof_reached();
                }
            }
            Err(err) => eprintln!("xmms-rs: failed to poll playback backend: {err}"),
        }
        let (stream_info, duration_ms) = {
            let backend = backend.borrow();
            (backend.audio_stream_info(), backend.duration_ms())
        };
        self.app_state
            .player
            .set_stream_info(None, stream_info.frequency, stream_info.channels);
        if let Some(duration_ms) = duration_ms {
            self.app_state.player.apply_playback_event(
                &crate::player::PlaybackEvent::DurationChanged(Some(duration_ms)),
            );
            applied_pending_seek |= self.apply_pending_backend_seek(&backend, true);
        }
        if self.should_sync_backend_position(applied_pending_seek) {
            let position_ms = { backend.borrow().position_ms() };
            if let Some(position_ms) = position_ms {
                self.playback_position_ms = position_ms.max(0);
                self.position_position = self.position_slider_position();
            }
        }
    }

    fn should_sync_backend_position(&self, applied_pending_seek: bool) -> bool {
        !applied_pending_seek && self.playback_transition.eof_pause_remaining_ms().is_none()
    }

    fn apply_pending_backend_seek(
        &mut self,
        backend: &Rc<RefCell<GStreamerBackend>>,
        log_failure: bool,
    ) -> bool {
        let Some(position_ms) = self.playback_transition.pending_backend_seek_ms() else {
            return false;
        };
        match backend.borrow().seek_to_ms(position_ms) {
            Ok(()) => {
                self.playback_transition = PlaybackTransitionState::Idle;
                true
            }
            Err(err) => {
                if log_failure {
                    eprintln!("xmms-rs: failed to seek playback: {err}");
                    self.playback_transition = PlaybackTransitionState::Idle;
                }
                false
            }
        }
    }

    pub(crate) fn playlist_eof_reached(&mut self) {
        self.position_position = 0;
        if self.app_state.config.pause_between_songs
            && self.app_state.config.pause_between_songs_time > 0
        {
            self.playback_transition = PlaybackTransitionState::wait_between_songs(
                i64::from(self.app_state.config.pause_between_songs_time) * 1_000,
            );
            self.playback_position_ms = 0;
            return;
        }
        self.advance_playlist_after_eof();
    }

    fn advance_playlist_after_eof(&mut self) {
        self.position_position = 0;
        if self.app_state.playlist.eof_reached() {
            self.start_current_playlist_playback_from_beginning();
        } else {
            self.stop_playback();
        }
    }

    pub(crate) fn click(&mut self, x: i32, y: i32) -> UiAction {
        self.press(x, y);
        self.release(x, y)
    }

    pub(crate) fn press(&mut self, x: i32, y: i32) {
        let Some(control) = self.hit_test(x, y) else {
            self.main_pointer = MainPointer::Idle;
            return;
        };

        self.main_pointer = if let MainControl::Slider(slider) = control {
            self.main_keyboard_slider = Some(slider);
            MainPointer::DraggingSlider {
                slider,
                offset: self.begin_slider_drag(slider, x),
            }
        } else {
            MainPointer::PressedButton {
                control,
                inside: true,
            }
        };
    }

    pub(crate) fn motion(&mut self, x: i32, y: i32) -> bool {
        match self.main_pointer {
            MainPointer::Idle => false,
            MainPointer::PressedButton { control, inside } => {
                let next_inside = self.control_rect(control).contains(x, y);
                let changed = inside != next_inside;
                self.main_pointer = MainPointer::PressedButton {
                    control,
                    inside: next_inside,
                };
                changed
            }
            MainPointer::DraggingSlider { slider, offset } => {
                self.set_slider_position(slider, x - self.slider_rect(slider).x - offset)
            }
        }
    }

    pub(crate) fn release(&mut self, x: i32, y: i32) -> UiAction {
        match std::mem::take(&mut self.main_pointer) {
            MainPointer::Idle => UiAction::None,
            MainPointer::PressedButton { control, inside } => {
                let activated = inside && self.control_rect(control).contains(x, y);
                match control {
                    MainControl::Push(button) if activated => self.activate_push(button),
                    MainControl::Toggle(toggle) if activated => {
                        self.activate_toggle(toggle);
                        UiAction::None
                    }
                    _ => UiAction::None,
                }
            }
            MainPointer::DraggingSlider { slider, offset } => {
                self.set_slider_position(slider, x - self.slider_rect(slider).x - offset);
                UiAction::None
            }
        }
    }

    pub(crate) fn scroll_main(&mut self, x: i32, y: i32, dy: f64) -> bool {
        if let Some((kind, panel_x, panel_y)) = self.docked_panel_at(x, y) {
            return match kind {
                PanelKind::Equalizer => self.equalizer_scroll(panel_x, panel_y, dy),
                PanelKind::Playlist => self.playlist_scroll(dy),
            };
        }
        if let Some(MainControl::Slider(slider)) = self.hit_test(x, y) {
            return self.scroll_slider(slider, dy);
        }
        self.scroll_volume(dy)
    }

    fn scroll_slider(&mut self, slider: MainSlider, dy: f64) -> bool {
        match slider {
            MainSlider::Volume => self.scroll_volume(dy),
            MainSlider::Balance => self.scroll_balance(dy),
            MainSlider::Position => self.scroll_position_slider(dy),
        }
    }

    pub(crate) fn adjust_main_seek(&mut self, diff: i32) -> bool {
        self.scroll_position_slider(f64::from(diff))
    }

    fn scroll_volume(&mut self, dy: f64) -> bool {
        let step = self.app_state.config.mouse_wheel_change.clamp(1, 100);
        let diff = if dy < 0.0 {
            step
        } else if dy > 0.0 {
            -step
        } else {
            return false;
        };
        self.adjust_volume_by(diff)
    }

    fn adjust_volume_by(&mut self, diff: i32) -> bool {
        let volume = (self.app_state.player.volume() + diff).clamp(0, 100);
        if volume == self.app_state.player.volume() {
            return false;
        }
        self.app_state.player.set_volume(volume);
        self.app_state.config.volume = volume;
        if let Some(backend) = &self.playback_backend {
            backend.borrow().set_volume_percent(volume);
        }
        true
    }

    fn scroll_balance(&mut self, dy: f64) -> bool {
        let step = self.app_state.config.mouse_wheel_change.clamp(1, 100);
        let diff = if dy < 0.0 {
            step
        } else if dy > 0.0 {
            -step
        } else {
            return false;
        };
        self.adjust_balance_by(diff)
    }

    fn adjust_balance_by(&mut self, diff: i32) -> bool {
        let balance = (self.app_state.player.balance() + diff).clamp(-100, 100);
        if balance == self.app_state.player.balance() {
            return false;
        }
        self.app_state.player.set_balance(balance);
        self.app_state.config.balance = balance;
        if let Some(backend) = &self.playback_backend {
            backend.borrow().set_balance_percent(balance);
        }
        true
    }

    fn scroll_position_slider(&mut self, dy: f64) -> bool {
        self.ensure_current_playlist_position_for_seek();
        let Some(duration_ms) = self.current_duration_ms().filter(|duration| *duration > 0) else {
            return false;
        };
        let step_ms = (duration_ms / 100).max(1_000);
        let old_position = self.playback_position_ms;
        let position_ms = if dy < 0.0 {
            old_position - step_ms
        } else if dy > 0.0 {
            old_position + step_ms
        } else {
            return false;
        };
        self.set_playback_position_ms(position_ms);
        self.playback_position_ms != old_position
    }

    fn hit_test(&self, x: i32, y: i32) -> Option<MainControl> {
        let mut controls = vec![
            MainControl::Push(MainPushButton::Close),
            MainControl::Push(MainPushButton::Shade),
            MainControl::Push(MainPushButton::Minimize),
            MainControl::Push(MainPushButton::Menu),
        ];
        if !self.shaded {
            controls.extend([
                MainControl::Toggle(MainToggleButton::Playlist),
                MainControl::Toggle(MainToggleButton::Equalizer),
                MainControl::Toggle(MainToggleButton::Repeat),
                MainControl::Toggle(MainToggleButton::Shuffle),
                MainControl::Slider(MainSlider::Position),
                MainControl::Slider(MainSlider::Balance),
                MainControl::Slider(MainSlider::Volume),
                MainControl::Push(MainPushButton::Eject),
                MainControl::Push(MainPushButton::Next),
                MainControl::Push(MainPushButton::Stop),
                MainControl::Push(MainPushButton::Pause),
                MainControl::Push(MainPushButton::Play),
                MainControl::Push(MainPushButton::Previous),
            ]);
        } else {
            controls.extend([
                MainControl::Slider(MainSlider::Position),
                MainControl::Push(MainPushButton::Eject),
                MainControl::Push(MainPushButton::Next),
                MainControl::Push(MainPushButton::Stop),
                MainControl::Push(MainPushButton::Pause),
                MainControl::Push(MainPushButton::Play),
                MainControl::Push(MainPushButton::Previous),
            ]);
        }

        controls
            .into_iter()
            .filter(|control| match control {
                MainControl::Slider(MainSlider::Position) if self.shaded => {
                    self.shaded_position_slider_visible()
                }
                _ => true,
            })
            .find(|control| self.control_rect(*control).contains(x, y))
    }

    pub(crate) fn activate_push(&mut self, button: MainPushButton) -> UiAction {
        match button {
            MainPushButton::Close => UiAction::Quit,
            MainPushButton::Minimize => UiAction::Minimize,
            MainPushButton::Menu => {
                self.menu_visible = true;
                UiAction::ShowMenu
            }
            MainPushButton::Shade => {
                self.shaded = !self.shaded;
                UiAction::Resize
            }
            MainPushButton::Play => {
                self.handle_playback_control_event(PlaybackControlEvent::Play);
                UiAction::None
            }
            MainPushButton::Pause => {
                self.handle_playback_control_event(PlaybackControlEvent::PauseToggle);
                UiAction::None
            }
            MainPushButton::Stop => {
                self.handle_playback_control_event(PlaybackControlEvent::Stop);
                UiAction::None
            }
            MainPushButton::Previous => {
                self.handle_playback_control_event(PlaybackControlEvent::Previous);
                UiAction::None
            }
            MainPushButton::Next => {
                self.handle_playback_control_event(PlaybackControlEvent::Next);
                UiAction::None
            }
            MainPushButton::Eject => UiAction::OpenFileDialog,
        }
    }

    pub(crate) fn activate_toggle(&mut self, toggle: MainToggleButton) {
        match toggle {
            MainToggleButton::Shuffle => {
                let selected = !self.app_state.playlist.shuffle();
                self.app_state.playlist.set_shuffle(selected);
                self.app_state.config.shuffle = selected;
            }
            MainToggleButton::Repeat => {
                let selected = !self.app_state.playlist.repeat();
                self.app_state.playlist.set_repeat(selected);
                self.app_state.config.repeat = selected;
            }
            MainToggleButton::Equalizer => {
                self.equalizer.panel.visible = !self.equalizer.panel.visible;
                self.sync_panel_config_from_placement();
            }
            MainToggleButton::Playlist => {
                self.playlist_ui.panel.visible = !self.playlist_ui.panel.visible;
                self.sync_panel_config_from_placement();
            }
        }
    }

    fn begin_slider_drag(&mut self, slider: MainSlider, x: i32) -> i32 {
        let rect = self.slider_rect(slider);
        let knob_width = self.slider_knob_width(slider);
        let position = self.slider_position(slider);
        let knob_x = rect.x + position;
        if x >= knob_x && x < knob_x + knob_width {
            x - knob_x
        } else {
            let offset = knob_width / 2;
            self.set_slider_position(slider, x - rect.x - offset);
            offset
        }
    }

    fn set_slider_position(&mut self, slider: MainSlider, position: i32) -> bool {
        if slider == MainSlider::Position {
            self.ensure_current_playlist_position_for_seek();
        }
        let position = position.clamp(self.slider_min(slider), self.slider_max(slider));
        let old_position = self.slider_position(slider);
        if old_position == position {
            return false;
        }

        match slider {
            MainSlider::Volume => {
                let volume = position_to_volume(position);
                self.app_state.player.set_volume(volume);
                if let Some(backend) = &self.playback_backend {
                    backend.borrow().set_volume_percent(volume);
                }
            }
            MainSlider::Balance => {
                let balance = position_to_balance(position);
                self.app_state.player.set_balance(balance);
                if let Some(backend) = &self.playback_backend {
                    backend.borrow().set_balance_percent(balance);
                }
            }
            MainSlider::Position => {
                if let Some(duration_ms) =
                    self.current_duration_ms().filter(|duration| *duration > 0)
                {
                    let position_ms = if self.shaded {
                        (duration_ms * i64::from(position - 1)) / 12
                    } else {
                        let position_slider = main_slider_layout(MainSlider::Position, false);
                        (duration_ms * i64::from(position)) / i64::from(position_slider.max)
                    };
                    self.set_playback_position_ms(position_ms);
                }
            }
        }
        self.app_state.sync_config_from_runtime();
        true
    }

    fn slider_position(&self, slider: MainSlider) -> i32 {
        match slider {
            MainSlider::Volume => volume_to_position(self.app_state.player.volume()),
            MainSlider::Balance => balance_to_position(self.app_state.player.balance()),
            MainSlider::Position if self.shaded => self.shaded_position_slider_position(),
            MainSlider::Position => self.position_slider_position(),
        }
    }

    fn slider_min(&self, slider: MainSlider) -> i32 {
        main_slider_layout(slider, self.shaded).min
    }

    fn slider_max(&self, slider: MainSlider) -> i32 {
        main_slider_layout(slider, self.shaded).max
    }

    fn slider_knob_width(&self, slider: MainSlider) -> i32 {
        main_slider_layout(slider, self.shaded).knob_size.width
    }

    fn pressed_push(&self) -> Option<MainPushButton> {
        match self.main_pointer.pressed_control() {
            Some(MainControl::Push(button)) => Some(button),
            _ => None,
        }
    }

    fn pressed_toggle(&self) -> Option<MainToggleButton> {
        match self.main_pointer.pressed_control() {
            Some(MainControl::Toggle(toggle)) => Some(toggle),
            _ => None,
        }
    }

    fn pressed_slider(&self) -> Option<MainSlider> {
        self.main_pointer.pressed_slider()
    }

    fn control_rect(&self, control: MainControl) -> ControlRect {
        match control {
            MainControl::Push(button) => main_push_button_rect(button, self.shaded),
            MainControl::Toggle(toggle) => main_toggle_button_rect(toggle),
            MainControl::Slider(slider) => self.slider_rect(slider),
        }
    }

    fn slider_rect(&self, slider: MainSlider) -> ControlRect {
        main_slider_layout(slider, self.shaded).rect
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelVisibility {
    pub(crate) equalizer: bool,
    pub(crate) playlist: bool,
}

type ControlRect = SkinRect;

fn panel_layout_kind(kind: PanelKind) -> LayoutPanel {
    match kind {
        PanelKind::Equalizer => LayoutPanel::Equalizer,
        PanelKind::Playlist => LayoutPanel::Playlist,
    }
}

fn playlist_menu_at(x: i32, y: i32, width: i32, height: i32) -> Option<PlaylistMenuKind> {
    playlist_menu_button_at(x, y, width, height).map(playlist_menu_from_button)
}

fn playlist_menu_from_button(button: PlaylistMenuButton) -> PlaylistMenuKind {
    match button {
        PlaylistMenuButton::Add => PlaylistMenuKind::Add,
        PlaylistMenuButton::Remove => PlaylistMenuKind::Remove,
        PlaylistMenuButton::Select => PlaylistMenuKind::Select,
        PlaylistMenuButton::Misc => PlaylistMenuKind::Misc,
        PlaylistMenuButton::List => PlaylistMenuKind::List,
    }
}

fn playlist_menu_button_from_kind(menu: PlaylistMenuKind) -> PlaylistMenuButton {
    match menu {
        PlaylistMenuKind::Add => PlaylistMenuButton::Add,
        PlaylistMenuKind::Remove => PlaylistMenuButton::Remove,
        PlaylistMenuKind::Select => PlaylistMenuButton::Select,
        PlaylistMenuKind::Misc => PlaylistMenuButton::Misc,
        PlaylistMenuKind::List => PlaylistMenuButton::List,
    }
}

fn playlist_menu_rect(menu: PlaylistMenuKind, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let rect = playlist_menu_popup_rect(playlist_menu_button_from_kind(menu), width, height);
    (rect.x, rect.y, rect.width, rect.height)
}

fn volume_to_position(volume: i32) -> i32 {
    ((volume.clamp(0, 100) * 51 + 50) / 100).clamp(0, 51)
}

fn position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 51) * 100) as f64 / 51.0) as i32
}

fn volume_to_eq_shaded_position(volume: i32) -> i32 {
    ((volume.clamp(0, 100) * 94 + 50) / 100).clamp(0, 94)
}

fn balance_to_position(balance: i32) -> i32 {
    (12 + (balance.clamp(-100, 100) * 12) / 100).clamp(0, 24)
}

fn position_to_balance(position: i32) -> i32 {
    (((position.clamp(0, 24) - 12) * 100) as f64 / 12.0) as i32
}

fn balance_to_eq_shaded_position(balance: i32) -> i32 {
    (19 + (balance.clamp(-100, 100) * 19) / 100).clamp(0, 39)
}

fn format_duration(milliseconds: i64) -> String {
    let seconds = (milliseconds.max(0) / 1000) as i32;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn format_playlist_footer_duration(milliseconds: i64, more: bool) -> String {
    if milliseconds <= 0 && more {
        return "?".to_string();
    }

    let seconds = milliseconds.max(0) / 1000;
    if seconds > 3600 {
        format!(
            "{}:{:02}:{:02}{}",
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60,
            if more { "+" } else { "" }
        )
    } else {
        format!(
            "{}:{:02}{}",
            seconds / 60,
            seconds % 60,
            if more { "+" } else { "" }
        )
    }
}

fn format_title_for_preferences(
    format: &str,
    filename: &str,
    title: &str,
    config: &Config,
) -> String {
    let title = title.trim();
    let fallback_title =
        if title.is_empty() || title == crate::playlist::format_title(filename, None) {
            filename_title(filename, config)
        } else {
            normalize_title_text(title, config)
        };
    let (artist, track_title) = split_artist_title(&fallback_title);
    let file_title = filename_title(filename, config);
    let format = if format.trim().is_empty() {
        "%p - %t"
    } else {
        format.trim()
    };

    let mut output = String::new();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('p') => output.push_str(artist.unwrap_or("")),
            Some('t') => output.push_str(track_title),
            Some('f') => output.push_str(&file_title),
            Some('a') | Some('g') => {}
            Some('%') => output.push('%'),
            Some(other) => {
                output.push('%');
                output.push(other);
            }
            None => output.push('%'),
        }
    }

    cleanup_formatted_title(&output).unwrap_or(fallback_title)
}

fn split_artist_title(title: &str) -> (Option<&str>, &str) {
    title
        .split_once(" - ")
        .map(|(artist, track)| (Some(artist.trim()), track.trim()))
        .unwrap_or((None, title.trim()))
}

fn filename_title(filename: &str, config: &Config) -> String {
    let without_query = filename.split(['?', '#']).next().unwrap_or(filename);
    let normalized = normalize_title_text(without_query, config);
    let path = normalized
        .strip_prefix("file://")
        .unwrap_or(normalized.as_str())
        .trim_end_matches('/');
    let basename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let stem = basename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(basename);
    stem.to_string()
}

fn normalize_title_text(text: &str, config: &Config) -> String {
    let mut normalized = text.to_string();
    if config.convert_twenty {
        normalized = normalized.replace("%20", " ");
    }
    if config.convert_underscore {
        normalized = normalized.replace('_', " ");
    }
    normalized
}

fn cleanup_formatted_title(text: &str) -> Option<String> {
    let mut cleaned = text.trim().to_string();
    for prefix in ["- ", ":", "/", "|"] {
        cleaned = cleaned.trim_start_matches(prefix).trim_start().to_string();
    }
    for suffix in [" -", ":", "/", "|"] {
        cleaned = cleaned.trim_end_matches(suffix).trim_end().to_string();
    }
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn ellipsize_chars(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_len {
        return text.to_string();
    }
    if max_len > 3 {
        let mut truncated: String = text.chars().take(max_len - 3).collect();
        truncated.push_str("...");
        truncated
    } else {
        text.chars().take(max_len).collect()
    }
}

fn eq_shaded_position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 94) * 100 + 47) / 94).clamp(0, 100)
}

fn eq_shaded_position_to_balance(position: i32) -> i32 {
    let position = position.clamp(0, 38);
    (((position - 19) * 100 + if position >= 19 { 9 } else { -9 }) / 19).clamp(-100, 100)
}

fn eq_slider_position_to_pixel(position: i32) -> i32 {
    let pixel = position.clamp(0, 100) / 2;
    if (24..=26).contains(&pixel) {
        25
    } else {
        pixel
    }
}

fn eq_slider_pixel_to_position(pixel: i32) -> i32 {
    let pixel = pixel.clamp(0, 50);
    if (24..=26).contains(&pixel) {
        50
    } else {
        pixel * 2
    }
}

fn event_to_base_coords(
    area: &gtk::DrawingArea,
    state: &MainWindowUiState,
    x: f64,
    y: f64,
) -> (i32, i32) {
    let width = area.allocated_width().max(1) as f64;
    let height = area.allocated_height().max(1) as f64;
    let (base_width, base_height) = state.docked_panel_size();
    scale_event_coords(width, height, base_width, base_height, x, y)
}

fn scale_event_coords(
    width: f64,
    height: f64,
    base_width: i32,
    base_height: i32,
    x: f64,
    y: f64,
) -> (i32, i32) {
    (
        (x / (width / f64::from(base_width))) as i32,
        (y / (height / f64::from(base_height))) as i32,
    )
}

fn apply_ui_action(
    action: UiAction,
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    menu_popover: &gtk::Popover,
    state: &Rc<RefCell<MainWindowUiState>>,
) {
    match action {
        UiAction::None => {}
        UiAction::Quit => app.quit(),
        UiAction::Minimize => window.minimize(),
        UiAction::Resize => {
            let (height, scale) = {
                let state = state.borrow();
                (
                    if state.shaded {
                        MAIN_TITLEBAR_HEIGHT
                    } else {
                        MAIN_WINDOW_HEIGHT
                    },
                    state.scale_factor(),
                )
            };
            drawing_area.set_content_height(scale_dim(height, scale));
            window.set_default_size(
                scale_dim(MAIN_WINDOW_WIDTH, scale),
                scale_dim(height, scale),
            );
        }
        UiAction::ShowMenu => {
            show_main_menu(menu_popover, drawing_area, &state.borrow());
        }
        UiAction::OpenFileDialog => {
            state.borrow_mut().set_file_dialog_visible(true);
            show_open_file_dialog(window, Rc::clone(state));
        }
    }
}

fn show_open_file_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Open Files"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    dialog.set_select_multiple(true);
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_file_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uris = files_from_list_model(dialog.files());
                state.accept_opened_uris(uris);
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_open_directory_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Open Directory"),
        Some(parent),
        gtk::FileChooserAction::SelectFolder,
        Some("Open"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_directory_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uri = dialog.file().map(|file| file.uri().to_string());
                state.accept_opened_uris(uri);
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_add_file_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Add Files"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    dialog.set_select_multiple(true);
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_file_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uris = files_from_list_model(dialog.files());
                state.accept_dropped_uris(uris, false, false);
            }
        }
        playlist_area.queue_draw();
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_add_directory_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Add Directory"),
        Some(parent),
        gtk::FileChooserAction::SelectFolder,
        Some("Open"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_directory_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uri = dialog.file().map(|file| file.uri().to_string());
                state.accept_dropped_uris(uri, false, false);
            }
        }
        playlist_area.queue_draw();
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_load_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Load Playlist"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_playlist_load_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    if let Err(err) = state.load_playlist_file(&path) {
                        eprintln!("xmms-rs: failed to load playlist {}: {err}", path.display());
                    }
                }
            }
        }
        playlist_area.queue_draw();
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_save_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Save Playlist"),
        Some(parent),
        gtk::FileChooserAction::Save,
        Some("Save"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_playlist_save_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    if let Err(err) = state.save_playlist_file(&path) {
                        eprintln!("xmms-rs: failed to save playlist {}: {err}", path.display());
                    }
                }
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn files_from_list_model(files: gtk::gio::ListModel) -> Vec<String> {
    (0..files.n_items())
        .filter_map(|idx| files.item(idx))
        .filter_map(|object| object.downcast::<gtk::gio::File>().ok())
        .map(|file| file.uri().to_string())
        .collect()
}

fn show_main_menu(
    menu_popover: &gtk::Popover,
    drawing_area: &gtk::DrawingArea,
    state: &MainWindowUiState,
) {
    let (base_width, base_height) = state.docked_panel_size();
    let rect = main_menu_anchor_rect(
        drawing_area.allocated_width(),
        drawing_area.allocated_height(),
        base_width,
        base_height,
    );
    menu_popover.set_position(gtk::PositionType::Bottom);
    menu_popover.set_pointing_to(Some(&rect));
    menu_popover.popup();
}

fn main_menu_anchor_rect(
    allocated_width: i32,
    allocated_height: i32,
    base_width: i32,
    base_height: i32,
) -> gtk::gdk::Rectangle {
    let scale_x = allocated_width.max(1) as f64 / f64::from(base_width.max(1));
    let scale_y = allocated_height.max(1) as f64 / f64::from(base_height.max(1));
    let rect = main_push_button_rect(MainPushButton::Menu, false);
    gtk::gdk::Rectangle::new(
        (f64::from(rect.x) * scale_x) as i32,
        (f64::from(rect.y) * scale_y) as i32,
        (f64::from(rect.width) * scale_x).max(1.0) as i32,
        (f64::from(rect.height) * scale_y).max(1.0) as i32,
    )
}

fn shortcut_matches(key: gtk::gdk::Key, state: gtk::gdk::ModifierType, accelerator: &str) -> bool {
    let Some((shortcut_key, shortcut_mods)) = gtk::accelerator_parse(accelerator) else {
        return false;
    };
    let relevant_mods = state
        & (gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::SHIFT_MASK
            | gtk::gdk::ModifierType::ALT_MASK);
    key == shortcut_key && relevant_mods == shortcut_mods
}

fn parse_time_ms(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    if let Some((minutes, seconds)) = text.split_once(':') {
        if seconds.contains(':') {
            return None;
        }
        let minutes = minutes.parse::<i64>().ok()?;
        let seconds = seconds.parse::<i64>().ok()?;
        return Some((minutes * 60 + seconds) * 1000);
    }
    Some(text.parse::<i64>().ok()? * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_gtk_for_tests() -> std::sync::MutexGuard<'static, ()> {
        static GTK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = GTK_TEST_LOCK.lock().unwrap();
        gtk::init().expect("GTK should initialize for widget tests");
        guard
    }

    fn find_named_widget<W: IsA<gtk::Widget> + Clone + 'static>(
        root: &impl IsA<gtk::Widget>,
        name: &str,
    ) -> Option<W> {
        let root = root.as_ref();
        if root.widget_name() == name {
            if let Ok(widget) = root.clone().downcast::<W>() {
                return Some(widget);
            }
        }

        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(found) = find_named_widget::<W>(&widget, name) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
    }

    fn collect_label_text(root: &impl IsA<gtk::Widget>, labels: &mut Vec<String>) {
        let root = root.as_ref();
        if let Ok(label) = root.clone().downcast::<gtk::Label>() {
            labels.push(label.text().to_string());
        }

        let mut child = root.first_child();
        while let Some(widget) = child {
            collect_label_text(&widget, labels);
            child = widget.next_sibling();
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn skin_browser_list_labels(list: &gtk::ListBox) -> Vec<String> {
        let mut labels = Vec::new();
        let mut index = 0;
        while let Some(row) = list.row_at_index(index) {
            let label = row
                .child()
                .and_then(|child| child.downcast::<gtk::Label>().ok())
                .expect("skin browser rows should contain labels");
            labels.push(label.text().to_string());
            index += 1;
        }
        labels
    }

    #[test]
    fn mouse_wheel_uses_configured_volume_step() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            volume: 50,
            mouse_wheel_change: 12,
            ..Config::default()
        }));

        assert!(state.scroll_main(1, 1, -1.0));
        assert_eq!(state.volume(), 62);
        assert!(state.scroll_main(1, 1, 1.0));
        assert_eq!(state.volume(), 50);
    }

    #[test]
    fn mouse_wheel_over_main_sliders_changes_each_slider() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            volume: 50,
            mouse_wheel_change: 12,
            ..Config::default()
        }));
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.app_state.playlist.set_position(0);

        assert!(state.scroll_main(108, 58, -1.0));
        assert_eq!(state.volume(), 62);
        assert!(state.scroll_main(178, 58, -1.0));
        assert_eq!(state.app_state.player.balance(), 12);
        assert!(state.scroll_main(140, 73, 1.0));
        let forward_position = state.playback_position_ms;
        assert!(forward_position > 0);
        assert!(state.scroll_main(140, 73, -1.0));
        assert!(state.playback_position_ms < forward_position);
    }

    #[test]
    fn mouse_wheel_seeking_selects_first_entry_before_initial_playback() {
        let mut state = MainWindowUiState::default();
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);

        assert_eq!(state.app_state.playlist.position(), None);
        assert!(state.scroll_main(140, 73, 1.0));

        assert_eq!(state.app_state.playlist.position(), Some(0));
        assert!(state.playback_position_ms > 0);
        assert_eq!(state.app_state.player.state(), PlayerState::Stopped);
    }

    #[test]
    fn mouse_wheel_over_shaded_sliders_changes_each_slider() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            volume: 50,
            mouse_wheel_change: 12,
            ..Config::default()
        }));
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.app_state.playlist.set_position(0);
        state.app_state.player.mark_playing();
        state.shaded = true;
        state.toggle_equalizer_shaded();

        assert!(state.scroll_main(227, 5, 1.0));
        let forward_position = state.playback_position_ms;
        assert!(forward_position > 0);
        assert!(state.scroll_main(227, 5, -1.0));
        assert!(state.playback_position_ms < forward_position);
        assert!(state.equalizer_scroll(62, 5, -1.0));
        assert_eq!(state.volume(), 62);
        assert!(state.equalizer_scroll(165, 5, -1.0));
        assert_eq!(state.app_state.player.balance(), 12);
    }

    #[test]
    fn wheel_event_coordinates_are_scaled_to_skin_space() {
        assert_eq!(
            scale_event_coords(
                f64::from(MAIN_WINDOW_WIDTH * 2),
                f64::from(MAIN_WINDOW_HEIGHT * 2),
                MAIN_WINDOW_WIDTH,
                MAIN_WINDOW_HEIGHT,
                216.0,
                116.0,
            ),
            (108, 58)
        );
        assert_eq!(
            scale_event_coords(
                f64::from(MAIN_WINDOW_WIDTH * 3),
                f64::from(MAIN_WINDOW_HEIGHT * 3),
                MAIN_WINDOW_WIDTH,
                MAIN_WINDOW_HEIGHT,
                534.0,
                174.0,
            ),
            (178, 58)
        );
    }

    #[test]
    fn playlist_mouse_wheel_scrolls_three_rows() {
        let mut state = MainWindowUiState::default();
        for index in 0..20 {
            state
                .app_state
                .playlist
                .add_uri(format!("file:///tmp/song{index}.mp3"));
        }

        assert!(state.playlist_scroll(1.0));
        assert_eq!(state.playlist_scroll_offset(), 3);
        assert!(state.playlist_scroll(-1.0));
        assert_eq!(state.playlist_scroll_offset(), 0);

        state.toggle_playlist_shaded();
        assert!(state.playlist_scroll(1.0));
        assert_eq!(state.playlist_scroll_offset(), 3);
    }

    #[test]
    fn pause_between_songs_delays_eof_advance() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            pause_between_songs: true,
            pause_between_songs_time: 2,
            ..Config::default()
        }));
        state.app_state.playlist.add_uri("file:///tmp/one.mp3");
        state.app_state.playlist.add_uri("file:///tmp/two.mp3");
        state.app_state.playlist.set_position(0);

        state.playlist_eof_reached();
        assert_eq!(state.app_state.playlist.position(), Some(0));
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 2_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);

        assert!(state.update_timer_tick(1_000));
        assert_eq!(state.app_state.playlist.position(), Some(0));
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 1_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);

        assert!(state.update_timer_tick(1_000));
        assert_eq!(state.app_state.playlist.position(), Some(1));
        assert_eq!(state.playback_transition, PlaybackTransitionState::Idle);
    }

    #[test]
    fn play_during_pause_between_songs_wait_starts_from_beginning() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            pause_between_songs: true,
            pause_between_songs_time: 2,
            ..Config::default()
        }));
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.app_state.playlist.set_position(0);

        state.playlist_eof_reached();
        assert!(state.update_timer_tick(1_000));
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 1_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);

        state.start_current_playlist_playback();

        assert_eq!(state.playback_transition, PlaybackTransitionState::Idle);
        assert_eq!(state.playback_position_ms, 0);
        assert_eq!(state.playback_position_ms, 0);
        assert_eq!(state.app_state.player.state(), PlayerState::Playing);
    }

    #[test]
    fn eof_pause_blocks_stale_backend_position_sync() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            pause_between_songs: true,
            pause_between_songs_time: 2,
            ..Config::default()
        }));
        state.app_state.playlist.add_uri("file:///tmp/one.mp3");
        state.app_state.playlist.add_uri("file:///tmp/two.mp3");
        state.app_state.playlist.set_position(0);

        assert!(state.should_sync_backend_position(false));

        state.playlist_eof_reached();

        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 2_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);
        assert!(!state.should_sync_backend_position(false));
        assert!(!state.should_sync_backend_position(true));
    }

    #[test]
    fn stop_preference_with_fadeout_ramps_down_then_restores_volume() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            volume: 80,
            stop_with_fadeout: true,
            ..Config::default()
        }));
        state.app_state.player.mark_playing();

        state.activate_push(MainPushButton::Stop);
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::FadingOut {
                remaining_ms: STOP_FADE_DURATION_MS,
                start_volume: 80,
            }
        );

        assert!(state.update_timer_tick(500));
        assert_eq!(state.volume(), 40);
        assert!(state.update_timer_tick(500));
        assert_eq!(state.app_state.player.state(), PlayerState::Stopped);
        assert_eq!(state.volume(), 80);
        assert_eq!(state.playback_transition, PlaybackTransitionState::Idle);
    }

    #[test]
    fn playback_control_event_handles_play_pause_transitions() {
        let mut state = MainWindowUiState::default();
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);

        assert!(state.handle_playback_control_event(PlaybackControlEvent::Play));
        assert_eq!(state.app_state.player.state(), PlayerState::Playing);

        assert!(state.handle_playback_control_event(PlaybackControlEvent::Pause));
        assert_eq!(state.app_state.player.state(), PlayerState::Paused);

        assert!(!state.handle_playback_control_event(PlaybackControlEvent::Pause));
        assert_eq!(state.app_state.player.state(), PlayerState::Paused);

        assert!(state.handle_playback_control_event(PlaybackControlEvent::Play));
        assert_eq!(state.app_state.player.state(), PlayerState::Playing);

        assert!(!state.handle_playback_control_event(PlaybackControlEvent::Play));
        assert_eq!(state.app_state.player.state(), PlayerState::Playing);
    }

    #[test]
    fn panel_state_maps_visibility_detach_and_shade_flags() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            equalizer_visible: true,
            equalizer_detached: false,
            playlist_visible: true,
            playlist_detached: true,
            ..Config::default()
        }));
        state.toggle_equalizer_shaded();

        assert_eq!(
            state.panel_state(PanelKind::Equalizer),
            PanelState::Docked { shaded: true }
        );
        assert_eq!(
            state.panel_state(PanelKind::Playlist),
            PanelState::Detached { shaded: false }
        );

        state.set_playlist_visible(false);
        assert_eq!(state.panel_state(PanelKind::Playlist), PanelState::Hidden);
    }

    #[test]
    fn playlist_menu_command_maps_menu_indices() {
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Add, 0),
            Some(PlaylistMenuCommand::OpenLocationWindow)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Add, 2),
            Some(PlaylistMenuCommand::OpenFileDialog)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Remove, 3),
            Some(PlaylistMenuCommand::RemoveSelectedOrCurrent)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::List, 1),
            Some(PlaylistMenuCommand::SavePlaylist)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Misc, 99),
            None
        );
    }

    #[test]
    fn play_from_stopped_preserves_selected_position() {
        let mut state = MainWindowUiState::default();
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.set_playback_position_ms(42_000);

        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);

        assert_eq!(state.app_state.player.state(), PlayerState::Playing);
        assert_eq!(state.playback_position_ms, 42_000);
        assert_eq!(state.playback_position_ms, 42_000);
    }

    #[test]
    fn changing_to_next_track_starts_from_beginning() {
        let mut state = MainWindowUiState::default();
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/one.ogg", "One", 120_000);
        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/two.ogg", "Two", 120_000);
        state.app_state.playlist.set_position(0);
        state.set_playback_position_ms(42_000);

        state.press(109, 90);
        assert_eq!(state.release(109, 90), UiAction::None);

        assert_eq!(state.app_state.playlist.position(), Some(1));
        assert_eq!(state.playback_position_ms, 0);
        assert_eq!(state.playback_position_ms, 0);
    }

    #[test]
    fn skin_browser_content_and_refresh_match_original_selector() {
        let _gtk = init_gtk_for_tests();

        let add = gtk::Button::with_label("Add...");
        add.set_widget_name(SKIN_BROWSER_ADD_WIDGET);
        let close = gtk::Button::with_label("Close");
        close.set_widget_name(SKIN_BROWSER_CLOSE_WIDGET);
        let (content, _content_list) = build_skin_browser_content(&add, &close);

        let header = find_named_widget::<gtk::Label>(&content, SKIN_BROWSER_HEADER_WIDGET)
            .expect("skin browser should have a Skins header");
        assert_eq!(header.text(), "Skins");

        let list = find_named_widget::<gtk::ListBox>(&content, SKIN_BROWSER_LIST_WIDGET)
            .expect("skin browser should have a selectable skin list");
        assert_eq!(list.selection_mode(), gtk::SelectionMode::Single);

        let add = find_named_widget::<gtk::Button>(&content, SKIN_BROWSER_ADD_WIDGET)
            .expect("skin browser should have an Add button");
        assert_eq!(add.label().as_deref(), Some("Add..."));

        let close = find_named_widget::<gtk::Button>(&content, SKIN_BROWSER_CLOSE_WIDGET)
            .expect("skin browser should have a Close button");
        assert_eq!(close.label().as_deref(), Some("Close"));

        let mut labels = Vec::new();
        collect_label_text(&content, &mut labels);
        assert!(!labels
            .iter()
            .any(|label| label.contains("placeholder for the Rust port")));

        let tmp = unique_temp_dir("xmms-rs-skin-browser-refresh");
        let skins = tmp.join("Skins");
        let broken = skins.join("Broken");
        let classic = skins.join("Classic");
        fs::create_dir_all(&broken).unwrap();
        fs::create_dir_all(&classic).unwrap();
        fs::write(broken.join("main.xpm"), b"not an xpm").unwrap();
        fs::write(skins.join("Blue.wsz"), b"archive").unwrap();

        let mut state = MainWindowUiState::default();
        refresh_skin_browser_list(&list, &mut state, std::slice::from_ref(&skins)).unwrap();

        assert_eq!(
            skin_browser_list_labels(&list),
            ["default", "Blue", "Broken", "Classic"]
        );
        assert_eq!(list.selected_row().map(|row| row.index()), Some(0));

        state.app_state.config.skin = Some(classic.display().to_string());
        fs::create_dir_all(skins.join("Zed")).unwrap();
        refresh_skin_browser_list(&list, &mut state, std::slice::from_ref(&skins)).unwrap();

        assert_eq!(
            skin_browser_list_labels(&list),
            ["default", "Blue", "Broken", "Classic", "Zed"]
        );
        assert_eq!(list.selected_row().map(|row| row.index()), Some(3));

        fs::write(
            classic.join("main.xpm"),
            r#"/* XPM */
static char * main_xpm[] = {
"1 1 1 1",
". c #010203",
"."};
"#,
        )
        .unwrap();
        let main_state = Rc::new(RefCell::new(state));
        let main_area = gtk::DrawingArea::new();
        let equalizer_area = gtk::DrawingArea::new();
        let playlist_area = gtk::DrawingArea::new();
        let populating = Rc::new(Cell::new(false));
        connect_skin_browser_selection(
            &list,
            &main_state,
            &main_area,
            &equalizer_area,
            &playlist_area,
            &populating,
        );
        list.select_row(list.row_at_index(0).as_ref());
        list.select_row(list.row_at_index(3).as_ref());

        let state = main_state.borrow();
        let classic_path = classic.display().to_string();
        assert_eq!(state.selected_skin(), Some(classic_path.as_str()));
        assert_eq!(
            state
                .active_skin()
                .get(SkinPixmapKind::Main)
                .unwrap()
                .pixel_argb(0, 0),
            Some(0xff010203)
        );
        drop(state);

        list.select_row(list.row_at_index(2).as_ref());
        let state = main_state.borrow();
        assert_eq!(state.selected_skin(), Some(classic_path.as_str()));
        assert_eq!(state.selected_skin_index(), 3);
        assert_eq!(list.selected_row().map(|row| row.index()), Some(3));
        assert_eq!(
            state
                .active_skin()
                .get(SkinPixmapKind::Main)
                .unwrap()
                .pixel_argb(0, 0),
            Some(0xff010203)
        );

        fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn skin_browser_import_copies_archives_and_directories_to_user_skin_dir() {
        let tmp = unique_temp_dir("xmms-rs-skin-browser-import");
        let source = tmp.join("source");
        let user_skins = tmp.join("user-skins");
        fs::create_dir_all(&source).unwrap();

        let archive = source.join("Blue.wsz");
        fs::write(&archive, b"archive").unwrap();
        let imported_archive = import_skin_to_user_dir(&archive, &user_skins).unwrap();
        assert_eq!(imported_archive, user_skins.join("Blue.wsz"));
        assert_eq!(fs::read(&imported_archive).unwrap(), b"archive");

        let duplicate_archive = import_skin_to_user_dir(&archive, &user_skins).unwrap();
        assert_eq!(duplicate_archive, user_skins.join("Blue 1.wsz"));

        let dir_skin = source.join("Classic");
        fs::create_dir_all(dir_skin.join("nested")).unwrap();
        fs::write(dir_skin.join("main.xpm"), b"main").unwrap();
        fs::write(dir_skin.join("nested").join("eqmain.xpm"), b"eq").unwrap();
        let imported_dir = import_skin_to_user_dir(&dir_skin, &user_skins).unwrap();
        assert_eq!(fs::read(imported_dir.join("main.xpm")).unwrap(), b"main");
        assert_eq!(
            fs::read(imported_dir.join("nested").join("eqmain.xpm")).unwrap(),
            b"eq"
        );

        let unsupported = source.join("notes.txt");
        fs::write(&unsupported, b"not a skin").unwrap();
        assert!(import_skin_to_user_dir(&unsupported, &user_skins).is_err());

        fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn main_menu_anchor_uses_full_menu_button_rect() {
        let rect = main_menu_anchor_rect(
            MAIN_WINDOW_WIDTH * 2,
            MAIN_WINDOW_HEIGHT * 2,
            MAIN_WINDOW_WIDTH,
            MAIN_WINDOW_HEIGHT,
        );

        assert_eq!(rect.x(), 12);
        assert_eq!(rect.y(), 6);
        assert_eq!(rect.width(), 18);
        assert_eq!(rect.height(), 18);

        let docked_height = MAIN_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT;
        let rect = main_menu_anchor_rect(
            MAIN_WINDOW_WIDTH * 2,
            docked_height * 2,
            MAIN_WINDOW_WIDTH,
            docked_height,
        );

        assert_eq!(rect.x(), 12);
        assert_eq!(rect.y(), 6);
        assert_eq!(rect.width(), 18);
        assert_eq!(rect.height(), 18);
    }

    #[test]
    fn main_window_buttons_update_player_and_toggle_state() {
        let mut state = MainWindowUiState::default();

        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);
        assert_eq!(state.app_state.player.state(), PlayerState::Stopped);

        state
            .app_state
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 10_000);
        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);
        assert_eq!(state.app_state.player.state(), PlayerState::Playing);

        state.press(63, 90);
        assert_eq!(state.release(63, 90), UiAction::None);
        assert_eq!(state.app_state.player.state(), PlayerState::Paused);

        state.press(165, 90);
        assert_eq!(state.release(165, 90), UiAction::None);
        assert!(state.app_state.playlist.shuffle());

        state.press(243, 59);
        assert_eq!(state.release(243, 59), UiAction::None);
        assert!(state.app_state.config.playlist_visible);
    }

    #[test]
    fn main_render_state_formats_stream_info_like_xmms() {
        let mut state = MainWindowUiState::default();

        assert_eq!(state.render_state().bitrate_text, "   ");
        assert_eq!(state.render_state().frequency_text, "  ");

        state
            .app_state_mut()
            .player
            .set_stream_info(Some(192), Some(44_100), Some(2));
        assert_eq!(state.render_state().bitrate_text, "192");
        assert_eq!(state.render_state().frequency_text, "44");

        state
            .app_state_mut()
            .player
            .set_stream_info(Some(1280), Some(48), Some(2));
        assert_eq!(state.render_state().bitrate_text, "12H");
        assert_eq!(state.render_state().frequency_text, "48");
    }

    #[test]
    fn main_window_sliders_update_runtime_values() {
        let mut state = MainWindowUiState::default();

        state.press(107, 58);
        state.motion(107, 58);
        assert_eq!(state.release(107, 58), UiAction::None);
        assert_eq!(state.app_state.player.volume(), 0);

        state.press(214, 58);
        assert_eq!(state.release(214, 58), UiAction::None);
        assert!(state.app_state.player.balance() > 70);

        state.press(263, 73);
        assert_eq!(state.release(263, 73), UiAction::None);
        assert_eq!(state.position(), 0);
    }

    #[test]
    fn zoom_scale_helpers_resize_from_preferences_scale_factor() {
        let mut state = MainWindowUiState::default();
        state.set_preference_scale_factor(1.7);

        assert_eq!(scale_dim(MAIN_WINDOW_WIDTH, state.scale_factor()), 468);
        assert_eq!(unscale_dim(468, state.scale_factor()), MAIN_WINDOW_WIDTH);
    }

    #[test]
    fn shade_and_close_titlebar_buttons_return_window_actions() {
        let mut state = MainWindowUiState::default();

        state.press(255, 4);
        assert_eq!(state.release(255, 4), UiAction::Resize);
        assert!(state.shaded);

        state.press(265, 4);
        assert_eq!(state.release(265, 4), UiAction::Quit);
    }

    #[test]
    fn main_titlebar_drag_region_excludes_title_buttons() {
        let state = MainWindowUiState::default();

        assert!(state.main_title_drag_region(40, 7));
        assert!(!state.main_title_drag_region(6, 4));
        assert!(!state.main_title_drag_region(244, 4));
        assert!(!state.main_title_drag_region(254, 4));
        assert!(!state.main_title_drag_region(264, 4));
        assert!(!state.main_title_drag_region(40, MAIN_TITLEBAR_HEIGHT));
    }

    #[test]
    fn shaded_equalizer_sliders_are_not_titlebar_drag_regions() {
        let mut state = MainWindowUiState::default();
        state.toggle_equalizer_shaded();

        assert!(!state.panel_title_drag_region(PanelKind::Equalizer, 61, 7));
        assert!(!state.panel_title_drag_region(PanelKind::Equalizer, 164, 7));
        assert!(state.panel_title_drag_region(PanelKind::Equalizer, 40, 7));
    }

    #[test]
    fn parse_prompt_time_accepts_seconds_and_minutes_seconds() {
        assert_eq!(parse_time_ms("42"), Some(42_000));
        assert_eq!(parse_time_ms("1:23"), Some(83_000));
        assert_eq!(parse_time_ms(""), None);
        assert_eq!(parse_time_ms("1:2:3"), None);
        assert_eq!(parse_time_ms("not-time"), None);
    }
}
