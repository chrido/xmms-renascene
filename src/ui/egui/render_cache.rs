//! Cached egui textures and their render-state keys.

use crate::render::{EqualizerRenderState, MainWindowRenderState, PlaylistRowsRenderState};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlaylistTextureKey {
    pub generation: u64,
    pub focused: bool,
    pub shaded: bool,
    pub width: i32,
    pub height: i32,
    pub shaded_info: String,
    pub rows: PlaylistRowsRenderState,
    pub footer_info: String,
    pub footer_time_minutes: String,
    pub footer_time_seconds: String,
    pub render_scale_bits: u64,
}

pub(crate) struct CachedMainTexture {
    pub generation: u64,
    pub state: MainWindowRenderState,
    pub texture: egui::TextureHandle,
}

pub(crate) struct CachedEqualizerTexture {
    pub generation: u64,
    pub state: EqualizerRenderState,
    pub texture: egui::TextureHandle,
}

pub(crate) struct CachedPlaylistTexture {
    pub key: PlaylistTextureKey,
    pub texture: egui::TextureHandle,
}

#[derive(Default)]
pub struct RenderCache {
    pub generation: u64,
    pub(crate) main: Option<CachedMainTexture>,
    pub(crate) equalizer: Option<CachedEqualizerTexture>,
    pub(crate) playlist: Option<CachedPlaylistTexture>,
}

impl RenderCache {
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}
