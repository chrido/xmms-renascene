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
    let panel_windows = Rc::new(PanelWindows::new(app, &skin));

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
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        click.connect_released(move |_gesture, _n_press, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, x, y);
            let action = main_state.borrow_mut().release(x, y);
            apply_ui_action(action, &app, &window, &drawing_area, &main_state.borrow());
            sync_panel_windows(&panel_windows, &main_state.borrow());
            drawing_area.queue_draw();
        });
    }
    drawing_area.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
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
    drawing_area.add_controller(motion);

    window.set_child(Some(&drawing_area));
    window.present();
    Ok(())
}

#[derive(Debug, Clone)]
struct PanelWindows {
    equalizer: gtk::ApplicationWindow,
    playlist: gtk::ApplicationWindow,
}

impl PanelWindows {
    fn new(app: &gtk::Application, skin: &Rc<DefaultSkin>) -> Self {
        Self {
            equalizer: build_equalizer_window(app, skin),
            playlist: build_playlist_window(app, skin),
        }
    }
}

fn build_equalizer_window(
    app: &gtk::Application,
    skin: &Rc<DefaultSkin>,
) -> gtk::ApplicationWindow {
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
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        cr.scale(
            width as f64 / EQUALIZER_WINDOW_WIDTH as f64,
            height as f64 / EQUALIZER_WINDOW_HEIGHT as f64,
        );
        if let Err(err) = render_equalizer_background(cr, &skin, true, false) {
            eprintln!("xmms-rs: failed to render equalizer preview: {err}");
        }
    });
    window.set_child(Some(&drawing_area));
    window
}

fn build_playlist_window(app: &gtk::Application, skin: &Rc<DefaultSkin>) -> gtk::ApplicationWindow {
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
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        cr.scale(
            width as f64 / PLAYLIST_DEFAULT_WIDTH as f64,
            height as f64 / PLAYLIST_DEFAULT_HEIGHT as f64,
        );
        if let Err(err) = render_playlist_frame(
            cr,
            &skin,
            true,
            false,
            PLAYLIST_DEFAULT_WIDTH,
            PLAYLIST_DEFAULT_HEIGHT,
        ) {
            eprintln!("xmms-rs: failed to render playlist preview: {err}");
        }
    });
    window.set_child(Some(&drawing_area));
    window
}

fn sync_panel_windows(windows: &PanelWindows, state: &MainWindowUiState) {
    let visibility = state.panel_visibility();
    if visibility.equalizer {
        windows.equalizer.present();
    } else {
        windows.equalizer.hide();
    }

    if visibility.playlist {
        windows.playlist.present();
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
}

#[derive(Debug, Clone)]
pub(crate) struct MainWindowUiState {
    app_state: AppState,
    shaded: bool,
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
            MainPushButton::Menu | MainPushButton::Eject => UiAction::None,
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
    state: &MainWindowUiState,
) {
    match action {
        UiAction::None => {}
        UiAction::Quit => app.quit(),
        UiAction::Minimize => window.minimize(),
        UiAction::Resize => {
            let height = if state.shaded {
                MAIN_TITLEBAR_HEIGHT
            } else {
                MAIN_WINDOW_HEIGHT
            };
            drawing_area.set_content_height(height * DEFAULT_SCALE);
            window.set_default_size(MAIN_WINDOW_WIDTH * DEFAULT_SCALE, height * DEFAULT_SCALE);
        }
    }
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
