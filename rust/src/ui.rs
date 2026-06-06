use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use crate::app_state::AppState;
use crate::config::{Config, TimerMode};
use crate::player::{
    equalizer_position_to_db, group_output_devices, OutputDevice, OutputDeviceGroups,
    OutputDeviceSelection, PlayerState,
};
use crate::playlist::{DurationIndexResult, Playlist, PlaylistSortKey};
use crate::render::{
    docked_panel_size, render_equalizer_state, render_main_player_state, render_playlist_frame,
    render_playlist_menu, render_playlist_rows, DockedPanelState, EqualizerControl,
    EqualizerRenderState, MainPushButton, MainSlider, MainToggleButton, MainWindowRenderState,
    PlaylistMenuRenderKind, PlaylistMenuRenderState, PlaylistRowRenderEntry,
    PlaylistRowsRenderState, VisualizationRenderState, EQUALIZER_WINDOW_HEIGHT,
    EQUALIZER_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH, PLAYLIST_MIN_HEIGHT, PLAYLIST_MIN_WIDTH,
};
use crate::skin::widget::{
    PlayStatusValue, VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode, VisScopeMode,
    VisVuMode, Visualization, WidgetId,
};
use crate::skin::{discover_skins_in_dirs, DefaultSkin, SkinEntry};

const DEFAULT_SCALE: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PreviewOptions {
    pub show_playlist: bool,
    pub playlist_size: Option<(i32, i32)>,
}

pub fn run_default_skin_preview(options: PreviewOptions) {
    run_preview_application(PreviewMode::Interactive, options);
}

pub fn run_default_skin_preview_smoke(options: PreviewOptions) {
    run_preview_application(PreviewMode::Smoke, options);
}

enum PreviewMode {
    Interactive,
    Smoke,
}

fn run_preview_application(mode: PreviewMode, options: PreviewOptions) {
    let app = gtk::Application::builder()
        .application_id("org.xmms.Resuscitated.RustPreview")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        if let Err(err) = build_preview_window(app, options) {
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

fn build_preview_window(app: &gtk::Application, options: PreviewOptions) -> Result<(), String> {
    let skin = DefaultSkin::load_bundled().map_err(|err| err.to_string())?;
    let skin = Rc::new(skin);
    let main_state = Rc::new(RefCell::new(MainWindowUiState::default()));
    {
        let mut state = main_state.borrow_mut();
        if let Some((width, height)) = options.playlist_size {
            state.set_playlist_size(width, height);
        }
        if options.show_playlist || options.playlist_size.is_some() {
            state.app_state.config.playlist_visible = true;
        }
    }

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Resuscitated Rust Preview")
        .resizable(false)
        .decorated(false)
        .default_width(MAIN_WINDOW_WIDTH * DEFAULT_SCALE)
        .default_height(MAIN_WINDOW_HEIGHT * DEFAULT_SCALE)
        .build();

    let drawing_area = gtk::DrawingArea::builder()
        .content_width(MAIN_WINDOW_WIDTH * DEFAULT_SCALE)
        .content_height(MAIN_WINDOW_HEIGHT * DEFAULT_SCALE)
        .focusable(true)
        .build();
    let panel_windows = Rc::new(PanelWindows::new(
        app,
        &skin,
        &main_state,
        &drawing_area,
        &window,
    ));
    sync_panel_windows(&panel_windows, &main_state.borrow());
    let menu_popover = Rc::new(build_main_menu_popover(
        app,
        &window,
        &drawing_area,
        &panel_windows.preferences,
        &panel_windows.open_location,
        &panel_windows.skin_browser,
        &main_state,
    ));

    {
        let skin = Rc::clone(&skin);
        let main_state = Rc::clone(&main_state);
        drawing_area.set_draw_func(move |_area, cr, width, height| {
            let base_height = if main_state.borrow().shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                MAIN_WINDOW_HEIGHT
            };
            cr.scale(
                width as f64 / MAIN_WINDOW_WIDTH as f64,
                height as f64 / base_height as f64,
            );
            let render_state = main_state.borrow().render_state();
            if let Err(err) = render_main_player_state(cr, &skin, &render_state) {
                eprintln!("xmms-rs: failed to render main-window preview: {err}");
            }
        });
    }

    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let drawing_area = drawing_area.clone();
        let main_state = Rc::clone(&main_state);
        click.connect_pressed(move |_gesture, _n_press, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, x, y);
            main_state.borrow_mut().press(x, y);
            drawing_area.queue_draw();
        });
    }
    {
        let app = app.clone();
        let window = window.clone();
        let drawing_area = drawing_area.clone();
        let menu_popover = Rc::clone(&menu_popover);
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        click.connect_released(move |_gesture, _n_press, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, x, y);
            let action = main_state.borrow_mut().release(x, y);
            apply_ui_action(
                action,
                &app,
                &window,
                &drawing_area,
                &menu_popover,
                &main_state,
            );
            sync_panel_windows(&panel_windows, &main_state.borrow());
            drawing_area.queue_draw();
        });
    }
    window.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let drawing_area = drawing_area.clone();
        let main_state = Rc::clone(&main_state);
        motion.connect_motion(move |_motion, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, x, y);
            if main_state.borrow_mut().motion(x, y) {
                drawing_area.queue_draw();
            }
        });
    }
    window.add_controller(motion);

    add_file_drop_controller(&drawing_area, Rc::clone(&main_state), true, true);

    let key_controller = gtk::EventControllerKey::new();
    {
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let window = window.clone();
        let drawing_area = drawing_area.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
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
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            if main_state.borrow_mut().update_timer_tick(100) {
                drawing_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
                panel_windows.equalizer_area.queue_draw();
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    window.set_child(Some(&drawing_area));
    window.present();
    Ok(())
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
    Plugins,
    Visualization,
    Options,
    Fonts,
    Title,
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
    let height = if state.shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        MAIN_WINDOW_HEIGHT
    };
    drawing_area.set_content_height(height * DEFAULT_SCALE);
    window.set_default_size(MAIN_WINDOW_WIDTH * DEFAULT_SCALE, height * DEFAULT_SCALE);
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
    popover.set_parent(parent);

    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let open_files = gtk::Button::with_label("Open Files...");
    open_files.set_halign(gtk::Align::Fill);
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

    let open_location = gtk::Button::with_label("Open Location...");
    open_location.set_halign(gtk::Align::Fill);
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

    let preferences = gtk::Button::with_label("Preferences");
    preferences.set_halign(gtk::Align::Fill);
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

    let skin_browser = gtk::Button::with_label("Skin Browser");
    skin_browser.set_halign(gtk::Align::Fill);
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

    let quit = gtk::Button::with_label("Quit");
    quit.set_halign(gtk::Align::Fill);
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
        skin: &Rc<DefaultSkin>,
        main_state: &Rc<RefCell<MainWindowUiState>>,
        main_area: &gtk::DrawingArea,
        parent_window: &gtk::ApplicationWindow,
    ) -> Self {
        let (equalizer, equalizer_area) = build_equalizer_window(app, skin, main_state, main_area);
        let preferences = build_preferences_window(app, main_state);
        let open_location =
            build_prompt_window(app, parent_window, main_state, PromptKind::OpenLocation);
        let jump_time = build_prompt_window(app, parent_window, main_state, PromptKind::JumpTime);
        let skin_browser = build_skin_browser_window(app, main_state);
        let (playlist, playlist_area) =
            build_playlist_window(app, skin, main_state, main_area, &open_location);
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
    skin: &Rc<DefaultSkin>,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> (gtk::ApplicationWindow, gtk::DrawingArea) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Resuscitated Rust Equalizer")
        .resizable(false)
        .decorated(false)
        .default_width(EQUALIZER_WINDOW_WIDTH * DEFAULT_SCALE)
        .default_height(EQUALIZER_WINDOW_HEIGHT * DEFAULT_SCALE)
        .build();
    let drawing_area = gtk::DrawingArea::builder()
        .content_width(EQUALIZER_WINDOW_WIDTH * DEFAULT_SCALE)
        .content_height(EQUALIZER_WINDOW_HEIGHT * DEFAULT_SCALE)
        .build();
    let skin = Rc::clone(skin);
    let state = Rc::clone(main_state);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let render_state = state.borrow().equalizer_render_state();
        let base_height = if render_state.shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        };
        cr.scale(
            width as f64 / EQUALIZER_WINDOW_WIDTH as f64,
            height as f64 / base_height as f64,
        );
        if let Err(err) = render_equalizer_state(cr, &skin, &render_state) {
            eprintln!("xmms-rs: failed to render equalizer preview: {err}");
        }
    });
    let presets_menu = build_equalizer_presets_popover(&drawing_area, main_state);
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
    window.set_child(Some(&drawing_area));
    (window, drawing_area)
}

fn build_playlist_window(
    app: &gtk::Application,
    skin: &Rc<DefaultSkin>,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    open_location_window: &gtk::ApplicationWindow,
) -> (gtk::ApplicationWindow, gtk::DrawingArea) {
    let (playlist_width, playlist_height) = main_state.borrow().playlist_size();
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Resuscitated Rust Playlist")
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
    let skin = Rc::clone(skin);
    let state = Rc::clone(main_state);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let state = state.borrow();
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
        if let Err(err) =
            render_playlist_frame(cr, &skin, focused, shaded, playlist_width, playlist_height)
        {
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
                    title: entry.title.clone(),
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
                width: playlist_width,
                height: playlist_height,
            };
            if let Err(err) = render_playlist_rows(cr, &skin, &row_state) {
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
            if let Err(err) = render_playlist_menu(cr, &skin, render_state) {
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
            let base_height = if state.playlist_shaded {
                state.playlist_height
            } else {
                (height / DEFAULT_SCALE).max(PLAYLIST_MIN_HEIGHT)
            };
            if state.set_playlist_size((width / DEFAULT_SCALE).max(PLAYLIST_MIN_WIDTH), base_height)
            {
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

fn handle_playlist_key_pressed(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    {
        let mut ui_state = main_state.borrow_mut();
        if ui_state.playlist_search_active {
            if key == gtk::gdk::Key::Escape
                || key == gtk::gdk::Key::Return
                || key == gtk::gdk::Key::KP_Enter
            {
                ui_state.stop_playlist_search();
                return true;
            }
            if key == gtk::gdk::Key::BackSpace {
                ui_state.pop_playlist_search_char();
                return true;
            }
            if state.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::META_MASK,
            ) {
                return true;
            }
            if let Some(ch) = key.to_unicode().filter(|ch| !ch.is_control()) {
                ui_state.push_playlist_search_char(ch);
                return true;
            }
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
    if key == gtk::gdk::Key::slash {
        main_state.borrow_mut().start_playlist_search();
        return true;
    }
    false
}

fn add_playlist_context_menu(
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    main_area: gtk::DrawingArea,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_parent(area);

    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for (label, action) in [
        ("Remove Selected", PlaylistContextAction::RemoveSelected),
        ("Remove Dead Files", PlaylistContextAction::RemoveDead),
        ("Physically Delete", PlaylistContextAction::PhysicallyDelete),
        ("Select All", PlaylistContextAction::SelectAll),
        ("Select None", PlaylistContextAction::SelectNone),
        ("Invert Selection", PlaylistContextAction::InvertSelection),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_halign(gtk::Align::Fill);
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
    popover.set_parent(parent);
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
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
        let item = gtk::Button::with_label(label);
        item.set_halign(gtk::Align::Fill);
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
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .build();
    popover.set_parent(parent);
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (label, preset) in [
        ("Flat", 0),
        ("Bass Boost", 1),
        ("Treble Boost", 2),
        ("Rock", 3),
    ] {
        let item = gtk::Button::with_label(label);
        item.set_halign(gtk::Align::Fill);
        {
            let main_state = Rc::clone(main_state);
            let popover = popover.clone();
            item.connect_clicked(move |_| {
                main_state.borrow_mut().apply_equalizer_preset(preset);
                popover.popdown();
            });
        }
        menu_box.append(&item);
    }
    popover.set_child(Some(&menu_box));
    popover
}

fn build_preferences_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Preferences")
        .default_width(560)
        .default_height(520)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);

    let notebook = gtk::Notebook::new();
    notebook.set_vexpand(true);
    for (page, label, body) in [
        (
            PreferencesPage::Audio,
            "Audio I/O Plugins",
            "GStreamer input support and output device selection are wired through the Rust preferences model.",
        ),
        (
            PreferencesPage::Plugins,
            "Effect/General Plugins",
            "Built-in equalizer, MPRIS, and service integrations are configured by the Rust port.",
        ),
        (
            PreferencesPage::Visualization,
            "Visualization Plugins",
            "Visualization mode, analyzer, scope, peaks, VU, and refresh settings apply immediately.",
        ),
        (
            PreferencesPage::Options,
            "Options",
            "Playback, playlist, docking, title conversion, and podcast options apply immediately.",
        ),
        (
            PreferencesPage::Fonts,
            "Fonts",
            "Playlist font family and main-window font compatibility settings are preserved.",
        ),
        (
            PreferencesPage::Title,
            "Title",
            "Title format compatibility settings are preserved and normalized.",
        ),
    ] {
        let page_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        page_box.set_margin_top(8);
        page_box.set_margin_bottom(8);
        page_box.set_margin_start(8);
        page_box.set_margin_end(8);
        let text = gtk::Label::new(Some(body));
        text.set_xalign(0.0);
        text.set_wrap(true);
        page_box.append(&text);
        notebook.append_page(&page_box, Some(&gtk::Label::new(Some(label))));
        if page == PreferencesPage::Options {
            notebook.set_current_page(Some(3));
        }
    }
    {
        let main_state = Rc::clone(main_state);
        notebook.connect_switch_page(move |_notebook, _page_widget, page_num| {
            let page = match page_num {
                0 => PreferencesPage::Audio,
                1 => PreferencesPage::Plugins,
                2 => PreferencesPage::Visualization,
                3 => PreferencesPage::Options,
                4 => PreferencesPage::Fonts,
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
        reset.connect_clicked(move |_| {
            main_state.borrow_mut().reset_preferences_to_defaults();
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
) -> gtk::ApplicationWindow {
    build_placeholder_window(
        app,
        main_state,
        "Skin Browser",
        520,
        420,
        "Skin Browser placeholder for the Rust port",
        MainWindowUiState::set_skin_browser_visible,
    )
}

fn build_placeholder_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    title: &str,
    default_width: i32,
    default_height: i32,
    label: &str,
    set_visible: fn(&mut MainWindowUiState, bool),
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&gtk::Label::new(Some(label)));
    window.set_child(Some(&content));
    {
        let main_state = Rc::clone(main_state);
        window.connect_close_request(move |window| {
            set_visible(&mut main_state.borrow_mut(), false);
            window.hide();
            gtk::glib::Propagation::Stop
        });
    }
    window
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EqualizerSlider {
    Preamp,
    Band(usize),
    ShadedVolume,
    ShadedBalance,
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
        click.connect_pressed(move |gesture, _n_press, x, y| {
            area.grab_focus();
            let (base_x, base_y) =
                panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            if !main_state
                .borrow()
                .panel_title_drag_region(kind, base_x, base_y)
            {
                if kind == PanelKind::Equalizer
                    && main_state.borrow_mut().equalizer_press(base_x, base_y)
                {
                    area.queue_draw();
                } else if kind == PanelKind::Playlist {
                    if main_state.borrow_mut().playlist_press(base_x, base_y) {
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
    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        motion.connect_motion(move |_motion, x, y| {
            let (x, y) = panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
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
    (
        (x / (width / f64::from(base_width))) as i32,
        (y / (height / f64::from(base_height))) as i32,
    )
}

fn sync_single_panel_window_from_state(
    kind: PanelKind,
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    state: &Rc<RefCell<MainWindowUiState>>,
) {
    let (visible, shaded, width, full_height) = {
        let state = state.borrow();
        panel_window_values(kind, &state)
    };
    sync_single_panel_window_values(window, area, visible, shaded, width, full_height);
}

fn panel_window_values(kind: PanelKind, state: &MainWindowUiState) -> (bool, bool, i32, i32) {
    match kind {
        PanelKind::Equalizer => (
            state.app_state.config.equalizer_visible,
            state.equalizer_shaded,
            EQUALIZER_WINDOW_WIDTH,
            EQUALIZER_WINDOW_HEIGHT,
        ),
        PanelKind::Playlist => (
            state.app_state.config.playlist_visible,
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
    area.set_content_width(width * DEFAULT_SCALE);
    area.set_content_height(height * DEFAULT_SCALE);
    window.set_default_size(width * DEFAULT_SCALE, height * DEFAULT_SCALE);
    area.queue_draw();
}

fn sync_panel_windows(windows: &PanelWindows, state: &MainWindowUiState) {
    let visibility = state.panel_visibility();
    if visibility.equalizer {
        let height = if state.equalizer_shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        };
        windows
            .equalizer_area
            .set_content_height(height * DEFAULT_SCALE);
        windows.equalizer.set_default_size(
            EQUALIZER_WINDOW_WIDTH * DEFAULT_SCALE,
            height * DEFAULT_SCALE,
        );
        windows.equalizer.present();
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
            .set_content_width(state.playlist_width * DEFAULT_SCALE);
        windows
            .playlist_area
            .set_content_height(height * DEFAULT_SCALE);
        windows
            .playlist
            .set_default_size(state.playlist_width * DEFAULT_SCALE, height * DEFAULT_SCALE);
        windows.playlist.present();
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
pub(crate) enum UiAction {
    None,
    Quit,
    Minimize,
    Resize,
    ShowMenu,
    OpenFileDialog,
}

#[derive(Debug, Clone)]
pub(crate) struct MainWindowUiState {
    app_state: AppState,
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
    equalizer_preamp_position: i32,
    equalizer_band_positions: [i32; 10],
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
    playlist_search_active: bool,
    playlist_search_query: String,
    playlist_load_dialog_visible: bool,
    playlist_save_dialog_visible: bool,
    last_playlist_file_info: Option<String>,
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
    output_device_picker_visible: bool,
    output_device_groups: OutputDeviceGroups,
    selected_spotify_output_device: Option<String>,
    output_switch_count: u32,
    file_dialog_visible: bool,
    directory_dialog_visible: bool,
    last_open_location: Option<String>,
    last_jump_time_ms: Option<i64>,
    position_position: i32,
    update_accumulator_ms: u32,
    visualization: Visualization,
    visualization_tick_counter: i32,
    active: Option<MainControl>,
    active_inside: bool,
    slider_press_offset: i32,
}

impl Default for MainWindowUiState {
    fn default() -> Self {
        Self::from_app_state(AppState::default())
    }
}

impl MainWindowUiState {
    pub(crate) fn from_app_state(app_state: AppState) -> Self {
        let mut state = Self {
            app_state,
            shaded: false,
            menu_visible: false,
            equalizer_shaded: false,
            equalizer_focused: false,
            equalizer_dragging_title: false,
            equalizer_active: true,
            equalizer_automatic: false,
            equalizer_pressed_control: None,
            equalizer_pressed_inside: false,
            equalizer_dragging: None,
            equalizer_preamp_position: 50,
            equalizer_band_positions: [50; 10],
            playlist_shaded: false,
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
            playlist_search_active: false,
            playlist_search_query: String::new(),
            playlist_load_dialog_visible: false,
            playlist_save_dialog_visible: false,
            last_playlist_file_info: None,
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
            output_device_picker_visible: false,
            output_device_groups: OutputDeviceGroups::default(),
            selected_spotify_output_device: None,
            output_switch_count: 0,
            file_dialog_visible: false,
            directory_dialog_visible: false,
            last_open_location: None,
            last_jump_time_ms: None,
            position_position: 0,
            update_accumulator_ms: 0,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
            active: None,
            active_inside: false,
            slider_press_offset: 0,
        };
        state.apply_visualization_preferences();
        state
    }

    pub(crate) fn app_state_mut(&mut self) -> &mut AppState {
        &mut self.app_state
    }

    fn render_state(&self) -> MainWindowRenderState {
        MainWindowRenderState {
            shaded: self.shaded,
            volume_position: volume_to_position(self.app_state.player.volume()),
            balance_position: balance_to_position(self.app_state.player.balance()),
            position_position: self.position_position,
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
            visualization: self.make_visualization_render_state(),
            ..MainWindowRenderState::default()
        }
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
            preamp_position: self.equalizer_preamp_position,
            band_positions: self.equalizer_band_positions,
        }
    }

    pub(crate) fn panel_visibility(&self) -> PanelVisibility {
        PanelVisibility {
            equalizer: self.app_state.config.equalizer_visible,
            playlist: self.app_state.config.playlist_visible,
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

    pub(crate) fn reset_preferences_to_defaults(&mut self) {
        self.app_state.config = Config::default();
        self.app_state.apply_config_to_runtime();
        self.equalizer_active = self.app_state.config.equalizer_active;
        self.equalizer_automatic = self.app_state.config.equalizer_auto;
        self.equalizer_preamp_position = self.app_state.config.equalizer_preamp_pos;
        self.equalizer_band_positions = self.app_state.config.equalizer_band_pos;
        self.apply_visualization_preferences();
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
        if index == 0 {
            self.app_state.config.skin = None;
            self.selected_skin_index = 0;
            self.reload_skin();
            return true;
        }

        let Some(entry) = self.skin_browser_entries.get(index - 1) else {
            return false;
        };
        self.app_state.config.skin = Some(entry.path.display().to_string());
        self.selected_skin_index = index;
        self.reload_skin();
        true
    }

    pub(crate) fn reload_skin(&mut self) {
        self.skin_reload_count = self.skin_reload_count.saturating_add(1);
    }

    pub(crate) fn skin_reload_count(&self) -> u32 {
        self.skin_reload_count
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

    pub(crate) fn playlist_position(&self) -> Option<usize> {
        self.app_state.playlist.position()
    }

    pub(crate) fn current_playlist_entry_uri(&self) -> Option<&str> {
        self.app_state
            .playlist
            .position()
            .and_then(|position| self.playlist_entry_uri(position))
    }

    pub(crate) fn add_spotify_entry(&mut self, uri: &str, title: &str, duration_ms: i64) {
        self.app_state.playlist.add_spotify(uri, title, duration_ms);
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

    pub(crate) fn start_playlist_search(&mut self) {
        self.playlist_menu = None;
        self.playlist_menu_hover = None;
        self.playlist_menu_pressed = false;
        self.playlist_search_active = true;
        self.playlist_search_query.clear();
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

    pub(crate) fn accept_open_location(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.last_open_location = Some(text.to_string());
        match self.app_state.playlist.add_location(text) {
            Ok(added) => {
                if added > 0 {
                    if self.app_state.playlist.position().is_none() {
                        self.app_state.playlist.set_position(0);
                    }
                    self.app_state.player.mark_playing();
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
        if accepted && start_playback {
            self.app_state.player.mark_playing();
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
        self.position_position = ((ms / 1000) as i32).clamp(0, slider_max(MainSlider::Position));
        self.jump_time_visible = false;
    }

    pub(crate) fn set_playlist_size(&mut self, width: i32, height: i32) -> bool {
        let width = width.max(PLAYLIST_MIN_WIDTH);
        let height = height.max(PLAYLIST_MIN_HEIGHT);
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
                self.set_equalizer_slider_position(slider, x);
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
            self.set_equalizer_slider_position(slider, y);
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

    pub(crate) fn equalizer_release(&mut self, x: i32, y: i32) -> PanelAction {
        if let Some(control) = self.equalizer_pressed_control.take() {
            let activated =
                self.equalizer_pressed_inside && equalizer_control_at(x, y) == Some(control);
            self.equalizer_pressed_inside = false;
            if activated {
                match control {
                    EqualizerControl::On => self.equalizer_active = !self.equalizer_active,
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
    }

    fn set_equalizer_slider_position(&mut self, slider: EqualizerSlider, coordinate: i32) -> bool {
        match slider {
            EqualizerSlider::Preamp => {
                let position = ((coordinate - 38) * 100 / 63).clamp(0, 100);
                let changed = self.equalizer_preamp_position != position;
                self.equalizer_preamp_position = position;
                changed
            }
            EqualizerSlider::Band(band) => {
                let position = ((coordinate - 38) * 100 / 63).clamp(0, 100);
                let Some(value) = self.equalizer_band_positions.get_mut(band) else {
                    return false;
                };
                let changed = *value != position;
                *value = position;
                changed
            }
            EqualizerSlider::ShadedVolume => {
                let position = (coordinate - 61).clamp(0, 94);
                let volume = eq_shaded_position_to_volume(position);
                let changed = self.app_state.player.volume() != volume;
                self.app_state.player.set_volume(volume);
                changed
            }
            EqualizerSlider::ShadedBalance => {
                let position = (coordinate - 164).clamp(0, 39);
                let balance = eq_shaded_position_to_balance(position);
                let changed = self.app_state.player.balance() != balance;
                self.app_state.player.set_balance(balance);
                changed
            }
        }
    }

    pub(crate) fn panel_title_drag_region(&self, kind: PanelKind, x: i32, y: i32) -> bool {
        let title_height = match kind {
            PanelKind::Equalizer => MAIN_TITLEBAR_HEIGHT,
            PanelKind::Playlist => 20,
        };
        y >= 0 && y < title_height && !self.panel_title_button_hit(kind, x, y)
    }

    pub(crate) fn playlist_resize_region(&self, x: i32, y: i32) -> bool {
        !self.playlist_shaded && x > self.playlist_width - 20 && y > self.playlist_height - 20
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

    pub(crate) fn playlist_press(&mut self, x: i32, y: i32) -> bool {
        let Some(item) = self.playlist_menu_item_at(x, y) else {
            return false;
        };
        self.playlist_menu_hover = Some(item);
        self.playlist_menu_pressed = true;
        true
    }

    pub(crate) fn playlist_motion(&mut self, x: i32, y: i32) -> bool {
        if self.playlist_menu.is_none() {
            return false;
        }
        let item = self.playlist_menu_item_at(x, y);
        let changed = self.playlist_menu_hover != item;
        self.playlist_menu_hover = item;
        changed
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
        }

        if self.panel_title_button_hit(kind, x, y) {
            if (264..273).contains(&x) {
                match kind {
                    PanelKind::Equalizer => self.app_state.config.equalizer_visible = false,
                    PanelKind::Playlist => self.app_state.config.playlist_visible = false,
                }
                return PanelAction::Changed;
            }

            if (254..263).contains(&x) {
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
        }

        PanelAction::None
    }

    fn panel_title_button_hit(&self, kind: PanelKind, x: i32, y: i32) -> bool {
        (3..12).contains(&y)
            && match kind {
                PanelKind::Equalizer | PanelKind::Playlist => {
                    (254..263).contains(&x) || (264..273).contains(&x)
                }
            }
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
        self.position_position
    }

    pub(crate) fn set_preference_output_device(&mut self, device: Option<String>) {
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
        self.mark_preferences_saved();
    }

    pub(crate) fn set_preference_balance(&mut self, balance: i32) {
        let balance = balance.clamp(-100, 100);
        self.app_state.config.balance = balance;
        self.app_state.player.set_balance(balance);
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

    pub(crate) fn update_timer_tick(&mut self, elapsed_ms: u32) -> bool {
        if self.app_state.player.state() != PlayerState::Playing {
            self.update_accumulator_ms = 0;
            self.visualization_tick_counter = 0;
            return false;
        }

        self.update_accumulator_ms = self.update_accumulator_ms.saturating_add(elapsed_ms);
        let seconds = self.update_accumulator_ms / 1000;
        self.update_accumulator_ms %= 1000;
        if seconds > 0 {
            self.position_position =
                (self.position_position + seconds as i32).min(slider_max(MainSlider::Position));
        }
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

    pub(crate) fn playlist_eof_reached(&mut self) {
        self.position_position = 0;
        if self.app_state.playlist.eof_reached() {
            self.app_state.player.mark_playing();
        } else {
            self.app_state.player.stop();
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
        }

        controls
            .into_iter()
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
                self.app_state.player.mark_playing();
                UiAction::None
            }
            MainPushButton::Pause => {
                match self.app_state.player.state() {
                    PlayerState::Playing => self.app_state.player.pause(),
                    PlayerState::Paused => self.app_state.player.unpause(),
                    PlayerState::Stopped => {}
                }
                UiAction::None
            }
            MainPushButton::Stop => {
                self.app_state.player.stop();
                self.position_position = 0;
                UiAction::None
            }
            MainPushButton::Previous => {
                if self.app_state.playlist.previous() {
                    self.app_state.player.mark_playing();
                }
                self.position_position = 0;
                UiAction::None
            }
            MainPushButton::Next => {
                if self.app_state.playlist.next() {
                    self.app_state.player.mark_playing();
                }
                self.position_position = 0;
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
        let knob_width = slider_knob_width(slider);
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
        let position = position.clamp(0, slider_max(slider));
        let old_position = self.slider_position(slider);
        if old_position == position {
            return false;
        }

        match slider {
            MainSlider::Volume => self
                .app_state
                .player
                .set_volume(position_to_volume(position)),
            MainSlider::Balance => self
                .app_state
                .player
                .set_balance(position_to_balance(position)),
            MainSlider::Position => self.position_position = position,
        }
        self.app_state.sync_config_from_runtime();
        true
    }

    fn slider_position(&self, slider: MainSlider) -> i32 {
        match slider {
            MainSlider::Volume => volume_to_position(self.app_state.player.volume()),
            MainSlider::Balance => balance_to_position(self.app_state.player.balance()),
            MainSlider::Position => self.position_position,
        }
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
            MainControl::Push(button) => push_button_rect(button),
            MainControl::Toggle(toggle) => toggle_button_rect(toggle),
            MainControl::Slider(slider) => self.slider_rect(slider),
        }
    }

    fn slider_rect(&self, slider: MainSlider) -> ControlRect {
        match slider {
            MainSlider::Volume => ControlRect::new(107, 57, 68, 13),
            MainSlider::Balance => ControlRect::new(177, 57, 38, 13),
            MainSlider::Position => ControlRect::new(16, 72, 248, 10),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelVisibility {
    pub(crate) equalizer: bool,
    pub(crate) playlist: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ControlRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl ControlRect {
    const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
            && self.width > 0
            && self.height > 0
    }
}

fn push_button_rect(button: MainPushButton) -> ControlRect {
    match button {
        MainPushButton::Menu => ControlRect::new(6, 3, 9, 9),
        MainPushButton::Minimize => ControlRect::new(244, 3, 9, 9),
        MainPushButton::Shade => ControlRect::new(254, 3, 9, 9),
        MainPushButton::Close => ControlRect::new(264, 3, 9, 9),
        MainPushButton::Previous => ControlRect::new(16, 88, 23, 18),
        MainPushButton::Play => ControlRect::new(39, 88, 23, 18),
        MainPushButton::Pause => ControlRect::new(62, 88, 23, 18),
        MainPushButton::Stop => ControlRect::new(85, 88, 23, 18),
        MainPushButton::Next => ControlRect::new(108, 88, 22, 18),
        MainPushButton::Eject => ControlRect::new(136, 89, 22, 16),
    }
}

fn equalizer_control_at(x: i32, y: i32) -> Option<EqualizerControl> {
    [
        (EqualizerControl::On, ControlRect::new(14, 18, 25, 12)),
        (EqualizerControl::Auto, ControlRect::new(39, 18, 33, 12)),
        (EqualizerControl::Presets, ControlRect::new(217, 18, 44, 12)),
    ]
    .into_iter()
    .find_map(|(control, rect)| rect.contains(x, y).then_some(control))
}

fn equalizer_slider_at(x: i32, y: i32) -> Option<EqualizerSlider> {
    if ControlRect::new(21, 38, 14, 63).contains(x, y) {
        return Some(EqualizerSlider::Preamp);
    }

    (0..10).find_map(|band| {
        ControlRect::new(78 + band * 18, 38, 14, 63)
            .contains(x, y)
            .then_some(EqualizerSlider::Band(band as usize))
    })
}

fn equalizer_shaded_slider_at(x: i32, y: i32) -> Option<EqualizerSlider> {
    if ControlRect::new(61, 4, 97, 8).contains(x, y) {
        return Some(EqualizerSlider::ShadedVolume);
    }
    if ControlRect::new(164, 4, 42, 8).contains(x, y) {
        return Some(EqualizerSlider::ShadedBalance);
    }
    None
}

fn playlist_menu_at(x: i32, y: i32, width: i32, height: i32) -> Option<PlaylistMenuKind> {
    [
        (
            PlaylistMenuKind::Add,
            ControlRect::new(12, playlist_button_y(height), 25, 18),
        ),
        (
            PlaylistMenuKind::Remove,
            ControlRect::new(41, playlist_button_y(height), 25, 18),
        ),
        (
            PlaylistMenuKind::Select,
            ControlRect::new(70, playlist_button_y(height), 25, 18),
        ),
        (
            PlaylistMenuKind::Misc,
            ControlRect::new(99, playlist_button_y(height), 25, 18),
        ),
        (
            PlaylistMenuKind::List,
            ControlRect::new(width - 46, playlist_button_y(height), 23, 18),
        ),
    ]
    .into_iter()
    .find_map(|(menu, rect)| rect.contains(x, y).then_some(menu))
}

fn playlist_menu_rect(menu: PlaylistMenuKind, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let (x, items) = match menu {
        PlaylistMenuKind::Add => (12, 3),
        PlaylistMenuKind::Remove => (41, 4),
        PlaylistMenuKind::Select => (70, 3),
        PlaylistMenuKind::Misc => (99, 3),
        PlaylistMenuKind::List => (width - 46, 3),
    };
    let item_height = 18;
    (
        x - 1,
        playlist_button_y(height) - ((items - 1) * item_height) - 1,
        25,
        items * item_height,
    )
}

const fn playlist_button_y(height: i32) -> i32 {
    height - 29
}

fn toggle_button_rect(toggle: MainToggleButton) -> ControlRect {
    match toggle {
        MainToggleButton::Shuffle => ControlRect::new(164, 89, 46, 15),
        MainToggleButton::Repeat => ControlRect::new(210, 89, 28, 15),
        MainToggleButton::Equalizer => ControlRect::new(219, 58, 23, 12),
        MainToggleButton::Playlist => ControlRect::new(242, 58, 23, 12),
    }
}

fn slider_max(slider: MainSlider) -> i32 {
    match slider {
        MainSlider::Volume => 51,
        MainSlider::Balance => 24,
        MainSlider::Position => 219,
    }
}

fn slider_knob_width(slider: MainSlider) -> i32 {
    match slider {
        MainSlider::Volume | MainSlider::Balance => 14,
        MainSlider::Position => 29,
    }
}

fn volume_to_position(volume: i32) -> i32 {
    ((volume.clamp(0, 100) * 51 + 50) / 100).clamp(0, 51)
}

fn position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 51) * 100) as f64 / 51.0) as i32
}

fn balance_to_position(balance: i32) -> i32 {
    (12 + (balance.clamp(-100, 100) * 12) / 100).clamp(0, 24)
}

fn position_to_balance(position: i32) -> i32 {
    (((position.clamp(0, 24) - 12) * 100) as f64 / 12.0) as i32
}

fn eq_shaded_position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 94) * 100 + 47) / 94).clamp(0, 100)
}

fn eq_shaded_position_to_balance(position: i32) -> i32 {
    let position = position.clamp(0, 38);
    (((position - 19) * 100 + if position >= 19 { 9 } else { -9 }) / 19).clamp(-100, 100)
}

fn event_to_base_coords(area: &gtk::DrawingArea, x: f64, y: f64) -> (i32, i32) {
    let width = area.allocated_width().max(1) as f64;
    let height = area.allocated_height().max(1) as f64;
    let base_height = if height <= f64::from(MAIN_TITLEBAR_HEIGHT * DEFAULT_SCALE) {
        MAIN_TITLEBAR_HEIGHT
    } else {
        MAIN_WINDOW_HEIGHT
    };
    (
        (x / (width / f64::from(MAIN_WINDOW_WIDTH))) as i32,
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
            let height = if state.borrow().shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                MAIN_WINDOW_HEIGHT
            };
            drawing_area.set_content_height(height * DEFAULT_SCALE);
            window.set_default_size(MAIN_WINDOW_WIDTH * DEFAULT_SCALE, height * DEFAULT_SCALE);
        }
        UiAction::ShowMenu => {
            show_main_menu(menu_popover, drawing_area);
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

fn show_main_menu(menu_popover: &gtk::Popover, drawing_area: &gtk::DrawingArea) {
    let scale_x = drawing_area.allocated_width().max(1) as f64 / f64::from(MAIN_WINDOW_WIDTH);
    let scale_y = drawing_area.allocated_height().max(1) as f64 / f64::from(MAIN_WINDOW_HEIGHT);
    let rect = gtk::gdk::Rectangle::new(
        (6.0 * scale_x) as i32,
        (12.0 * scale_y) as i32,
        (9.0 * scale_x).max(1.0) as i32,
        (1.0 * scale_y).max(1.0) as i32,
    );
    menu_popover.set_pointing_to(Some(&rect));
    menu_popover.popup();
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

    #[test]
    fn main_window_buttons_update_player_and_toggle_state() {
        let mut state = MainWindowUiState::default();

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
        assert_eq!(state.position_position, 219);
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
    fn parse_prompt_time_accepts_seconds_and_minutes_seconds() {
        assert_eq!(parse_time_ms("42"), Some(42_000));
        assert_eq!(parse_time_ms("1:23"), Some(83_000));
        assert_eq!(parse_time_ms(""), None);
        assert_eq!(parse_time_ms("1:2:3"), None);
        assert_eq!(parse_time_ms("not-time"), None);
    }
}
