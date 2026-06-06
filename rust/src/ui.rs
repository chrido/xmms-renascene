use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::app_state::AppState;
use crate::player::PlayerState;
use crate::render::{
    render_equalizer_background, render_main_player_state, render_playlist_frame, MainPushButton,
    MainSlider, MainToggleButton, MainWindowRenderState, EQUALIZER_WINDOW_HEIGHT,
    EQUALIZER_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH,
};
use crate::skin::widget::PlayStatusValue;
use crate::skin::DefaultSkin;

const DEFAULT_SCALE: i32 = 2;

pub fn run_default_skin_preview() {
    run_preview_application(PreviewMode::Interactive);
}

pub fn run_default_skin_preview_smoke() {
    run_preview_application(PreviewMode::Smoke);
}

enum PreviewMode {
    Interactive,
    Smoke,
}

fn run_preview_application(mode: PreviewMode) {
    let app = gtk::Application::builder()
        .application_id("org.xmms.Resuscitated.RustPreview")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        if let Err(err) = build_preview_window(app) {
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

fn build_preview_window(app: &gtk::Application) -> Result<(), String> {
    let skin = DefaultSkin::load_bundled().map_err(|err| err.to_string())?;
    let skin = Rc::new(skin);
    let main_state = Rc::new(RefCell::new(MainWindowUiState::default()));

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
    let panel_windows = Rc::new(PanelWindows::new(app, &skin, &main_state, &drawing_area));
    let menu_popover = Rc::new(build_main_menu_popover(
        app,
        &drawing_area,
        &panel_windows.preferences,
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

    window.set_child(Some(&drawing_area));
    window.present();
    Ok(())
}

fn build_main_menu_popover(
    app: &gtk::Application,
    parent: &gtk::DrawingArea,
    preferences_window: &gtk::ApplicationWindow,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .build();
    popover.set_parent(parent);

    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for label in ["Open Files...", "Open Location..."] {
        let item = gtk::Button::with_label(label);
        item.set_halign(gtk::Align::Fill);
        menu_box.append(&item);
    }

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
    menu_box.append(&skin_browser);

    let quit = gtk::Button::with_label("Quit");
    quit.set_halign(gtk::Align::Fill);
    {
        let app = app.clone();
        quit.connect_clicked(move |_| app.quit());
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
}

impl PanelWindows {
    fn new(
        app: &gtk::Application,
        skin: &Rc<DefaultSkin>,
        main_state: &Rc<RefCell<MainWindowUiState>>,
        main_area: &gtk::DrawingArea,
    ) -> Self {
        let (equalizer, equalizer_area) = build_equalizer_window(app, skin, main_state, main_area);
        let (playlist, playlist_area, _playlist_menu) =
            build_playlist_window(app, skin, main_state, main_area);
        let preferences = build_preferences_window(app, main_state);
        Self {
            equalizer,
            equalizer_area,
            playlist,
            playlist_area,
            preferences,
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
        let shaded = state.borrow().equalizer_shaded;
        let base_height = if shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        };
        cr.scale(
            width as f64 / EQUALIZER_WINDOW_WIDTH as f64,
            height as f64 / base_height as f64,
        );
        if let Err(err) = render_equalizer_background(cr, &skin, true, shaded) {
            eprintln!("xmms-rs: failed to render equalizer preview: {err}");
        }
    });
    add_panel_click_controller(
        &window,
        &drawing_area,
        Rc::clone(main_state),
        main_area.clone(),
        PanelKind::Equalizer,
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
) -> (gtk::ApplicationWindow, gtk::DrawingArea, gtk::Popover) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Resuscitated Rust Playlist")
        .resizable(false)
        .decorated(false)
        .default_width(PLAYLIST_DEFAULT_WIDTH * DEFAULT_SCALE)
        .default_height(PLAYLIST_DEFAULT_HEIGHT * DEFAULT_SCALE)
        .build();
    let drawing_area = gtk::DrawingArea::builder()
        .content_width(PLAYLIST_DEFAULT_WIDTH * DEFAULT_SCALE)
        .content_height(PLAYLIST_DEFAULT_HEIGHT * DEFAULT_SCALE)
        .build();
    let skin = Rc::clone(skin);
    let state = Rc::clone(main_state);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let shaded = state.borrow().playlist_shaded;
        let base_height = if shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            PLAYLIST_DEFAULT_HEIGHT
        };
        cr.scale(
            width as f64 / PLAYLIST_DEFAULT_WIDTH as f64,
            height as f64 / base_height as f64,
        );
        if let Err(err) = render_playlist_frame(
            cr,
            &skin,
            true,
            shaded,
            PLAYLIST_DEFAULT_WIDTH,
            PLAYLIST_DEFAULT_HEIGHT,
        ) {
            eprintln!("xmms-rs: failed to render playlist preview: {err}");
        }
    });
    let playlist_menu = build_playlist_menu_popover(&drawing_area);
    add_panel_click_controller(
        &window,
        &drawing_area,
        Rc::clone(main_state),
        main_area.clone(),
        PanelKind::Playlist,
        Some(playlist_menu.clone()),
    );
    window.set_child(Some(&drawing_area));
    (window, drawing_area, playlist_menu)
}

fn build_playlist_menu_popover(parent: &gtk::DrawingArea) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .build();
    popover.set_parent(parent);
    popover
}

fn set_playlist_menu_items(popover: &gtk::Popover, menu: PlaylistMenuKind) {
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for label in menu.labels() {
        let item = gtk::Button::with_label(label);
        item.set_halign(gtk::Align::Fill);
        menu_box.append(&item);
    }
    popover.set_child(Some(&menu_box));
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
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&gtk::Label::new(Some(
        "Preferences UI placeholder for the Rust port",
    )));
    window.set_child(Some(&content));
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

impl PlaylistMenuKind {
    fn labels(self) -> &'static [&'static str] {
        match self {
            Self::Add => &["Add URL", "Add Directory", "Add File"],
            Self::Remove => &["Remove Misc", "Remove All", "Crop", "Remove Selected"],
            Self::Select => &["Invert Selection", "Select None", "Select All"],
            Self::Misc => &["Sort List", "File Info", "Options"],
            Self::List => &["New List", "Save List", "Load List"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelAction {
    None,
    Changed,
    ShowPlaylistMenu(PlaylistMenuKind),
}

fn add_panel_click_controller(
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    main_area: gtk::DrawingArea,
    kind: PanelKind,
    playlist_menu: Option<gtk::Popover>,
) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        click.connect_pressed(move |gesture, _n_press, x, y| {
            let (base_x, base_y) =
                panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            if !main_state
                .borrow()
                .panel_title_drag_region(kind, base_x, base_y)
            {
                return;
            }

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
        click.connect_released(move |_gesture, _n_press, x, y| {
            let (x, y) = panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            let action = main_state.borrow_mut().panel_click(kind, x, y);
            match action {
                PanelAction::None => {}
                PanelAction::Changed => {
                    sync_single_panel_window(kind, &window, &area, &main_state.borrow());
                    main_area.queue_draw();
                }
                PanelAction::ShowPlaylistMenu(menu) => {
                    if let Some(popover) = playlist_menu.as_ref() {
                        set_playlist_menu_items(popover, menu);
                        show_playlist_menu(popover, &area, menu);
                    }
                    area.queue_draw();
                }
            }
        });
    }
    window.add_controller(click);
}

fn show_playlist_menu(popover: &gtk::Popover, area: &gtk::DrawingArea, menu: PlaylistMenuKind) {
    let (base_x, base_y, base_width, base_height) = playlist_menu_rect(menu);
    let scale_x = area.allocated_width().max(1) as f64 / f64::from(PLAYLIST_DEFAULT_WIDTH);
    let scale_y = area.allocated_height().max(1) as f64 / f64::from(PLAYLIST_DEFAULT_HEIGHT);
    let rect = gtk::gdk::Rectangle::new(
        (f64::from(base_x) * scale_x) as i32,
        (f64::from(base_y) * scale_y) as i32,
        (f64::from(base_width) * scale_x).max(1.0) as i32,
        (f64::from(base_height) * scale_y).max(1.0) as i32,
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
            PLAYLIST_DEFAULT_WIDTH,
            if state.playlist_shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                PLAYLIST_DEFAULT_HEIGHT
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

fn sync_single_panel_window(
    kind: PanelKind,
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    state: &MainWindowUiState,
) {
    let (visible, shaded, width, full_height) = match kind {
        PanelKind::Equalizer => (
            state.app_state.config.equalizer_visible,
            state.equalizer_shaded,
            EQUALIZER_WINDOW_WIDTH,
            EQUALIZER_WINDOW_HEIGHT,
        ),
        PanelKind::Playlist => (
            state.app_state.config.playlist_visible,
            state.playlist_shaded,
            PLAYLIST_DEFAULT_WIDTH,
            PLAYLIST_DEFAULT_HEIGHT,
        ),
    };
    if !visible {
        window.hide();
        return;
    }
    let height = if shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        full_height
    };
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
            PLAYLIST_DEFAULT_HEIGHT
        };
        windows
            .playlist_area
            .set_content_height(height * DEFAULT_SCALE);
        windows.playlist.set_default_size(
            PLAYLIST_DEFAULT_WIDTH * DEFAULT_SCALE,
            height * DEFAULT_SCALE,
        );
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
}

#[derive(Debug, Clone)]
pub(crate) struct MainWindowUiState {
    app_state: AppState,
    shaded: bool,
    menu_visible: bool,
    equalizer_shaded: bool,
    playlist_shaded: bool,
    playlist_menu: Option<PlaylistMenuKind>,
    preferences_visible: bool,
    position_position: i32,
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
        Self {
            app_state,
            shaded: false,
            menu_visible: false,
            equalizer_shaded: false,
            playlist_shaded: false,
            playlist_menu: None,
            preferences_visible: false,
            position_position: 0,
            active: None,
            active_inside: false,
            slider_press_offset: 0,
        }
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
            ..MainWindowRenderState::default()
        }
    }

    pub(crate) fn panel_visibility(&self) -> PanelVisibility {
        PanelVisibility {
            equalizer: self.app_state.config.equalizer_visible,
            playlist: self.app_state.config.playlist_visible,
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

    pub(crate) fn is_preferences_visible(&self) -> bool {
        self.preferences_visible
    }

    pub(crate) fn set_preferences_visible(&mut self, visible: bool) {
        self.preferences_visible = visible;
    }

    pub(crate) fn panel_title_drag_region(&self, kind: PanelKind, x: i32, y: i32) -> bool {
        let title_height = match kind {
            PanelKind::Equalizer => MAIN_TITLEBAR_HEIGHT,
            PanelKind::Playlist => 20,
        };
        y >= 0 && y < title_height && !self.panel_title_button_hit(kind, x, y)
    }

    pub(crate) fn panel_click(&mut self, kind: PanelKind, x: i32, y: i32) -> PanelAction {
        if kind == PanelKind::Playlist {
            self.playlist_menu = None;
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
            if let Some(menu) = playlist_menu_at(x, y) {
                self.playlist_menu = Some(menu);
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

    pub(crate) fn volume(&self) -> i32 {
        self.app_state.player.volume()
    }

    pub(crate) fn balance(&self) -> i32 {
        self.app_state.player.balance()
    }

    pub(crate) fn position(&self) -> i32 {
        self.position_position
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

    fn activate_push(&mut self, button: MainPushButton) -> UiAction {
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
            MainPushButton::Previous | MainPushButton::Next => {
                self.position_position = 0;
                UiAction::None
            }
            MainPushButton::Eject => UiAction::None,
        }
    }

    fn activate_toggle(&mut self, toggle: MainToggleButton) {
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

fn playlist_menu_at(x: i32, y: i32) -> Option<PlaylistMenuKind> {
    [
        (
            PlaylistMenuKind::Add,
            ControlRect::new(12, playlist_button_y(), 25, 18),
        ),
        (
            PlaylistMenuKind::Remove,
            ControlRect::new(41, playlist_button_y(), 25, 18),
        ),
        (
            PlaylistMenuKind::Select,
            ControlRect::new(70, playlist_button_y(), 25, 18),
        ),
        (
            PlaylistMenuKind::Misc,
            ControlRect::new(99, playlist_button_y(), 25, 18),
        ),
        (
            PlaylistMenuKind::List,
            ControlRect::new(PLAYLIST_DEFAULT_WIDTH - 46, playlist_button_y(), 23, 18),
        ),
    ]
    .into_iter()
    .find_map(|(menu, rect)| rect.contains(x, y).then_some(menu))
}

fn playlist_menu_rect(menu: PlaylistMenuKind) -> (i32, i32, i32, i32) {
    let (x, items) = match menu {
        PlaylistMenuKind::Add => (12, 3),
        PlaylistMenuKind::Remove => (41, 4),
        PlaylistMenuKind::Select => (70, 3),
        PlaylistMenuKind::Misc => (99, 3),
        PlaylistMenuKind::List => (PLAYLIST_DEFAULT_WIDTH - 46, 3),
    };
    let item_height = 18;
    (
        x - 1,
        playlist_button_y() - ((items - 1) * item_height) - 1,
        25,
        items * item_height,
    )
}

const fn playlist_button_y() -> i32 {
    PLAYLIST_DEFAULT_HEIGHT - 29
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
    }
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
}
