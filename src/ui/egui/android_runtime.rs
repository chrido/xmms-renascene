use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::playlist::Playlist;

const PERSIST_INTERVAL: Duration = Duration::from_millis(500);
const POST_ROTATION_REPAINT_FRAMES: u8 = 3;

pub(crate) struct AndroidRuntime {
    persistence_dirty: bool,
    last_persist: Instant,
    layout: LayoutState,
    pub playlist_manager_open: bool,
    pub playlist_name: String,
    pub saved_playlists: Vec<PathBuf>,
    media_playlist_snapshot: Option<Playlist>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AndroidSystemInsets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl AndroidRuntime {
    pub fn new(saved_playlists: Vec<PathBuf>) -> Self {
        Self {
            persistence_dirty: false,
            last_persist: Instant::now(),
            layout: LayoutState::default(),
            playlist_manager_open: false,
            playlist_name: "playlist".to_string(),
            saved_playlists,
            media_playlist_snapshot: None,
        }
    }

    pub fn mark_persistence(&mut self) {
        self.persistence_dirty = true;
    }

    pub fn take_persistence_due(&mut self, force: bool) -> bool {
        let now = Instant::now();
        if !self.persistence_dirty
            || (!force && now.saturating_duration_since(self.last_persist) < PERSIST_INTERVAL)
        {
            return false;
        }
        self.persistence_dirty = false;
        self.last_persist = now;
        true
    }

    pub fn persistence_delay(&self) -> Option<Duration> {
        self.persistence_dirty.then(|| {
            PERSIST_INTERVAL.saturating_sub(self.last_persist.elapsed())
        })
    }

    pub fn playlist_changed(&self, playlist: &Playlist) -> bool {
        self.media_playlist_snapshot.as_ref() != Some(playlist)
    }

    pub fn remember_playlist(&mut self, playlist: Playlist) {
        self.media_playlist_snapshot = Some(playlist);
    }

    pub fn layout_orientation(&self) -> Option<AndroidLayoutOrientation> {
        self.layout.orientation()
    }

    pub fn mark_layout_transitioning(&mut self, orientation: AndroidLayoutOrientation) {
        self.layout.phase = AndroidLayoutPhase::Transitioning { orientation };
    }

    pub fn accept_layout(&mut self, orientation: AndroidLayoutOrientation) -> bool {
        self.layout.accept(orientation)
    }

    pub fn remember_layout(
        &mut self,
        orientation: AndroidLayoutOrientation,
        layout: AndroidStableLayout,
    ) {
        match orientation {
            AndroidLayoutOrientation::Portrait => self.layout.portrait = Some(layout),
            AndroidLayoutOrientation::Landscape => self.layout.landscape = Some(layout),
        }
    }

    pub fn stable_layout(
        &self,
        orientation: AndroidLayoutOrientation,
    ) -> Option<AndroidStableLayout> {
        match orientation {
            AndroidLayoutOrientation::Portrait => self.layout.portrait,
            AndroidLayoutOrientation::Landscape => self.layout.landscape,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AndroidLayoutOrientation {
    Portrait,
    Landscape,
}

impl AndroidLayoutOrientation {
    pub fn from_configuration(orientation: i32) -> Option<Self> {
        match orientation {
            1 => Some(Self::Portrait),
            2 => Some(Self::Landscape),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AndroidStableLayout {
    pub width: i32,
    pub height: i32,
    pub insets: AndroidSystemInsets,
    pub scale_factor: f32,
    pub playlist_width: i32,
    pub playlist_height: i32,
}

#[derive(Clone, Copy, Debug, Default)]
enum AndroidLayoutPhase {
    #[default]
    Uninitialized,
    Transitioning {
        orientation: AndroidLayoutOrientation,
    },
    Stabilizing {
        orientation: AndroidLayoutOrientation,
        frames_remaining: u8,
    },
    Stable {
        orientation: AndroidLayoutOrientation,
    },
}

#[derive(Default)]
struct LayoutState {
    phase: AndroidLayoutPhase,
    portrait: Option<AndroidStableLayout>,
    landscape: Option<AndroidStableLayout>,
}

impl LayoutState {
    fn orientation(&self) -> Option<AndroidLayoutOrientation> {
        match self.phase {
            AndroidLayoutPhase::Uninitialized => None,
            AndroidLayoutPhase::Transitioning { orientation }
            | AndroidLayoutPhase::Stabilizing { orientation, .. }
            | AndroidLayoutPhase::Stable { orientation } => Some(orientation),
        }
    }

    fn accept(&mut self, orientation: AndroidLayoutOrientation) -> bool {
        let next = match self.phase {
            AndroidLayoutPhase::Stabilizing {
                orientation: current,
                frames_remaining,
            } if current == orientation && frames_remaining > 1 => {
                AndroidLayoutPhase::Stabilizing {
                    orientation,
                    frames_remaining: frames_remaining - 1,
                }
            }
            AndroidLayoutPhase::Stabilizing {
                orientation: current,
                ..
            } if current == orientation => {
                self.phase = AndroidLayoutPhase::Stable { orientation };
                return true;
            }
            AndroidLayoutPhase::Stable {
                orientation: current,
            } if current == orientation => return false,
            _ => AndroidLayoutPhase::Stabilizing {
                orientation,
                frames_remaining: POST_ROTATION_REPAINT_FRAMES,
            },
        };
        self.phase = next;
        true
    }
}

pub(crate) fn layout_extent_is_stable(width: f32, height: f32) -> bool {
    const MIN_STABLE_EXTENT: f32 = 32.0;
    width.is_finite()
        && height.is_finite()
        && width >= MIN_STABLE_EXTENT
        && height >= MIN_STABLE_EXTENT
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_snapshot_is_consistent(
    width: i32,
    height: i32,
    orientation: AndroidLayoutOrientation,
    insets: [i32; 4],
    inset_width: i32,
    inset_height: i32,
    inset_orientation: i32,
    config_generation: i64,
    inset_generation: i64,
    insets_fresh: bool,
) -> bool {
    let [left, top, right, bottom] = insets;
    let orientation_matches_extent = match orientation {
        AndroidLayoutOrientation::Portrait => height >= width,
        AndroidLayoutOrientation::Landscape => width > height,
    };
    width > 0
        && height > 0
        && orientation_matches_extent
        && config_generation > 0
        && inset_generation == config_generation
        && insets_fresh
        && inset_width == width
        && inset_height == height
        && AndroidLayoutOrientation::from_configuration(inset_orientation) == Some(orientation)
        && [left, top, right, bottom].iter().all(|inset| *inset >= 0)
        && layout_extent_is_stable(
            (width - left - right) as f32,
            (height - top - bottom) as f32,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_persistence_has_a_bounded_delay() {
        let mut runtime = AndroidRuntime::new(Vec::new());
        assert_eq!(runtime.persistence_delay(), None);

        runtime.mark_persistence();

        assert!(runtime.persistence_delay().is_some_and(|delay| delay <= PERSIST_INTERVAL));
        assert!(runtime.take_persistence_due(true));
        assert_eq!(runtime.persistence_delay(), None);
    }
}
