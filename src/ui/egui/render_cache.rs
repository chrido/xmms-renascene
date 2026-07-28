//! Cached egui textures and their render-state keys.

use crate::app::view_model::{playlist_projection, PlaylistProjection, PlaylistProjectionKey};
use crate::app_state::AppState;
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

pub(crate) struct CachedPlaylistProjection {
    pub key: PlaylistProjectionKey,
    pub projection: PlaylistProjection,
}

#[derive(Default)]
pub struct RenderCache {
    pub generation: u64,
    pub(crate) main: Option<CachedMainTexture>,
    pub(crate) equalizer: Option<CachedEqualizerTexture>,
    pub(crate) playlist: Option<CachedPlaylistTexture>,
    pub(crate) playlist_projection: Option<CachedPlaylistProjection>,
}

impl RenderCache {
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn playlist_projection(&mut self, state: &AppState) -> &PlaylistProjection {
        let key = PlaylistProjectionKey::from_state(state);
        if self
            .playlist_projection
            .as_ref()
            .is_none_or(|cached| cached.key != key)
        {
            self.playlist_projection = Some(CachedPlaylistProjection {
                key,
                projection: playlist_projection(state),
            });
        }
        &self
            .playlist_projection
            .as_ref()
            .expect("playlist projection initialized")
            .projection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_projection_cache_ignores_unrelated_player_changes() {
        let mut state = AppState::default();
        state.playlist.add_timed_uri("one.mp3", "One", 1_000);
        let mut cache = RenderCache::default();

        let first = cache.playlist_projection(&state) as *const PlaylistProjection;
        state.player.set_volume(25);
        let second = cache.playlist_projection(&state) as *const PlaylistProjection;
        assert_eq!(first, second);

        state.playlist.select_all(true);
        assert!(cache.playlist_projection(&state).rows[0].selected);
    }
}
