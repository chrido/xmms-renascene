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
    docked_panel_size, equalizer_window_height, main_window_height, playlist_window_height,
    render_equalizer_state, render_main_player_state, render_playlist_frame, render_playlist_menu,
    render_playlist_rows, scale_dim, DockedPanelState, EqualizerControl, EqualizerRenderState,
    MainPushButton, MainSlider, MainToggleButton, MainWindowRenderState, PlaylistMenuRenderKind,
    PlaylistMenuRenderState, PlaylistRowRenderEntry, PlaylistRowsRenderState,
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
use crate::spotify::{SpotifyPlaylist, SpotifyTrack};

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
const XMMS_MENU_CSS_TEMPLATE: &str = r#"
.xmms-menu-popover,
.xmms-menu-popover contents,
.xmms-menu-box {
    background: MENU_NORMAL_BG;
    color: MENU_NORMAL;
}

.xmms-menu-button {
    background: MENU_NORMAL_BG;
    background-image: none;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    color: MENU_NORMAL;
    padding: 4px 12px;
    text-shadow: none;
}

.xmms-menu-button:hover {
    background: MENU_SELECTED_BG;
    color: MENU_CURRENT;
}

.xmms-menu-button:active {
    background: MENU_SELECTED_BG;
    color: MENU_CURRENT;
}

.xmms-menu-popover modelbutton {
    background: MENU_NORMAL_BG;
    background-image: none;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    color: MENU_NORMAL;
    padding: 4px 12px;
    text-shadow: none;
}

.xmms-menu-popover modelbutton:hover {
    background: MENU_SELECTED_BG;
    color: MENU_CURRENT;
}
"#;

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
    pub skin_path: Option<String>,
    pub screenshot_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotifyChooserPage {
    Playlists,
    Tracks,
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
    render_docked_ui_state(&cr, state.active_skin(), &state)
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
    let mut state = preview_state_from_app_state(app_state, options)?;
    if let Some(config_dir) = config_path.parent() {
        state.set_equalizer_preset_dir(config_dir.to_path_buf());
    }
    match GStreamerBackend::new() {
        Ok(backend) => state.set_playback_backend(Rc::new(RefCell::new(backend))),
        Err(err) => eprintln!("xmms-rs: audio playback backend unavailable: {err}"),
    }
    let main_state = Rc::new(RefCell::new(state));
    install_xmms_menu_css(main_state.borrow().active_skin());

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Renascene Rust Preview")
        .resizable(false)
        .decorated(false)
        .default_width(MAIN_WINDOW_WIDTH * DEFAULT_SCALE)
        .default_height(MAIN_WINDOW_HEIGHT * DEFAULT_SCALE)
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
        .content_width(MAIN_WINDOW_WIDTH * DEFAULT_SCALE)
        .content_height(MAIN_WINDOW_HEIGHT * DEFAULT_SCALE)
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
            cr.scale(
                width as f64 / base_width as f64,
                height as f64 / base_height as f64,
            );
            if let Err(err) = render_docked_ui_state(cr, state.active_skin(), &state) {
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
    Ok(())
}

fn install_xmms_menu_css(skin: &DefaultSkin) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&xmms_menu_css(skin));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn xmms_menu_css(skin: &DefaultSkin) -> String {
    let colors = skin.playlist_colors();
    XMMS_MENU_CSS_TEMPLATE
        .replace("MENU_NORMAL_BG", &css_rgb(colors.normal_bg))
        .replace("MENU_NORMAL", &css_rgb(colors.normal))
        .replace("MENU_SELECTED_BG", &css_rgb(colors.selected_bg))
        .replace("MENU_CURRENT", &css_rgb(colors.current))
}

fn css_rgb(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
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
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return false;
    }
    let mut ui_state = main_state.borrow_mut();
    if !ui_state.app_state.config.playlist_visible {
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
            {
                let mut state = main_state.borrow_mut();
                state.shaded = !state.shaded;
            }
            resize_main_window(window, drawing_area, &main_state.borrow());
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
                state.playlist_shaded = !state.playlist_shaded;
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
                state.equalizer_shaded = !state.equalizer_shaded;
            }
            sync_single_panel_window_from_state(
                PanelKind::Equalizer,
                &panel_windows.equalizer,
                &panel_windows.equalizer_area,
                main_state,
            );
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
) -> Result<bool, crate::render::RenderError> {
    let mut y = 0;
    let mut rendered = render_main_player_state(cr, skin, &state.render_state())?;
    y += main_window_height(state.shaded);

    if state.app_state.config.equalizer_visible && !state.app_state.config.equalizer_detached {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        rendered |= render_equalizer_state(cr, skin, &state.equalizer_render_state())?;
        cr.restore()?;
        y += equalizer_window_height(state.equalizer_shaded);
    }

    if state.app_state.config.playlist_visible && !state.app_state.config.playlist_detached {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        rendered |= render_playlist_frame(
            cr,
            skin,
            state.playlist_focused || state.playlist_dragging_title,
            state.playlist_shaded,
            state.playlist_width,
            state.playlist_height,
            Some(&state.shaded_playlist_info()),
            Some(&state.playlist_footer_info()),
            Some(&state.playlist_footer_time_min_text()),
            Some(&state.playlist_footer_time_sec_text()),
        )?;
        if !state.playlist_shaded {
            let current = state.app_state.playlist.position();
            let rows = state
                .app_state
                .playlist
                .entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| PlaylistRowRenderEntry {
                    title: state.formatted_playlist_entry_title(entry),
                    length_ms: entry.length_ms,
                    selected: entry.selected,
                    current: current == Some(index),
                })
                .collect();
            let row_state = PlaylistRowsRenderState {
                entries: rows,
                scroll_offset: state.playlist_scroll_offset,
                scrollbar_dragging: state.playlist_scrollbar_dragging,
                search_query: state
                    .playlist_search_active
                    .then(|| state.playlist_search_query.clone()),
                show_numbers: state.app_state.config.show_numbers_in_pl,
                font_family: state.app_state.config.playlist_font.clone(),
                width: state.playlist_width,
                height: state.playlist_height,
            };
            rendered |= render_playlist_rows(cr, skin, &row_state)?;
        }
        if let Some(menu) = state.playlist_menu() {
            let (x, y, _, _) =
                playlist_menu_rect(menu, state.playlist_width, state.playlist_height);
            cr.save()?;
            cr.translate(f64::from(x), f64::from(y));
            rendered |= render_playlist_menu(
                cr,
                skin,
                PlaylistMenuRenderState {
                    kind: menu.render_kind(),
                    hover: state.playlist_menu_hover(),
                },
            )?;
            cr.restore()?;
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

    let spotify = xmms_menu_button("Spotify Playlists...");
    {
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        spotify.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                state.set_menu_visible(false);
                state.open_spotify_window();
            }
            popover.popdown();
        });
    }
    menu_box.append(&spotify);

    let stop_with_fade = xmms_menu_button("Stop with Fadeout");
    {
        let popover = popover.clone();
        let main_state = Rc::clone(main_state);
        stop_with_fade.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                state.set_menu_visible(false);
                state.stop_with_fade();
            }
            popover.popdown();
        });
    }
    menu_box.append(&stop_with_fade);

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
        cr.scale(
            width as f64 / EQUALIZER_WINDOW_WIDTH as f64,
            height as f64 / base_height as f64,
        );
        if let Err(err) = render_equalizer_state(cr, state.active_skin(), &render_state) {
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
        let shaded = state.playlist_shaded;
        let focused = state.playlist_focused || state.playlist_dragging_title;
        let playlist_width = state.playlist_width;
        let playlist_height = state.playlist_height;
        let base_height = if shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            playlist_height
        };
        cr.scale(
            width as f64 / playlist_width as f64,
            height as f64 / base_height as f64,
        );
        if let Err(err) = render_playlist_frame(
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
        ) {
            eprintln!("xmms-rs: failed to render playlist preview: {err}");
        }
        if !shaded {
            let current = state.app_state.playlist.position();
            let rows = state
                .app_state
                .playlist
                .entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| PlaylistRowRenderEntry {
                    title: state.formatted_playlist_entry_title(entry),
                    length_ms: entry.length_ms,
                    selected: entry.selected,
                    current: current == Some(index),
                })
                .collect();
            let row_state = PlaylistRowsRenderState {
                entries: rows,
                scroll_offset: state.playlist_scroll_offset,
                scrollbar_dragging: state.playlist_scrollbar_dragging,
                search_query: state
                    .playlist_search_active
                    .then(|| state.playlist_search_query.clone()),
                show_numbers: state.app_state.config.show_numbers_in_pl,
                font_family: state.app_state.config.playlist_font.clone(),
                width: playlist_width,
                height: playlist_height,
            };
            if let Err(err) = render_playlist_rows(cr, skin, &row_state) {
                eprintln!("xmms-rs: failed to render playlist rows: {err}");
            }
        }
        if let Some(menu) = state.playlist_menu() {
            let (x, y, _, _) = playlist_menu_rect(menu, playlist_width, playlist_height);
            if let Err(err) = cr.save() {
                eprintln!("xmms-rs: failed to save playlist menu render state: {err}");
                return;
            }
            cr.translate(f64::from(x), f64::from(y));
            let render_state = PlaylistMenuRenderState {
                kind: menu.render_kind(),
                hover: state.playlist_menu_hover(),
            };
            if let Err(err) = render_playlist_menu(cr, skin, render_state) {
                eprintln!("xmms-rs: failed to render playlist menu: {err}");
            }
            if let Err(err) = cr.restore() {
                eprintln!("xmms-rs: failed to restore playlist menu render state: {err}");
            }
        }
    });

    add_file_drop_controller(&drawing_area, Rc::clone(main_state), false, false);
    add_playlist_context_menu(&drawing_area, Rc::clone(main_state), main_area.clone());
    add_playlist_key_controller(&drawing_area, Rc::clone(main_state));

    {
        let main_state = Rc::clone(main_state);
        drawing_area.connect_resize(move |area, width, height| {
            let mut state = main_state.borrow_mut();
            if !state.app_state.config.playlist_detached {
                return;
            }
            let scale = state.scale_factor();
            let base_height = if state.playlist_shaded {
                state.playlist_height
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
            if state.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::META_MASK,
            ) {
                return gtk::glib::Propagation::Proceed;
            }
            let diff = match key {
                gtk::gdk::Key::Left | gtk::gdk::Key::KP_Left => -4,
                gtk::gdk::Key::Right | gtk::gdk::Key::KP_Right => 4,
                _ => return gtk::glib::Propagation::Proceed,
            };
            if main_state
                .borrow_mut()
                .adjust_shaded_equalizer_balance(diff)
            {
                area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    area.add_controller(key_controller);
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
    if !ui_state.playlist_search_active {
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
    let window = gtk::Window::builder()
        .title("Delete selected files?")
        .modal(true)
        .default_width(280)
        .default_height(100)
        .build();
    if let Some(root) = parent
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
    {
        window.set_transient_for(Some(&root));
    }

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
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
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Preferences")
        .default_width(default_width)
        .default_height(default_height)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
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
        notebook.append_page(&page_widget, Some(&gtk::Label::new(Some(label))));
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
                "Time remaining",
                state.preference_timer_remaining(),
                PreferenceCheck::TimerRemaining,
            ),
            (
                "Dock playlist",
                !state.app_state.config.playlist_detached,
                PreferenceCheck::DockPlaylist,
            ),
            (
                "Dock equalizer",
                !state.app_state.config.equalizer_detached,
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
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(kind.title())
        .transient_for(parent)
        .modal(true)
        .resizable(false)
        .default_width(360)
        .default_height(110)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
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
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Skin selector")
        .default_width(300)
        .default_height(280)
        .build();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistMenuKind {
    Add,
    Remove,
    Select,
    Misc,
    List,
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
    let window = gtk::Window::builder()
        .title(title)
        .modal(true)
        .default_width(320)
        .default_height(90)
        .build();
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
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
    let window = gtk::Window::builder()
        .title(title)
        .modal(true)
        .default_width(350)
        .default_height(300)
        .build();
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
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
    let window = gtk::Window::builder()
        .title("Configure Equalizer")
        .modal(true)
        .default_width(360)
        .default_height(140)
        .build();
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
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
            if state.equalizer_shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                EQUALIZER_WINDOW_HEIGHT
            },
        ),
        PanelKind::Playlist => (
            state.playlist_width,
            if state.playlist_shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                state.playlist_height
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
            state.app_state.config.equalizer_visible && state.app_state.config.equalizer_detached,
            state.equalizer_shaded,
            EQUALIZER_WINDOW_WIDTH,
            EQUALIZER_WINDOW_HEIGHT,
        ),
        PanelKind::Playlist => (
            state.app_state.config.playlist_visible && state.app_state.config.playlist_detached,
            state.playlist_shaded,
            state.playlist_width,
            state.playlist_height,
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
        let height = if state.equalizer_shaded {
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
        let height = if state.playlist_shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            state.playlist_height
        };
        windows
            .playlist_area
            .set_content_width(scale_dim(state.playlist_width, scale));
        windows
            .playlist_area
            .set_content_height(scale_dim(height, scale));
        windows.playlist.set_resizable(!state.playlist_shaded);
        windows.playlist.set_default_size(
            scale_dim(state.playlist_width, scale),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainControl {
    Push(MainPushButton),
    Toggle(MainToggleButton),
    Slider(MainSlider),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeekState {
    Idle,
    StoppedAt(i64),
    PendingBackendSeek(i64),
    WaitingBetweenSongs { remaining_ms: i64 },
}

impl SeekState {
    fn eof_pause_remaining_ms(self) -> Option<i64> {
        match self {
            SeekState::WaitingBetweenSongs { remaining_ms } => Some(remaining_ms),
            _ => None,
        }
    }

    fn pending_backend_seek_ms(self) -> Option<i64> {
        match self {
            SeekState::PendingBackendSeek(position_ms) => Some(position_ms),
            _ => None,
        }
    }

    fn play_start_position_ms(self, fallback_ms: i64) -> i64 {
        match self {
            SeekState::StoppedAt(position_ms) => position_ms,
            SeekState::WaitingBetweenSongs { .. } => 0,
            SeekState::Idle | SeekState::PendingBackendSeek(_) => fallback_ms,
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

pub(crate) struct MainWindowUiState {
    app_state: AppState,
    playback_backend: Option<Rc<RefCell<GStreamerBackend>>>,
    duration_index_sender: Sender<DurationIndexResult>,
    duration_index_receiver: Receiver<DurationIndexResult>,
    playback_requests: Vec<String>,
    shaded: bool,
    menu_visible: bool,
    equalizer_shaded: bool,
    equalizer_focused: bool,
    equalizer_dragging_title: bool,
    equalizer_active: bool,
    equalizer_automatic: bool,
    equalizer_pressed_control: Option<EqualizerControl>,
    equalizer_pressed_inside: bool,
    equalizer_dragging: Option<EqualizerSlider>,
    equalizer_slider_press_offset: i32,
    equalizer_preamp_position: i32,
    equalizer_band_positions: [i32; 10],
    equalizer_preset_dir: PathBuf,
    equalizer_presets: Vec<EqualizerPreset>,
    equalizer_auto_presets: Vec<EqualizerPreset>,
    playlist_shaded: bool,
    playlist_focused: bool,
    playlist_dragging_title: bool,
    playlist_width: i32,
    playlist_height: i32,
    playlist_menu: Option<PlaylistMenuKind>,
    playlist_menu_hover: Option<usize>,
    playlist_menu_pressed: bool,
    playlist_scroll_offset: usize,
    playlist_scrollbar_dragging: bool,
    playlist_scrollbar_drag_offset: i32,
    playlist_docked_resizing: bool,
    playlist_resize_drag_offset_y: i32,
    playlist_drag_index: Option<usize>,
    playlist_drag_moved: bool,
    playlist_last_click: Option<(usize, Instant)>,
    playlist_pending_double_click: Option<usize>,
    playlist_search_active: bool,
    playlist_search_query: String,
    playlist_load_dialog_visible: bool,
    playlist_save_dialog_visible: bool,
    last_playlist_file_info: Option<String>,
    active_skin: DefaultSkin,
    playlist_options_opened: bool,
    preferences_visible: bool,
    preferences_page: PreferencesPage,
    preferences_saved: bool,
    open_location_visible: bool,
    jump_time_visible: bool,
    skin_browser_visible: bool,
    skin_browser_entries: Vec<SkinEntry>,
    selected_skin_index: usize,
    skin_reload_count: u32,
    spotify_authenticated: bool,
    spotify_auth_prompt_visible: bool,
    spotify_window_visible: bool,
    spotify_page: SpotifyChooserPage,
    spotify_status: String,
    spotify_playlists: Vec<SpotifyPlaylist>,
    spotify_tracks: Vec<SpotifyTrack>,
    spotify_current_playlist_uri: Option<String>,
    spotify_last_track_request: Option<String>,
    output_device_picker_visible: bool,
    output_device_groups: OutputDeviceGroups,
    selected_spotify_output_device: Option<String>,
    output_switch_count: u32,
    spotify_playback_poll_requests: u32,
    mpris_events: Vec<MprisEvent>,
    mpris_quit_requested: bool,
    seek_state: SeekState,
    stop_fade_remaining_ms: i64,
    stop_fade_start_volume: i32,
    file_dialog_visible: bool,
    directory_dialog_visible: bool,
    last_open_location: Option<String>,
    last_jump_time_ms: Option<i64>,
    position_position: i32,
    playback_position_ms: i64,
    visualization: Visualization,
    visualization_tick_counter: i32,
    active: Option<MainControl>,
    active_inside: bool,
    slider_press_offset: i32,
}

impl fmt::Debug for MainWindowUiState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MainWindowUiState")
            .field("app_state", &self.app_state)
            .field("shaded", &self.shaded)
            .field("playlist_shaded", &self.playlist_shaded)
            .field("preferences_visible", &self.preferences_visible)
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
        let equalizer_shaded = app_state.config.equalizer_shaded;
        let playlist_shaded = app_state.config.playlist_shaded;
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
            equalizer_shaded,
            equalizer_focused: false,
            equalizer_dragging_title: false,
            equalizer_active: true,
            equalizer_automatic: false,
            equalizer_pressed_control: None,
            equalizer_pressed_inside: false,
            equalizer_dragging: None,
            equalizer_slider_press_offset: 0,
            equalizer_preamp_position: 50,
            equalizer_band_positions: [50; 10],
            equalizer_preset_dir: default_config_dir().join("xmms-renascene"),
            equalizer_presets: Vec::new(),
            equalizer_auto_presets: Vec::new(),
            playlist_shaded,
            playlist_focused: false,
            playlist_dragging_title: false,
            playlist_width: PLAYLIST_DEFAULT_WIDTH,
            playlist_height: PLAYLIST_DEFAULT_HEIGHT,
            playlist_menu: None,
            playlist_menu_hover: None,
            playlist_menu_pressed: false,
            playlist_scroll_offset: 0,
            playlist_scrollbar_dragging: false,
            playlist_scrollbar_drag_offset: 0,
            playlist_docked_resizing: false,
            playlist_resize_drag_offset_y: 0,
            playlist_drag_index: None,
            playlist_drag_moved: false,
            playlist_last_click: None,
            playlist_pending_double_click: None,
            playlist_search_active: false,
            playlist_search_query: String::new(),
            playlist_load_dialog_visible: false,
            playlist_save_dialog_visible: false,
            last_playlist_file_info: None,
            active_skin,
            playlist_options_opened: false,
            preferences_visible: false,
            preferences_page: PreferencesPage::Options,
            preferences_saved: false,
            open_location_visible: false,
            jump_time_visible: false,
            skin_browser_visible: false,
            skin_browser_entries: Vec::new(),
            selected_skin_index: 0,
            skin_reload_count: 0,
            spotify_authenticated: false,
            spotify_auth_prompt_visible: false,
            spotify_window_visible: false,
            spotify_page: SpotifyChooserPage::Playlists,
            spotify_status: String::new(),
            spotify_playlists: Vec::new(),
            spotify_tracks: Vec::new(),
            spotify_current_playlist_uri: None,
            spotify_last_track_request: None,
            output_device_picker_visible: false,
            output_device_groups: OutputDeviceGroups::default(),
            selected_spotify_output_device: None,
            output_switch_count: 0,
            spotify_playback_poll_requests: 0,
            mpris_events: Vec::new(),
            mpris_quit_requested: false,
            seek_state: SeekState::Idle,
            stop_fade_remaining_ms: 0,
            stop_fade_start_volume: 100,
            file_dialog_visible: false,
            directory_dialog_visible: false,
            last_open_location: None,
            last_jump_time_ms: None,
            position_position: 0,
            playback_position_ms: 0,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
            active: None,
            active_inside: false,
            slider_press_offset: 0,
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

    fn load_configured_skin(&mut self) -> io::Result<()> {
        self.active_skin = load_skin_from_config(&self.app_state.config)?;
        Ok(())
    }

    fn set_equalizer_preset_dir(&mut self, dir: PathBuf) {
        self.equalizer_preset_dir = dir;
        if let Err(err) = self.load_equalizer_preset_stores() {
            eprintln!("xmms-rs: failed to load equalizer presets: {err}");
        }
    }

    fn load_equalizer_preset_stores(&mut self) -> io::Result<()> {
        self.equalizer_presets =
            load_preset_store(&preset_store_path(&self.equalizer_preset_dir, "eq.preset"))?;
        if self.equalizer_presets.is_empty() {
            self.equalizer_presets = default_equalizer_presets();
        }
        self.equalizer_auto_presets = load_preset_store(&preset_store_path(
            &self.equalizer_preset_dir,
            "eq.auto_preset",
        ))?;
        Ok(())
    }

    fn save_equalizer_presets(&self) -> io::Result<()> {
        save_preset_store(
            &preset_store_path(&self.equalizer_preset_dir, "eq.preset"),
            &self.equalizer_presets,
        )
    }

    fn save_equalizer_auto_presets(&self) -> io::Result<()> {
        save_preset_store(
            &preset_store_path(&self.equalizer_preset_dir, "eq.auto_preset"),
            &self.equalizer_auto_presets,
        )
    }

    fn current_equalizer_preset(&self, name: impl Into<String>) -> EqualizerPreset {
        EqualizerPreset::from_positions(
            name,
            self.equalizer_preamp_position,
            self.equalizer_band_positions,
        )
    }

    fn apply_equalizer_preset_values(&mut self, preset: &EqualizerPreset) {
        self.equalizer_preamp_position = preset.preamp_position();
        self.equalizer_band_positions = preset.band_positions();
        self.sync_equalizer_to_backend();
    }

    fn load_named_equalizer_preset(&mut self, name: &str, automatic: bool) -> bool {
        let preset = if automatic {
            find_preset(&self.equalizer_auto_presets, name)
        } else {
            find_preset(&self.equalizer_presets, name)
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
            upsert_preset(&mut self.equalizer_auto_presets, preset);
            self.save_equalizer_auto_presets()
        } else {
            upsert_preset(&mut self.equalizer_presets, preset);
            self.save_equalizer_presets()
        }
    }

    fn delete_named_equalizer_presets(
        &mut self,
        names: Vec<String>,
        automatic: bool,
    ) -> io::Result<()> {
        if automatic {
            remove_presets(&mut self.equalizer_auto_presets, &names);
            self.save_equalizer_auto_presets()
        } else {
            remove_presets(&mut self.equalizer_presets, &names);
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
            upsert_preset(&mut self.equalizer_presets, preset);
        }
        self.save_equalizer_presets()?;
        Ok(count)
    }

    fn save_equalizer_winamp_file(&self, path: &Path) -> io::Result<()> {
        save_winamp_eqf(path, &self.current_equalizer_preset("Entry1"))
    }

    fn sorted_equalizer_presets(&self, automatic: bool) -> Vec<EqualizerPreset> {
        let mut presets = if automatic {
            self.equalizer_auto_presets.clone()
        } else {
            self.equalizer_presets.clone()
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
                self.equalizer_active,
                self.equalizer_preamp_position,
                self.equalizer_band_positions,
            );
        }
        self.playback_backend = Some(backend);
    }

    fn render_state(&self) -> MainWindowRenderState {
        MainWindowRenderState {
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
            equalizer_selected: self.app_state.config.equalizer_visible,
            playlist_selected: self.app_state.config.playlist_visible,
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
            ..MainWindowRenderState::default()
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
        let slider = self.equalizer_dragging?;
        let (label, position) = match slider {
            EqualizerSlider::Preamp => ("PREAMP", self.equalizer_preamp_position),
            EqualizerSlider::Band(0) => ("60HZ", self.equalizer_band_positions[0]),
            EqualizerSlider::Band(1) => ("170HZ", self.equalizer_band_positions[1]),
            EqualizerSlider::Band(2) => ("310HZ", self.equalizer_band_positions[2]),
            EqualizerSlider::Band(3) => ("600HZ", self.equalizer_band_positions[3]),
            EqualizerSlider::Band(4) => ("1KHZ", self.equalizer_band_positions[4]),
            EqualizerSlider::Band(5) => ("3KHZ", self.equalizer_band_positions[5]),
            EqualizerSlider::Band(6) => ("6KHZ", self.equalizer_band_positions[6]),
            EqualizerSlider::Band(7) => ("12KHZ", self.equalizer_band_positions[7]),
            EqualizerSlider::Band(8) => ("14KHZ", self.equalizer_band_positions[8]),
            EqualizerSlider::Band(9) => ("16KHZ", self.equalizer_band_positions[9]),
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
        let max_len = ((self.playlist_width - 35) / 5)
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
        if self.app_state.playlist.position().is_none() && self.app_state.playlist.len() > 0 {
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
        if let Some(remaining) = self.seek_state.eof_pause_remaining_ms() {
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
            && self.seek_state.eof_pause_remaining_ms().is_none()
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

    fn equalizer_render_state(&self) -> EqualizerRenderState {
        EqualizerRenderState {
            focused: self.equalizer_focused || self.equalizer_dragging_title,
            shaded: self.equalizer_shaded,
            active: self.equalizer_active,
            automatic: self.equalizer_automatic,
            pressed_control: self
                .equalizer_pressed_control
                .filter(|_| self.equalizer_pressed_inside),
            pressed_slider: self.equalizer_dragging,
            preamp_position: self.equalizer_preamp_position,
            band_positions: self.equalizer_band_positions,
            volume_position: volume_to_eq_shaded_position(self.app_state.player.volume()),
            balance_position: balance_to_eq_shaded_position(self.app_state.player.balance()),
        }
    }

    pub(crate) fn panel_visibility(&self) -> PanelVisibility {
        PanelVisibility {
            equalizer: self.app_state.config.equalizer_visible
                && self.app_state.config.equalizer_detached,
            playlist: self.app_state.config.playlist_visible
                && self.app_state.config.playlist_detached,
        }
    }

    pub(crate) fn docked_panel_state(&self) -> DockedPanelState {
        DockedPanelState {
            main_focused: true,
            main_shaded: self.shaded,
            equalizer_visible: self.app_state.config.equalizer_visible,
            equalizer_detached: self.app_state.config.equalizer_detached,
            equalizer_focused: self.equalizer_focused || self.equalizer_dragging_title,
            equalizer_shaded: self.equalizer_shaded,
            playlist_visible: self.app_state.config.playlist_visible,
            playlist_detached: self.app_state.config.playlist_detached,
            playlist_focused: self.playlist_focused || self.playlist_dragging_title,
            playlist_shaded: self.playlist_shaded,
            playlist_width: self.playlist_width,
            playlist_height: self.playlist_height,
        }
    }

    pub(crate) fn docked_panel_size(&self) -> (i32, i32) {
        docked_panel_size(self.docked_panel_state())
    }

    pub(crate) fn docked_panel_at(&self, x: i32, y: i32) -> Option<(PanelKind, i32, i32)> {
        let mut offset_y = main_window_height(self.shaded);
        if self.app_state.config.equalizer_visible && !self.app_state.config.equalizer_detached {
            let height = equalizer_window_height(self.equalizer_shaded);
            if x >= 0 && x < EQUALIZER_WINDOW_WIDTH && y >= offset_y && y < offset_y + height {
                return Some((PanelKind::Equalizer, x, y - offset_y));
            }
            offset_y += height;
        }

        if self.app_state.config.playlist_visible && !self.app_state.config.playlist_detached {
            let height = playlist_window_height(self.playlist_shaded, self.playlist_height);
            if x >= 0 && x < self.playlist_width && y >= offset_y && y < offset_y + height {
                return Some((PanelKind::Playlist, x, y - offset_y));
            }
        }

        None
    }

    fn docked_playlist_local_y(&self, y: i32) -> Option<i32> {
        if !self.app_state.config.playlist_visible || self.app_state.config.playlist_detached {
            return None;
        }
        let mut offset_y = main_window_height(self.shaded);
        if self.app_state.config.equalizer_visible && !self.app_state.config.equalizer_detached {
            offset_y += equalizer_window_height(self.equalizer_shaded);
        }
        Some(y - offset_y)
    }

    pub(crate) fn set_panel_detached(&mut self, kind: PanelKind, detached: bool) {
        match kind {
            PanelKind::Equalizer => self.app_state.config.equalizer_detached = detached,
            PanelKind::Playlist => self.app_state.config.playlist_detached = detached,
        }
    }

    pub(crate) fn is_panel_detached(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Equalizer => self.app_state.config.equalizer_detached,
            PanelKind::Playlist => self.app_state.config.playlist_detached,
        }
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
        self.equalizer_shaded
    }

    pub(crate) fn is_playlist_shaded(&self) -> bool {
        self.playlist_shaded
    }

    pub(crate) fn playlist_menu(&self) -> Option<PlaylistMenuKind> {
        self.playlist_menu
    }

    pub(crate) fn playlist_menu_hover(&self) -> Option<usize> {
        self.playlist_menu_hover
    }

    pub(crate) fn playlist_menu_pressed(&self) -> bool {
        self.playlist_menu_pressed
    }

    pub(crate) fn playlist_size(&self) -> (i32, i32) {
        (self.playlist_width, self.playlist_height)
    }

    pub(crate) fn playlist_scroll_offset(&self) -> usize {
        self.playlist_scroll_offset
    }

    pub(crate) fn playlist_scrollbar_visible(&self) -> bool {
        self.playlist_scrollbar_geometry().is_some()
    }

    pub(crate) fn playlist_search_active(&self) -> bool {
        self.playlist_search_active
    }

    pub(crate) fn playlist_search_query(&self) -> &str {
        &self.playlist_search_query
    }

    pub(crate) fn set_playlist_visible(&mut self, visible: bool) {
        self.app_state.config.playlist_visible = visible;
    }

    pub(crate) fn is_preferences_visible(&self) -> bool {
        self.preferences_visible
    }

    pub(crate) fn set_preferences_visible(&mut self, visible: bool) {
        self.preferences_visible = visible;
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
        self.equalizer_active = self.app_state.config.equalizer_active;
        self.equalizer_automatic = self.app_state.config.equalizer_auto;
        self.equalizer_preamp_position = self.app_state.config.equalizer_preamp_pos;
        self.equalizer_band_positions = self.app_state.config.equalizer_band_pos;
        self.playback_position_ms = self.app_state.config.playback_position_ms.max(0);
        self.seek_state = if self.playback_position_ms > 0 {
            SeekState::StoppedAt(self.playback_position_ms)
        } else {
            SeekState::Idle
        };
        self.position_position = self.position_slider_position();
        self.apply_visualization_preferences();
    }

    fn sync_config_from_ui_state(&mut self) {
        self.app_state.config.playback_position_ms = self.playback_position_ms.max(0);
        self.app_state.config.main_shaded = self.shaded;
        self.app_state.config.playlist_shaded = self.playlist_shaded;
        self.app_state.config.equalizer_shaded = self.equalizer_shaded;
        self.app_state.config.equalizer_active = self.equalizer_active;
        self.app_state.config.equalizer_auto = self.equalizer_automatic;
        self.app_state.config.equalizer_preamp_pos = self.equalizer_preamp_position;
        self.app_state.config.equalizer_band_pos = self.equalizer_band_positions;
    }

    pub(crate) fn reset_preferences_to_defaults(&mut self) {
        self.app_state.config = Config::default();
        self.app_state.apply_config_to_runtime();
        self.apply_config_to_ui_state();
        self.mark_preferences_saved();
    }

    pub(crate) fn is_open_location_visible(&self) -> bool {
        self.open_location_visible
    }

    pub(crate) fn set_open_location_visible(&mut self, visible: bool) {
        self.open_location_visible = visible;
    }

    pub(crate) fn is_jump_time_visible(&self) -> bool {
        self.jump_time_visible
    }

    pub(crate) fn set_jump_time_visible(&mut self, visible: bool) {
        self.jump_time_visible = visible;
    }

    pub(crate) fn is_skin_browser_visible(&self) -> bool {
        self.skin_browser_visible
    }

    pub(crate) fn set_skin_browser_visible(&mut self, visible: bool) {
        self.skin_browser_visible = visible;
    }

    pub(crate) fn is_output_device_picker_visible(&self) -> bool {
        self.output_device_picker_visible
    }

    pub(crate) fn set_output_device_picker_visible(&mut self, visible: bool) {
        self.output_device_picker_visible = visible;
    }

    pub(crate) fn set_output_devices(
        &mut self,
        system_devices: Vec<OutputDevice>,
        spotify_devices: Vec<OutputDevice>,
    ) {
        self.output_device_groups = group_output_devices(system_devices, spotify_devices);
    }

    pub(crate) fn output_device_groups(&self) -> &OutputDeviceGroups {
        &self.output_device_groups
    }

    pub(crate) fn selected_output_device(&self) -> Option<&str> {
        self.app_state.config.output_device.as_deref()
    }

    pub(crate) fn selected_spotify_output_device(&self) -> Option<&str> {
        self.selected_spotify_output_device.as_deref()
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
            OutputDeviceSelection::Spotify(id) => {
                if !self
                    .output_device_groups
                    .spotify
                    .iter()
                    .any(|device| device.id == id)
                {
                    return false;
                }
                self.selected_spotify_output_device = Some(id.to_string());
                self.output_switch_count = self.output_switch_count.saturating_add(1);
                true
            }
        }
    }

    pub(crate) fn output_switch_count(&self) -> u32 {
        self.output_switch_count
    }

    pub(crate) fn spotify_playback_poll_requests(&self) -> u32 {
        self.spotify_playback_poll_requests
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
                if self.app_state.player.state() == PlayerState::Playing {
                    self.app_state.player.pause();
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
                if self.app_state.player.state() == PlayerState::Paused {
                    self.app_state.player.unpause();
                } else {
                    if self.app_state.playlist.position().is_none()
                        && self.app_state.playlist.len() > 0
                    {
                        self.app_state.playlist.set_position(0);
                    }
                    self.start_current_playlist_playback();
                }
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
        self.skin_browser_entries = discover_skins_in_dirs(dirs)?;
        self.selected_skin_index = self
            .app_state
            .config
            .skin
            .as_deref()
            .and_then(|current| {
                self.skin_browser_entries
                    .iter()
                    .position(|entry| entry.path == Path::new(current))
                    .map(|index| index + 1)
            })
            .unwrap_or(0);
        Ok(())
    }

    pub(crate) fn skin_browser_entries(&self) -> &[SkinEntry] {
        &self.skin_browser_entries
    }

    pub(crate) fn selected_skin_index(&self) -> usize {
        self.selected_skin_index
    }

    pub(crate) fn selected_skin(&self) -> Option<&str> {
        self.app_state.config.skin.as_deref()
    }

    pub(crate) fn select_skin_browser_index(&mut self, index: usize) -> bool {
        let previous_skin = self.app_state.config.skin.clone();
        let previous_index = self.selected_skin_index;
        if index == 0 {
            self.app_state.config.skin = None;
            self.selected_skin_index = 0;
        } else {
            let Some(entry) = self.skin_browser_entries.get(index - 1) else {
                return false;
            };
            self.app_state.config.skin = Some(entry.path.display().to_string());
            self.selected_skin_index = index;
        }

        if let Err(err) = self.reload_skin() {
            eprintln!("xmms-rs: failed to load selected skin: {err}");
            self.app_state.config.skin = previous_skin;
            self.selected_skin_index = previous_index;
            return false;
        }
        true
    }

    pub(crate) fn reload_skin(&mut self) -> io::Result<()> {
        self.load_configured_skin()?;
        self.skin_reload_count = self.skin_reload_count.saturating_add(1);
        Ok(())
    }

    pub(crate) fn skin_reload_count(&self) -> u32 {
        self.skin_reload_count
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
        if self.app_state.config.scale_factor <= 1.0 {
            self.app_state.config.scale_factor = 2.0;
            self.app_state.config.doublesize = true;
        } else {
            self.app_state.config.scale_factor = 1.0;
            self.app_state.config.doublesize = false;
        }
    }

    pub(crate) fn double_size(&self) -> bool {
        self.app_state.config.doublesize
    }

    pub(crate) fn show_current_file_info(&mut self) {
        self.last_playlist_file_info = self
            .app_state
            .playlist
            .position()
            .and_then(|position| self.app_state.playlist.entries().get(position))
            .or_else(|| self.app_state.playlist.entries().first())
            .map(|entry| entry.title.clone());
    }

    pub(crate) fn play_first_playlist_entry(&mut self) {
        if !self.app_state.playlist.is_empty() {
            self.app_state.playlist.set_position(0);
            self.app_state.player.mark_playing();
        }
    }

    pub(crate) fn set_spotify_authenticated(&mut self, authenticated: bool) {
        self.spotify_authenticated = authenticated;
    }

    pub(crate) fn open_spotify_window(&mut self) {
        self.spotify_auth_prompt_visible = false;
        if !self.spotify_authenticated {
            self.spotify_auth_prompt_visible = true;
            self.spotify_window_visible = false;
            self.spotify_status = "Authentication required".to_string();
            return;
        }
        self.spotify_window_visible = true;
        self.spotify_page = SpotifyChooserPage::Playlists;
        self.spotify_status = "Loading playlists...".to_string();
    }

    pub(crate) fn spotify_window_visible(&self) -> bool {
        self.spotify_window_visible
    }

    pub(crate) fn spotify_auth_prompt_visible(&self) -> bool {
        self.spotify_auth_prompt_visible
    }

    pub(crate) fn spotify_page(&self) -> SpotifyChooserPage {
        self.spotify_page
    }

    pub(crate) fn spotify_status(&self) -> &str {
        &self.spotify_status
    }

    pub(crate) fn spotify_playlist_names(&self) -> Vec<&str> {
        self.spotify_playlists
            .iter()
            .map(|playlist| playlist.name.as_str())
            .collect()
    }

    pub(crate) fn spotify_track_titles(&self) -> Vec<String> {
        self.spotify_tracks
            .iter()
            .enumerate()
            .map(|(index, track)| {
                format!(
                    "{}. {} - {}",
                    index + 1,
                    track.artist.as_deref().unwrap_or("Unknown"),
                    track.name
                )
            })
            .collect()
    }

    pub(crate) fn set_spotify_playlists(&mut self, playlists: Vec<SpotifyPlaylist>) {
        let count = playlists.len();
        self.spotify_playlists = playlists;
        self.spotify_page = SpotifyChooserPage::Playlists;
        self.spotify_status = format!("{count} playlists");
    }

    pub(crate) fn select_spotify_playlist(&mut self, index: usize) -> bool {
        let Some(playlist) = self.spotify_playlists.get(index) else {
            return false;
        };
        self.spotify_current_playlist_uri = Some(playlist.uri.clone());
        self.spotify_last_track_request = Some(playlist.id.clone());
        self.spotify_status = "Loading tracks...".to_string();
        true
    }

    pub(crate) fn spotify_last_track_request(&self) -> Option<&str> {
        self.spotify_last_track_request.as_deref()
    }

    pub(crate) fn set_spotify_tracks(&mut self, tracks: Vec<SpotifyTrack>) {
        let count = tracks.len();
        self.spotify_tracks = tracks;
        self.spotify_page = SpotifyChooserPage::Tracks;
        self.spotify_status = format!("{count} tracks");
    }

    pub(crate) fn show_spotify_playlists_page(&mut self) {
        self.spotify_page = SpotifyChooserPage::Playlists;
    }

    pub(crate) fn set_spotify_error(&mut self, message: impl Into<String>) {
        self.spotify_status = message.into();
    }

    pub(crate) fn load_spotify_tracks_into_playlist(&mut self) -> bool {
        if self.spotify_tracks.is_empty() {
            return false;
        }
        self.app_state.playlist.clear();
        for track in &self.spotify_tracks {
            let title = format!(
                "{} - {}",
                track.artist.as_deref().unwrap_or("Unknown"),
                track.name
            );
            self.app_state
                .playlist
                .add_spotify(&track.uri, title, i64::from(track.duration_ms));
        }
        self.app_state.playlist.set_position(0);
        self.spotify_window_visible = false;
        true
    }

    pub(crate) fn close_spotify_window(&mut self) {
        self.spotify_window_visible = false;
    }

    pub(crate) fn is_file_dialog_visible(&self) -> bool {
        self.file_dialog_visible
    }

    pub(crate) fn set_file_dialog_visible(&mut self, visible: bool) {
        self.file_dialog_visible = visible;
    }

    pub(crate) fn is_directory_dialog_visible(&self) -> bool {
        self.directory_dialog_visible
    }

    pub(crate) fn set_directory_dialog_visible(&mut self, visible: bool) {
        self.directory_dialog_visible = visible;
    }

    pub(crate) fn is_playlist_load_dialog_visible(&self) -> bool {
        self.playlist_load_dialog_visible
    }

    pub(crate) fn set_playlist_load_dialog_visible(&mut self, visible: bool) {
        self.playlist_load_dialog_visible = visible;
    }

    pub(crate) fn is_playlist_save_dialog_visible(&self) -> bool {
        self.playlist_save_dialog_visible
    }

    pub(crate) fn set_playlist_save_dialog_visible(&mut self, visible: bool) {
        self.playlist_save_dialog_visible = visible;
    }

    pub(crate) fn last_playlist_file_info(&self) -> Option<&str> {
        self.last_playlist_file_info.as_deref()
    }

    pub(crate) fn playlist_options_opened(&self) -> bool {
        self.playlist_options_opened
    }

    pub(crate) fn load_playlist_file(&mut self, path: &Path) -> std::io::Result<()> {
        self.app_state.playlist = Playlist::load_m3u_file(path)?;
        self.playlist_scroll_offset = 0;
        self.playlist_search_active = false;
        self.playlist_search_query.clear();
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
        self.playlist_scroll_offset
            .checked_add(row)
            .and_then(|index| self.playlist_entry_uri(index))
    }

    pub(crate) fn visible_playlist_entry_title(&self, row: usize) -> Option<String> {
        self.playlist_scroll_offset
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
            self.seek_state
                .play_start_position_ms(self.playback_position_ms),
        );
    }

    fn start_current_playlist_playback_from_beginning(&mut self) {
        self.start_current_playlist_playback_at(0);
    }

    fn start_current_playlist_playback_at(&mut self, position_ms: i64) {
        self.seek_state = SeekState::Idle;
        self.stop_fade_remaining_ms = 0;
        self.set_runtime_volume(self.app_state.config.volume);
        if self.app_state.playlist.position().is_none() && self.app_state.playlist.len() > 0 {
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
        if uri.starts_with("spotify:") {
            let duration_ms = self.playlist_entry_length_ms(position).unwrap_or(0);
            self.app_state.player.play_spotify_uri(uri, duration_ms);
            if self.playback_position_ms > 0 {
                self.app_state.player.apply_spotify_playback_state(
                    true,
                    self.playback_position_ms,
                    duration_ms,
                );
            }
        } else {
            self.load_equalizer_auto_preset_for_uri(&uri);
            self.playback_requests.push(uri.clone());
            if let Some(backend) = &self.playback_backend {
                if let Err(err) = backend.borrow().play_uri(&uri) {
                    eprintln!("xmms-rs: failed to play {uri}: {err}");
                    self.app_state.player.stop();
                    return;
                }
                if self.playback_position_ms > 0 {
                    self.seek_state = SeekState::PendingBackendSeek(self.playback_position_ms);
                }
            }
            self.app_state.player.mark_playing();
        }
    }

    fn load_equalizer_auto_preset_for_uri(&mut self, uri: &str) {
        if !self.equalizer_automatic {
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

    fn stop_playback(&mut self) {
        self.seek_state = SeekState::Idle;
        self.stop_fade_remaining_ms = 0;
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

    pub(crate) fn stop_with_fade(&mut self) {
        if self.app_state.player.state() == PlayerState::Stopped {
            self.stop_playback();
            return;
        }
        self.seek_state = SeekState::Idle;
        self.stop_fade_start_volume = self.app_state.player.volume().max(0);
        if self.stop_fade_start_volume == 0 {
            self.stop_playback();
            self.set_runtime_volume(self.app_state.config.volume);
            return;
        }
        self.stop_fade_remaining_ms = STOP_FADE_DURATION_MS;
    }

    fn set_runtime_volume(&mut self, volume: i32) {
        let volume = volume.clamp(0, 100);
        self.app_state.player.set_volume(volume);
        if let Some(backend) = &self.playback_backend {
            backend.borrow().set_volume_percent(volume);
        }
    }

    pub(crate) fn player_spotify_mode(&self) -> bool {
        self.app_state.player.spotify_mode()
    }

    pub(crate) fn player_spotify_uri(&self) -> Option<&str> {
        self.app_state.player.spotify_uri()
    }

    pub(crate) fn player_spotify_position_ms(&self) -> i64 {
        self.app_state.player.spotify_position_ms()
    }

    pub(crate) fn playback_position_ms(&self) -> i64 {
        self.playback_position_ms
    }

    pub(crate) fn last_playback_request(&self) -> Option<&str> {
        self.playback_requests.last().map(String::as_str)
    }

    pub(crate) fn player_spotify_duration_ms(&self) -> i64 {
        self.app_state.player.spotify_duration_ms()
    }

    pub(crate) fn add_spotify_entry(&mut self, uri: &str, title: &str, duration_ms: i64) {
        self.app_state.playlist.add_spotify(uri, title, duration_ms);
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
        self.playlist_menu = None;
        self.playlist_menu_hover = None;
        self.playlist_menu_pressed = false;
        self.playlist_search_active = true;
        self.playlist_search_query.clear();
        true
    }

    pub(crate) fn stop_playlist_search(&mut self) {
        self.playlist_search_active = false;
        self.playlist_search_query.clear();
    }

    pub(crate) fn push_playlist_search_char(&mut self, ch: char) {
        if !self.playlist_search_active || ch.is_control() {
            return;
        }
        self.playlist_search_query.push(ch);
        self.update_playlist_search_match();
    }

    pub(crate) fn pop_playlist_search_char(&mut self) {
        if !self.playlist_search_active {
            return;
        }
        self.playlist_search_query.pop();
        self.update_playlist_search_match();
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
        self.open_location_visible = false;
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
        self.jump_time_visible = false;
    }

    pub(crate) fn set_playlist_size(&mut self, width: i32, height: i32) -> bool {
        let size = snap_playlist_size(width, height);
        let (width, height) = (size.width, size.height);
        let changed = self.playlist_width != width || self.playlist_height != height;
        self.playlist_width = width;
        self.playlist_height = height;
        self.clamp_playlist_scroll_offset();
        changed
    }

    pub(crate) fn set_panel_dragging(&mut self, kind: PanelKind, dragging: bool) {
        match kind {
            PanelKind::Equalizer => self.equalizer_dragging_title = dragging,
            PanelKind::Playlist => self.playlist_dragging_title = dragging,
        }
    }

    pub(crate) fn set_panel_focused(&mut self, kind: PanelKind, focused: bool) {
        match kind {
            PanelKind::Equalizer => self.equalizer_focused = focused,
            PanelKind::Playlist => self.playlist_focused = focused,
        }
    }

    pub(crate) fn is_panel_focused(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Equalizer => self.equalizer_focused,
            PanelKind::Playlist => self.playlist_focused,
        }
    }

    pub(crate) fn equalizer_active(&self) -> bool {
        self.equalizer_active
    }

    pub(crate) fn equalizer_automatic(&self) -> bool {
        self.equalizer_automatic
    }

    pub(crate) fn equalizer_preamp_position(&self) -> i32 {
        self.equalizer_preamp_position
    }

    pub(crate) fn equalizer_band_position(&self, band: usize) -> Option<i32> {
        self.equalizer_band_positions.get(band).copied()
    }

    pub(crate) fn equalizer_preamp_db(&self) -> f64 {
        equalizer_position_to_db(self.equalizer_preamp_position)
    }

    pub(crate) fn equalizer_band_db(&self, band: usize) -> Option<f64> {
        self.equalizer_band_positions
            .get(band)
            .map(|position| equalizer_position_to_db(*position))
    }

    pub(crate) fn equalizer_gstreamer_band_db_values(&self) -> [f64; 10] {
        if self.equalizer_active {
            self.equalizer_band_positions.map(equalizer_position_to_db)
        } else {
            [0.0; 10]
        }
    }

    pub(crate) fn equalizer_presets_pressed(&self) -> bool {
        self.equalizer_pressed_control == Some(EqualizerControl::Presets)
            && self.equalizer_pressed_inside
    }

    pub(crate) fn equalizer_press(&mut self, x: i32, y: i32) -> bool {
        if self.equalizer_shaded {
            if let Some(slider) = equalizer_shaded_slider_at(x, y) {
                self.equalizer_dragging = Some(slider);
                self.begin_equalizer_slider_drag(slider, x, y);
                return true;
            }
            return false;
        }

        if let Some(control) = equalizer_control_at(x, y) {
            self.equalizer_pressed_control = Some(control);
            self.equalizer_pressed_inside = true;
            return true;
        }

        if let Some(slider) = equalizer_slider_at(x, y) {
            self.equalizer_dragging = Some(slider);
            self.begin_equalizer_slider_drag(slider, x, y);
            return true;
        }

        false
    }

    pub(crate) fn equalizer_motion(&mut self, x: i32, y: i32) -> bool {
        if let Some(control) = self.equalizer_pressed_control {
            let inside = equalizer_control_at(x, y) == Some(control);
            let changed = self.equalizer_pressed_inside != inside;
            self.equalizer_pressed_inside = inside;
            return changed;
        }

        let Some(slider) = self.equalizer_dragging else {
            return false;
        };
        let coordinate = match slider {
            EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => x,
            EqualizerSlider::Preamp | EqualizerSlider::Band(_) => y,
        };
        self.set_equalizer_slider_position(slider, coordinate)
    }

    pub(crate) fn equalizer_scroll(&mut self, x: i32, y: i32, dy: f64) -> bool {
        let slider = if self.equalizer_shaded {
            equalizer_shaded_slider_at(x, y)
        } else {
            equalizer_slider_at(x, y)
        };
        let Some(slider) = slider else {
            return false;
        };
        let diff = if dy < 0.0 {
            -4
        } else if dy > 0.0 {
            4
        } else {
            return false;
        };
        match slider {
            EqualizerSlider::Preamp => {
                let next = (self.equalizer_preamp_position + diff).clamp(0, 100);
                let changed = self.equalizer_preamp_position != next;
                self.equalizer_preamp_position = next;
                if changed {
                    self.sync_equalizer_to_backend();
                }
                changed
            }
            EqualizerSlider::Band(band) => {
                let Some(value) = self.equalizer_band_positions.get_mut(band) else {
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
            EqualizerSlider::ShadedVolume => self.scroll_volume(dy),
            EqualizerSlider::ShadedBalance => self.scroll_balance(dy),
        }
    }

    pub(crate) fn adjust_shaded_equalizer_balance(&mut self, diff: i32) -> bool {
        if !self.equalizer_shaded {
            return false;
        }
        let balance = (self.app_state.player.balance() + diff).clamp(-100, 100);
        let changed = self.app_state.player.balance() != balance;
        self.app_state.player.set_balance(balance);
        if changed {
            if let Some(backend) = &self.playback_backend {
                backend.borrow().set_balance_percent(balance);
            }
        }
        changed
    }

    pub(crate) fn equalizer_release(&mut self, x: i32, y: i32) -> PanelAction {
        if let Some(control) = self.equalizer_pressed_control.take() {
            let activated =
                self.equalizer_pressed_inside && equalizer_control_at(x, y) == Some(control);
            self.equalizer_pressed_inside = false;
            if activated {
                match control {
                    EqualizerControl::On => {
                        self.equalizer_active = !self.equalizer_active;
                        self.sync_equalizer_to_backend();
                    }
                    EqualizerControl::Auto => self.equalizer_automatic = !self.equalizer_automatic,
                    EqualizerControl::Presets => return PanelAction::ShowEqualizerPresets,
                }
            }
            return PanelAction::Changed;
        }

        if self.equalizer_dragging.take().is_some() {
            return PanelAction::Changed;
        }

        PanelAction::None
    }

    pub(crate) fn apply_equalizer_preset(&mut self, preset: i32) {
        self.equalizer_preamp_position = 50;
        self.equalizer_band_positions = [50; 10];
        match preset {
            1 => {
                self.equalizer_band_positions[0] = 25;
                self.equalizer_band_positions[1] = 30;
                self.equalizer_band_positions[2] = 40;
            }
            2 => {
                self.equalizer_band_positions[7] = 40;
                self.equalizer_band_positions[8] = 30;
                self.equalizer_band_positions[9] = 25;
            }
            3 => {
                self.equalizer_band_positions[0] = 30;
                self.equalizer_band_positions[1] = 35;
                self.equalizer_band_positions[4] = 60;
                self.equalizer_band_positions[5] = 60;
                self.equalizer_band_positions[8] = 35;
                self.equalizer_band_positions[9] = 30;
            }
            _ => {}
        }
        self.sync_equalizer_to_backend();
    }

    fn set_equalizer_slider_position(&mut self, slider: EqualizerSlider, coordinate: i32) -> bool {
        let changed = match slider {
            EqualizerSlider::Preamp => {
                let position = eq_slider_pixel_to_position(
                    coordinate
                        - equalizer_slider_layout(slider).rect.y
                        - self.equalizer_slider_press_offset,
                );
                let changed = self.equalizer_preamp_position != position;
                self.equalizer_preamp_position = position;
                changed
            }
            EqualizerSlider::Band(band) => {
                let position = eq_slider_pixel_to_position(
                    coordinate
                        - equalizer_slider_layout(slider).rect.y
                        - self.equalizer_slider_press_offset,
                );
                let Some(value) = self.equalizer_band_positions.get_mut(band) else {
                    return false;
                };
                let changed = *value != position;
                *value = position;
                changed
            }
            EqualizerSlider::ShadedVolume => {
                let position = (coordinate
                    - equalizer_slider_layout(slider).rect.x
                    - self.equalizer_slider_press_offset)
                    .clamp(0, 94);
                let volume = eq_shaded_position_to_volume(position);
                let changed = self.app_state.player.volume() != volume;
                self.app_state.player.set_volume(volume);
                changed
            }
            EqualizerSlider::ShadedBalance => {
                let position = (coordinate
                    - equalizer_slider_layout(slider).rect.x
                    - self.equalizer_slider_press_offset)
                    .clamp(0, 39);
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

    fn begin_equalizer_slider_drag(&mut self, slider: EqualizerSlider, x: i32, y: i32) {
        let layout = equalizer_slider_layout(slider);
        match slider {
            EqualizerSlider::Preamp | EqualizerSlider::Band(_) => {
                let position = self.equalizer_slider_pixel_position(slider);
                let local_y = y - layout.rect.y;
                if local_y >= position && local_y < position + 11 {
                    self.equalizer_slider_press_offset = local_y - position;
                } else {
                    self.equalizer_slider_press_offset = 5;
                    self.set_equalizer_slider_position(slider, y);
                }
            }
            EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => {
                let position = self.equalizer_slider_pixel_position(slider);
                let local_x = x - layout.rect.x;
                if local_x >= position && local_x < position + layout.knob_size.width {
                    self.equalizer_slider_press_offset = local_x - position;
                } else {
                    self.equalizer_slider_press_offset = layout.knob_size.width / 2;
                    self.set_equalizer_slider_position(slider, x);
                }
            }
        }
    }

    fn equalizer_slider_pixel_position(&self, slider: EqualizerSlider) -> i32 {
        match slider {
            EqualizerSlider::Preamp => eq_slider_position_to_pixel(self.equalizer_preamp_position),
            EqualizerSlider::Band(band) => self
                .equalizer_band_positions
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
                self.equalizer_active,
                self.equalizer_preamp_position,
                self.equalizer_band_positions,
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
                && self.equalizer_shaded
                && equalizer_shaded_slider_at(x, y).is_some())
    }

    pub(crate) fn main_title_drag_region(&self, x: i32, y: i32) -> bool {
        y >= 0 && y < MAIN_TITLEBAR_HEIGHT && self.hit_test(x, y).is_none()
    }

    pub(crate) fn playlist_resize_region(&self, x: i32, y: i32) -> bool {
        !self.playlist_shaded && x > self.playlist_width - 20 && y > self.playlist_height - 20
    }

    pub(crate) fn begin_docked_playlist_resize(&mut self, local_y: i32) -> bool {
        if !self.playlist_resize_region(self.playlist_width - 1, local_y) {
            return false;
        }
        self.playlist_docked_resizing = true;
        self.playlist_resize_drag_offset_y = self.playlist_height - local_y;
        true
    }

    pub(crate) fn docked_playlist_resize_motion(&mut self, main_y: i32) -> bool {
        if !self.playlist_docked_resizing {
            return false;
        }
        let Some(local_y) = self.docked_playlist_local_y(main_y) else {
            return false;
        };
        let height = local_y + self.playlist_resize_drag_offset_y;
        self.set_playlist_size(PLAYLIST_MIN_WIDTH, height)
    }

    pub(crate) fn end_docked_playlist_resize(&mut self) -> bool {
        let was_resizing = self.playlist_docked_resizing;
        self.playlist_docked_resizing = false;
        self.playlist_resize_drag_offset_y = 0;
        was_resizing
    }

    pub(crate) fn is_docked_playlist_resizing(&self) -> bool {
        self.playlist_docked_resizing
    }

    pub(crate) fn playlist_scrollbar_press(&mut self, x: i32, y: i32) -> bool {
        let Some((thumb_y, thumb_h)) = self.playlist_scrollbar_geometry() else {
            return false;
        };
        if !self.playlist_scrollbar_region(x, y) {
            return false;
        }
        self.playlist_scrollbar_dragging = true;
        self.playlist_scrollbar_drag_offset = if y >= thumb_y && y < thumb_y + thumb_h {
            y - thumb_y
        } else {
            thumb_h / 2
        };
        self.update_playlist_scroll_from_thumb_y(y - self.playlist_scrollbar_drag_offset);
        true
    }

    pub(crate) fn playlist_scrollbar_motion(&mut self, x: i32, y: i32) -> bool {
        if !self.playlist_scrollbar_dragging {
            return false;
        }
        let old = self.playlist_scroll_offset;
        let _ = x;
        self.update_playlist_scroll_from_thumb_y(y - self.playlist_scrollbar_drag_offset);
        old != self.playlist_scroll_offset
    }

    pub(crate) fn playlist_scrollbar_release(&mut self) -> bool {
        let was_dragging = self.playlist_scrollbar_dragging;
        self.playlist_scrollbar_dragging = false;
        self.playlist_scrollbar_drag_offset = 0;
        was_dragging
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
        let old = self.playlist_scroll_offset;
        if rows < 0 {
            self.playlist_scroll_offset = self
                .playlist_scroll_offset
                .saturating_sub(rows.unsigned_abs() as usize);
        } else {
            self.playlist_scroll_offset = self.playlist_scroll_offset.saturating_add(rows as usize);
            self.clamp_playlist_scroll_offset();
        }
        old != self.playlist_scroll_offset
    }

    pub(crate) fn playlist_press(&mut self, x: i32, y: i32) -> bool {
        self.playlist_press_with_ctrl(x, y, false)
    }

    pub(crate) fn playlist_press_with_ctrl(&mut self, x: i32, y: i32, ctrl_pressed: bool) -> bool {
        if let Some(item) = self.playlist_menu_item_at(x, y) {
            self.playlist_menu_hover = Some(item);
            self.playlist_menu_pressed = true;
            return true;
        }
        if self.playlist_menu.is_some() {
            return false;
        }

        let Some(index) = self.playlist_entry_at(x, y) else {
            return false;
        };
        if ctrl_pressed {
            if let Some(entry) = self.app_state.playlist.entries_mut().get_mut(index) {
                entry.selected = !entry.selected;
            }
            self.playlist_last_click = None;
            self.playlist_pending_double_click = None;
            self.playlist_drag_index = None;
            self.playlist_drag_moved = false;
            return true;
        }

        let now = Instant::now();
        let is_double_click = self
            .playlist_last_click
            .is_some_and(|(last_index, last_time)| {
                last_index == index && now.duration_since(last_time) <= Duration::from_millis(500)
            });

        self.playlist_last_click = Some((index, now));
        self.playlist_pending_double_click = is_double_click.then_some(index);
        self.playlist_drag_moved = false;
        self.select_single_playlist_entry(index);
        self.playlist_drag_index = Some(index);
        true
    }

    pub(crate) fn activate_playlist_entry_at(&mut self, x: i32, y: i32) -> bool {
        if self.playlist_menu.is_some() {
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
        self.playlist_last_click = None;
        self.playlist_pending_double_click = None;
        self.playlist_drag_index = None;
        self.playlist_drag_moved = false;
        self.app_state.playlist.set_position(index);
        self.start_current_playlist_playback_from_beginning();
    }

    pub(crate) fn playlist_motion(&mut self, x: i32, y: i32) -> bool {
        if let Some(from) = self.playlist_drag_index {
            let Some(to) = self.playlist_entry_at(x, y) else {
                return false;
            };
            if self.app_state.playlist.move_entry(from, to) {
                self.playlist_drag_moved = true;
                self.playlist_pending_double_click = None;
                self.playlist_drag_index = Some(to);
                self.scroll_playlist_entry_into_view(to);
                return true;
            }
            return false;
        }

        if self.playlist_menu.is_none() {
            return false;
        }
        let item = self.playlist_menu_item_at(x, y);
        let changed = self.playlist_menu_hover != item;
        self.playlist_menu_hover = item;
        changed
    }

    pub(crate) fn playlist_entry_release(&mut self) -> bool {
        let was_pressed = self.playlist_drag_index.take().is_some();
        if was_pressed {
            if let Some(index) = self.playlist_pending_double_click.take() {
                if !self.playlist_drag_moved {
                    self.activate_playlist_entry(index);
                }
            }
        }
        self.playlist_drag_moved = false;
        was_pressed
    }

    pub(crate) fn playlist_release(&mut self, x: i32, y: i32) -> PanelAction {
        let menu = self.playlist_menu;
        let item = self.playlist_menu_item_at(x, y);
        let activated = item == self.playlist_menu_hover;
        self.playlist_menu = None;
        self.playlist_menu_hover = None;
        self.playlist_menu_pressed = false;
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
        let changed = match (menu, item) {
            (PlaylistMenuKind::Add, 0) => return PanelAction::OpenLocationWindow,
            (PlaylistMenuKind::Add, 1) => return PanelAction::OpenDirectoryDialog,
            (PlaylistMenuKind::Add, 2) => return PanelAction::OpenFileDialog,
            (PlaylistMenuKind::Misc, 0) => return PanelAction::ShowPlaylistSortMenu,
            (PlaylistMenuKind::Misc, 1) => {
                self.last_playlist_file_info = self
                    .selected_playlist_index()
                    .or_else(|| self.app_state.playlist.position())
                    .and_then(|index| self.app_state.playlist.entries().get(index))
                    .map(|entry| entry.title.clone());
                true
            }
            (PlaylistMenuKind::Misc, 2) => {
                self.playlist_options_opened = true;
                true
            }
            (PlaylistMenuKind::Remove, 1) => {
                self.app_state.playlist.clear();
                true
            }
            (PlaylistMenuKind::Remove, 2) => self.app_state.playlist.crop_to_selected_or_current(),
            (PlaylistMenuKind::Remove, 3) => self.app_state.playlist.remove_selected_or_current(),
            (PlaylistMenuKind::Select, 0) => {
                self.app_state.playlist.invert_selection();
                true
            }
            (PlaylistMenuKind::Select, 1) => {
                self.app_state.playlist.select_all(false);
                true
            }
            (PlaylistMenuKind::Select, 2) => {
                self.app_state.playlist.select_all(true);
                true
            }
            (PlaylistMenuKind::List, 0) => {
                self.app_state.playlist.clear();
                true
            }
            (PlaylistMenuKind::List, 1) => return PanelAction::OpenPlaylistSaveDialog,
            (PlaylistMenuKind::List, 2) => return PanelAction::OpenPlaylistLoadDialog,
            _ => false,
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
        if self.playlist_search_query.is_empty() {
            return;
        }
        let total = self.app_state.playlist.len();
        if total == 0 {
            return;
        }
        let query = self.playlist_search_query.to_lowercase();
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

    pub(crate) fn play_selected_playlist_entry(&mut self) -> bool {
        if !self.app_state.config.vim_playlist_navigation {
            return false;
        }
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
        if index < self.playlist_scroll_offset {
            self.playlist_scroll_offset = index;
        } else if index >= self.playlist_scroll_offset + visible {
            self.playlist_scroll_offset = index + 1 - visible;
        }
        self.clamp_playlist_scroll_offset();
    }

    fn playlist_visible_entries(&self) -> usize {
        ((self.playlist_height - 58).max(0) / 11) as usize
    }

    fn playlist_entry_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.playlist_shaded || !(12..self.playlist_width - 19).contains(&x) {
            return None;
        }
        if !(20..self.playlist_height - 38).contains(&y) {
            return None;
        }
        let row = ((y - 20) / 11) as usize;
        if row >= self.playlist_visible_entries() {
            return None;
        }
        let index = self.playlist_scroll_offset + row;
        (index < self.app_state.playlist.len()).then_some(index)
    }

    fn playlist_max_scroll(&self) -> usize {
        self.app_state
            .playlist
            .len()
            .saturating_sub(self.playlist_visible_entries())
    }

    fn clamp_playlist_scroll_offset(&mut self) {
        self.playlist_scroll_offset = self.playlist_scroll_offset.min(self.playlist_max_scroll());
    }

    fn playlist_scrollbar_region(&self, x: i32, y: i32) -> bool {
        !self.playlist_shaded
            && x >= self.playlist_width - 15
            && x < self.playlist_width - 7
            && y >= 20
            && y < self.playlist_height - 38
    }

    fn playlist_scrollbar_geometry(&self) -> Option<(i32, i32)> {
        let visible = self.playlist_visible_entries();
        let total = self.app_state.playlist.len();
        if total <= visible || visible == 0 {
            return None;
        }
        let list_h = self.playlist_height - 58;
        let thumb_h = 18;
        let max_scroll = total - visible;
        let max_thumb_pos = (list_h - thumb_h).max(0);
        let thumb_y = 20
            + ((self.playlist_scroll_offset.min(max_scroll) as i32 * max_thumb_pos)
                / max_scroll.max(1) as i32);
        Some((thumb_y, thumb_h))
    }

    fn update_playlist_scroll_from_thumb_y(&mut self, thumb_y: i32) {
        let visible = self.playlist_visible_entries();
        let total = self.app_state.playlist.len();
        if total <= visible || visible == 0 {
            self.playlist_scroll_offset = 0;
            return;
        }
        let list_h = self.playlist_height - 58;
        let thumb_h = 18;
        let max_scroll = total - visible;
        let max_thumb_pos = (list_h - thumb_h).max(0);
        if max_thumb_pos <= 0 {
            self.playlist_scroll_offset = 0;
            return;
        }
        let thumb_pos = (thumb_y - 20).clamp(0, max_thumb_pos);
        self.playlist_scroll_offset = ((thumb_pos as usize * max_scroll)
            + (max_thumb_pos as usize / 2))
            / max_thumb_pos as usize;
    }

    fn playlist_menu_item_at(&self, x: i32, y: i32) -> Option<usize> {
        let menu = self.playlist_menu?;
        let (menu_x, menu_y, menu_width, menu_height) =
            playlist_menu_rect(menu, self.playlist_width, self.playlist_height);
        if x < menu_x || x >= menu_x + menu_width || y < menu_y || y >= menu_y + menu_height {
            return None;
        }
        Some(((y - menu_y) / 18) as usize)
    }

    pub(crate) fn panel_click(&mut self, kind: PanelKind, x: i32, y: i32) -> PanelAction {
        if kind == PanelKind::Playlist {
            self.playlist_menu = None;
            self.playlist_menu_hover = None;
            self.playlist_menu_pressed = false;
            self.playlist_drag_index = None;
        }

        if self.panel_title_button_hit(kind, x, y) {
            if self.panel_close_button_hit(kind, x) {
                match kind {
                    PanelKind::Equalizer => self.app_state.config.equalizer_visible = false,
                    PanelKind::Playlist => self.app_state.config.playlist_visible = false,
                }
                return PanelAction::Changed;
            }

            if self.panel_shade_button_hit(kind, x) {
                match kind {
                    PanelKind::Equalizer => self.equalizer_shaded = !self.equalizer_shaded,
                    PanelKind::Playlist => self.playlist_shaded = !self.playlist_shaded,
                }
                return PanelAction::Changed;
            }
        }

        if kind == PanelKind::Playlist && !self.playlist_shaded {
            if let Some(menu) = playlist_menu_at(x, y, self.playlist_width, self.playlist_height) {
                self.playlist_menu = Some(menu);
                self.playlist_menu_hover = Some(menu.item_count().saturating_sub(1));
                return PanelAction::ShowPlaylistMenu(menu);
            }
            if let Some(button) =
                playlist_footer_button_at(x, y, self.playlist_width, self.playlist_height)
            {
                return self.activate_playlist_footer_button(button);
            }
        }

        PanelAction::None
    }

    fn activate_playlist_footer_button(&mut self, button: PlaylistFooterButton) -> PanelAction {
        match button {
            PlaylistFooterButton::Previous => {
                if self.app_state.playlist.previous() {
                    self.start_current_playlist_playback_from_beginning();
                }
                self.position_position = 0;
                self.playback_position_ms = 0;
                PanelAction::Changed
            }
            PlaylistFooterButton::Play => {
                match self.app_state.player.state() {
                    PlayerState::Paused => self.unpause_playback(),
                    PlayerState::Stopped => self.start_current_playlist_playback(),
                    PlayerState::Playing => {}
                }
                PanelAction::Changed
            }
            PlaylistFooterButton::Pause => {
                match self.app_state.player.state() {
                    PlayerState::Playing => self.pause_playback(),
                    PlayerState::Paused => self.unpause_playback(),
                    PlayerState::Stopped => {}
                }
                PanelAction::Changed
            }
            PlaylistFooterButton::Stop => {
                self.stop_playback();
                PanelAction::Changed
            }
            PlaylistFooterButton::Next => {
                if self.app_state.playlist.next() {
                    self.start_current_playlist_playback_from_beginning();
                }
                self.position_position = 0;
                self.playback_position_ms = 0;
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
        panel_title_button_at(panel_layout_kind(kind), x, y, self.playlist_width).is_some()
    }

    fn panel_shade_button_hit(&self, kind: PanelKind, x: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, 7, self.playlist_width)
            == Some(PanelTitleButton::Shade)
    }

    fn panel_close_button_hit(&self, kind: PanelKind, x: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, 7, self.playlist_width)
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

    pub(crate) fn toggle_playlist_shaded(&mut self) {
        self.playlist_shaded = !self.playlist_shaded;
    }

    pub(crate) fn toggle_equalizer_shaded(&mut self) {
        self.equalizer_shaded = !self.equalizer_shaded;
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
            self.seek_state = SeekState::Idle;
        }
        self.mark_preferences_saved();
    }

    pub(crate) fn preference_pause_between_songs(&self) -> bool {
        self.app_state.config.pause_between_songs
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
        self.app_state.config.playlist_detached = !docked;
        if docked {
            self.playlist_width = PLAYLIST_MIN_WIDTH;
            self.clamp_playlist_scroll_offset();
        }
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_equalizer_docked(&mut self, docked: bool) {
        self.app_state.config.equalizer_detached = !docked;
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
        if self.app_state.player.state() != PlayerState::Stopped {
            if self.app_state.player.spotify_mode() {
                let duration_ms = self.current_duration_ms().unwrap_or(0);
                self.app_state.player.apply_spotify_playback_state(
                    self.app_state.player.state() == PlayerState::Playing,
                    self.playback_position_ms,
                    duration_ms,
                );
                return;
            }
        } else {
            self.seek_state = if self.playback_position_ms > 0 {
                SeekState::StoppedAt(self.playback_position_ms)
            } else {
                SeekState::Idle
            };
            return;
        }
        if self.seek_state.pending_backend_seek_ms().is_some() {
            self.seek_state = SeekState::PendingBackendSeek(self.playback_position_ms);
            return;
        }
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().seek_to_ms(self.playback_position_ms) {
                eprintln!("xmms-rs: failed to seek playback: {err}");
            }
        }
        if self.app_state.player.spotify_mode() {
            self.app_state.player.apply_spotify_playback_state(
                self.app_state.player.state() == PlayerState::Playing,
                self.playback_position_ms,
                duration.unwrap_or(0),
            );
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

        if self
            .app_state
            .player
            .tick_spotify_playback(i64::from(elapsed_ms))
        {
            self.spotify_playback_poll_requests =
                self.spotify_playback_poll_requests.saturating_add(1);
        }
        if self.playback_backend.is_none() || self.app_state.player.spotify_mode() {
            self.playback_position_ms = self
                .playback_position_ms
                .saturating_add(i64::from(elapsed_ms));
            if self.app_state.player.spotify_mode() {
                self.playback_position_ms = self.app_state.player.spotify_position_ms();
            }
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
        if self.stop_fade_remaining_ms <= 0 {
            return false;
        }
        self.stop_fade_remaining_ms = (self.stop_fade_remaining_ms - i64::from(elapsed_ms)).max(0);
        if self.stop_fade_remaining_ms == 0 {
            let restore_volume = self.app_state.config.volume;
            self.stop_playback();
            self.set_runtime_volume(restore_volume);
            return true;
        }
        let volume = ((i64::from(self.stop_fade_start_volume) * self.stop_fade_remaining_ms)
            / STOP_FADE_DURATION_MS)
            .clamp(0, 100) as i32;
        self.set_runtime_volume(volume);
        true
    }

    fn update_pending_eof_advance(&mut self, elapsed_ms: u32) -> bool {
        let Some(remaining) = self.seek_state.eof_pause_remaining_ms() else {
            return false;
        };
        let remaining = remaining - i64::from(elapsed_ms);
        if remaining > 0 {
            self.seek_state = SeekState::WaitingBetweenSongs {
                remaining_ms: remaining,
            };
            return true;
        }
        self.seek_state = SeekState::Idle;
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
        !applied_pending_seek && self.seek_state.eof_pause_remaining_ms().is_none()
    }

    fn apply_pending_backend_seek(
        &mut self,
        backend: &Rc<RefCell<GStreamerBackend>>,
        log_failure: bool,
    ) -> bool {
        let Some(position_ms) = self.seek_state.pending_backend_seek_ms() else {
            return false;
        };
        match backend.borrow().seek_to_ms(position_ms) {
            Ok(()) => {
                self.seek_state = SeekState::Idle;
                true
            }
            Err(err) => {
                if log_failure {
                    eprintln!("xmms-rs: failed to seek playback: {err}");
                    self.seek_state = SeekState::Idle;
                }
                false
            }
        }
    }

    pub(crate) fn playlist_eof_reached(&mut self) {
        self.stop_fade_remaining_ms = 0;
        self.position_position = 0;
        if self.app_state.config.pause_between_songs
            && self.app_state.config.pause_between_songs_time > 0
        {
            self.seek_state = SeekState::WaitingBetweenSongs {
                remaining_ms: i64::from(self.app_state.config.pause_between_songs_time) * 1_000,
            };
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
            self.active = None;
            self.active_inside = false;
            return;
        };
        self.active = Some(control);
        self.active_inside = true;

        if let MainControl::Slider(slider) = control {
            self.begin_slider_drag(slider, x);
        }
    }

    pub(crate) fn motion(&mut self, x: i32, y: i32) -> bool {
        let Some(active) = self.active else {
            return false;
        };

        match active {
            MainControl::Push(_) | MainControl::Toggle(_) => {
                let inside = self.control_rect(active).contains(x, y);
                let changed = self.active_inside != inside;
                self.active_inside = inside;
                changed
            }
            MainControl::Slider(slider) => {
                self.active_inside = self.control_rect(active).contains(x, y);
                self.set_slider_position(
                    slider,
                    x - self.slider_rect(slider).x - self.slider_press_offset,
                )
            }
        }
    }

    pub(crate) fn release(&mut self, x: i32, y: i32) -> UiAction {
        let Some(active) = self.active.take() else {
            self.active_inside = false;
            return UiAction::None;
        };

        let activated = self.control_rect(active).contains(x, y) && self.active_inside;
        self.active_inside = false;

        match active {
            MainControl::Push(button) if activated => self.activate_push(button),
            MainControl::Toggle(toggle) if activated => {
                self.activate_toggle(toggle);
                UiAction::None
            }
            MainControl::Slider(slider) => {
                self.set_slider_position(
                    slider,
                    x - self.slider_rect(slider).x - self.slider_press_offset,
                );
                UiAction::None
            }
            _ => UiAction::None,
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

    fn scroll_volume(&mut self, dy: f64) -> bool {
        let step = self.app_state.config.mouse_wheel_change.clamp(1, 100);
        let diff = if dy < 0.0 {
            step
        } else if dy > 0.0 {
            -step
        } else {
            return false;
        };
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
                self.start_current_playlist_playback();
                UiAction::None
            }
            MainPushButton::Pause => {
                match self.app_state.player.state() {
                    PlayerState::Playing => self.pause_playback(),
                    PlayerState::Paused => self.unpause_playback(),
                    PlayerState::Stopped => {}
                }
                UiAction::None
            }
            MainPushButton::Stop => {
                self.stop_playback();
                UiAction::None
            }
            MainPushButton::Previous => {
                if self.app_state.playlist.previous() {
                    self.start_current_playlist_playback_from_beginning();
                }
                self.position_position = 0;
                self.playback_position_ms = 0;
                UiAction::None
            }
            MainPushButton::Next => {
                if self.app_state.playlist.next() {
                    self.start_current_playlist_playback_from_beginning();
                }
                self.position_position = 0;
                self.playback_position_ms = 0;
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
                self.app_state.config.equalizer_visible = !self.app_state.config.equalizer_visible;
            }
            MainToggleButton::Playlist => {
                self.app_state.config.playlist_visible = !self.app_state.config.playlist_visible;
            }
        }
    }

    fn begin_slider_drag(&mut self, slider: MainSlider, x: i32) {
        let rect = self.slider_rect(slider);
        let knob_width = self.slider_knob_width(slider);
        let position = self.slider_position(slider);
        let knob_x = rect.x + position;
        if x >= knob_x && x < knob_x + knob_width {
            self.slider_press_offset = x - knob_x;
        } else {
            self.slider_press_offset = knob_width / 2;
            self.set_slider_position(slider, x - rect.x - self.slider_press_offset);
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
        match (self.active, self.active_inside) {
            (Some(MainControl::Push(button)), true) => Some(button),
            _ => None,
        }
    }

    fn pressed_toggle(&self) -> Option<MainToggleButton> {
        match (self.active, self.active_inside) {
            (Some(MainControl::Toggle(toggle)), true) => Some(toggle),
            _ => None,
        }
    }

    fn pressed_slider(&self) -> Option<MainSlider> {
        match self.active {
            Some(MainControl::Slider(slider)) => Some(slider),
            _ => None,
        }
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
            .add_spotify("spotify:track:test", "Test", 120_000);
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
            .add_spotify("spotify:track:test", "Test", 120_000);

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
            .add_spotify("spotify:track:test", "Test", 120_000);
        state.app_state.playlist.set_position(0);
        state.app_state.player.mark_playing();
        state.shaded = true;
        state.equalizer_shaded = true;

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

        state.playlist_shaded = true;
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
            state.seek_state,
            SeekState::WaitingBetweenSongs {
                remaining_ms: 2_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);

        assert!(state.update_timer_tick(1_000));
        assert_eq!(state.app_state.playlist.position(), Some(0));
        assert_eq!(
            state.seek_state,
            SeekState::WaitingBetweenSongs {
                remaining_ms: 1_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);

        assert!(state.update_timer_tick(1_000));
        assert_eq!(state.app_state.playlist.position(), Some(1));
        assert_eq!(state.seek_state, SeekState::Idle);
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
            .add_spotify("spotify:track:test", "Test", 120_000);
        state.app_state.playlist.set_position(0);

        state.playlist_eof_reached();
        assert!(state.update_timer_tick(1_000));
        assert_eq!(
            state.seek_state,
            SeekState::WaitingBetweenSongs {
                remaining_ms: 1_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);

        state.start_current_playlist_playback();

        assert_eq!(state.seek_state, SeekState::Idle);
        assert_eq!(state.playback_position_ms, 0);
        assert_eq!(state.app_state.player.spotify_position_ms(), 0);
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
            state.seek_state,
            SeekState::WaitingBetweenSongs {
                remaining_ms: 2_000
            }
        );
        assert_eq!(state.playback_position_ms, 0);
        assert!(!state.should_sync_backend_position(false));
        assert!(!state.should_sync_backend_position(true));
    }

    #[test]
    fn stop_with_fade_ramps_down_then_restores_volume() {
        let mut state = MainWindowUiState::from_app_state(AppState::from_config(Config {
            volume: 80,
            ..Config::default()
        }));
        state.app_state.player.mark_playing();

        state.stop_with_fade();
        assert!(state.stop_fade_remaining_ms > 0);

        assert!(state.update_timer_tick(500));
        assert_eq!(state.volume(), 40);
        assert!(state.update_timer_tick(500));
        assert_eq!(state.app_state.player.state(), PlayerState::Stopped);
        assert_eq!(state.volume(), 80);
        assert_eq!(state.stop_fade_remaining_ms, 0);
    }

    #[test]
    fn play_from_stopped_preserves_selected_position() {
        let mut state = MainWindowUiState::default();
        state
            .app_state
            .playlist
            .add_spotify("spotify:track:test", "Test", 120_000);
        state.set_playback_position_ms(42_000);

        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);

        assert_eq!(state.app_state.player.state(), PlayerState::Playing);
        assert_eq!(state.playback_position_ms, 42_000);
        assert_eq!(state.app_state.player.spotify_position_ms(), 42_000);
    }

    #[test]
    fn changing_to_next_track_starts_from_beginning() {
        let mut state = MainWindowUiState::default();
        state
            .app_state
            .playlist
            .add_spotify("spotify:track:one", "One", 120_000);
        state
            .app_state
            .playlist
            .add_spotify("spotify:track:two", "Two", 120_000);
        state.app_state.playlist.set_position(0);
        state.set_playback_position_ms(42_000);

        state.press(109, 90);
        assert_eq!(state.release(109, 90), UiAction::None);

        assert_eq!(state.app_state.playlist.position(), Some(1));
        assert_eq!(state.playback_position_ms, 0);
        assert_eq!(state.app_state.player.spotify_position_ms(), 0);
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
    fn xmms_menu_css_uses_playlist_skin_colors() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let colors = skin.playlist_colors();
        let css = xmms_menu_css(&skin);

        assert!(css.contains(&format!(
            "background: #{:02x}{:02x}{:02x}",
            colors.normal_bg[0], colors.normal_bg[1], colors.normal_bg[2]
        )));
        assert!(css.contains(&format!(
            "color: #{:02x}{:02x}{:02x}",
            colors.normal[0], colors.normal[1], colors.normal[2]
        )));
        assert!(css.contains(&format!(
            "background: #{:02x}{:02x}{:02x}",
            colors.selected_bg[0], colors.selected_bg[1], colors.selected_bg[2]
        )));
        assert!(css.contains(&format!(
            "color: #{:02x}{:02x}{:02x}",
            colors.current[0], colors.current[1], colors.current[2]
        )));
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
            .add_spotify("spotify:track:test", "Test", 10_000);
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
        state.equalizer_shaded = true;

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
