use cairo::Context;

use super::core::RenderError;
use super::equalizer::render_equalizer_background;
use super::main::render_main_player;
use super::playlist::render_playlist_frame;
use crate::skin::layout::{
    equalizer_window_height, main_window_height, playlist_window_height, MAIN_WINDOW_WIDTH,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH, PLAYLIST_MIN_WIDTH,
};
use crate::skin::DefaultSkin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockedPanelState {
    pub main_focused: bool,
    pub main_shaded: bool,
    pub equalizer_visible: bool,
    pub equalizer_detached: bool,
    pub equalizer_focused: bool,
    pub equalizer_shaded: bool,
    pub playlist_visible: bool,
    pub playlist_detached: bool,
    pub playlist_focused: bool,
    pub playlist_shaded: bool,
    pub playlist_width: i32,
    pub playlist_height: i32,
}

impl Default for DockedPanelState {
    fn default() -> Self {
        Self {
            main_focused: true,
            main_shaded: false,
            equalizer_visible: false,
            equalizer_detached: false,
            equalizer_focused: true,
            equalizer_shaded: false,
            playlist_visible: false,
            playlist_detached: false,
            playlist_focused: true,
            playlist_shaded: false,
            playlist_width: PLAYLIST_DEFAULT_WIDTH,
            playlist_height: PLAYLIST_DEFAULT_HEIGHT,
        }
    }
}

pub fn docked_panel_size(state: DockedPanelState) -> (i32, i32) {
    let playlist_width = state.playlist_width.max(PLAYLIST_MIN_WIDTH);
    let mut width = MAIN_WINDOW_WIDTH;
    let mut height = main_window_height(state.main_shaded);

    if state.equalizer_visible && !state.equalizer_detached {
        height += equalizer_window_height(state.equalizer_shaded);
    }
    if state.playlist_visible && !state.playlist_detached {
        height += playlist_window_height(state.playlist_shaded, state.playlist_height);
        width = width.max(playlist_width);
    }

    (width, height)
}

pub fn render_docked_panels(
    cr: &Context,
    skin: &DefaultSkin,
    state: DockedPanelState,
) -> Result<bool, RenderError> {
    let mut y = 0;
    let mut rendered = render_main_player(cr, skin, state.main_focused, state.main_shaded)?;
    y += main_window_height(state.main_shaded);

    if state.equalizer_visible && !state.equalizer_detached {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        rendered |=
            render_equalizer_background(cr, skin, state.equalizer_focused, state.equalizer_shaded)?;
        cr.restore()?;
        y += equalizer_window_height(state.equalizer_shaded);
    }

    if state.playlist_visible && !state.playlist_detached {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        rendered |= render_playlist_frame(
            cr,
            skin,
            state.playlist_focused,
            state.playlist_shaded,
            state.playlist_width,
            state.playlist_height,
            None,
            None,
            None,
            None,
        )?;
        cr.restore()?;
    }

    Ok(rendered)
}
