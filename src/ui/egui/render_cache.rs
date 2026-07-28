//! Cached egui textures and their render-state keys.

use std::hash::{DefaultHasher, Hash, Hasher};

use crate::app::view_model::{playlist_projection, PlaylistProjection, PlaylistProjectionKey};
use crate::app_state::AppState;
use crate::player::PlayerState;
use crate::playlist::PlaylistRevisions;
use crate::render::{EqualizerRenderState, MainWindowRenderState};

use super::skin_texture::ImageRenderBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlaylistTextureKey {
    pub generation: u64,
    pub focused: bool,
    pub shaded: bool,
    pub width: i32,
    pub height: i32,
    pub scroll_offset: usize,
    pub revisions: PlaylistRevisions,
    pub title_preferences_hash: u64,
    pub font_hash: u64,
    pub show_numbers: bool,
    pub playback_time_visible: bool,
    pub render_scale_bits: u64,
}

impl PlaylistTextureKey {
    pub(crate) fn from_state(
        state: &AppState,
        generation: u64,
        scroll_offset: usize,
        width: i32,
        height: i32,
        render_scale: f64,
    ) -> Self {
        let mut title_preferences = DefaultHasher::new();
        state.config.title_format.hash(&mut title_preferences);
        state.config.convert_underscore.hash(&mut title_preferences);
        state.config.convert_twenty.hash(&mut title_preferences);

        let mut font = DefaultHasher::new();
        state.config.playlist_font.hash(&mut font);

        Self {
            generation,
            focused: true,
            shaded: state.config.playlist_shaded,
            width,
            height,
            scroll_offset,
            revisions: state.playlist.revisions(),
            title_preferences_hash: title_preferences.finish(),
            font_hash: font.finish(),
            show_numbers: state.config.show_numbers_in_pl,
            playback_time_visible: state.player.state() != PlayerState::Stopped,
            render_scale_bits: render_scale.to_bits(),
        }
    }
}

pub(crate) struct CachedMainTexture {
    pub generation: u64,
    pub state: MainWindowRenderState,
    pub texture: egui::TextureHandle,
}

pub(crate) struct CachedMainStaticTexture {
    pub generation: u64,
    pub focused: bool,
    pub shaded: bool,
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
    pub(crate) main_static: Option<CachedMainStaticTexture>,
    pub(crate) equalizer: Option<CachedEqualizerTexture>,
    pub(crate) playlist: Option<CachedPlaylistTexture>,
    pub(crate) playlist_projection: Option<CachedPlaylistProjection>,
    pub(crate) main_staging: ImageRenderBuffer,
    pub(crate) main_static_staging: ImageRenderBuffer,
    pub(crate) equalizer_staging: ImageRenderBuffer,
    pub(crate) playlist_staging: ImageRenderBuffer,
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

    #[test]
    fn playlist_texture_key_uses_revisions_not_rendered_rows() {
        let mut state = AppState::default();
        state.playlist.add_timed_uri("one.mp3", "One", 1_000);
        let key = PlaylistTextureKey::from_state(&state, 1, 0, 275, 232, 1.0);

        state.config.playback_position_ms = 500;
        assert_eq!(
            key,
            PlaylistTextureKey::from_state(&state, 1, 0, 275, 232, 1.0)
        );

        state.playlist.select_all(true);
        assert_ne!(
            key,
            PlaylistTextureKey::from_state(&state, 1, 0, 275, 232, 1.0)
        );
    }
}
