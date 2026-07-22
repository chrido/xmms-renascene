//! Lifecycle-owned Android policy state.
//!
//! Unlike JNI callback registries, this state is owned by one egui app
//! instance: persistence debounce, layout transition/cache state, managed
//! playlists, and the last media projection.

use std::time::{Duration, Instant};

use crate::playlist::Playlist;

#[cfg(target_os = "android")]
use super::android::playlist_manager::PlaylistManager;
#[cfg(target_os = "android")]
use super::android_media::AndroidActivityGeneration;

const PERSIST_INTERVAL: Duration = Duration::from_millis(500);
const POST_ROTATION_REPAINT_FRAMES: u8 = 3;

pub(crate) struct AndroidRuntime {
    persistence_dirty: bool,
    last_persist: Instant,
    layout: LayoutState,
    layout_view: AndroidLayoutView,
    #[cfg(target_os = "android")]
    pub playlist_manager: PlaylistManager,
    #[cfg(target_os = "android")]
    activity_generation: Option<AndroidActivityGeneration>,
    media_playlist_snapshot: Option<Playlist>,
    media_projection_pending: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AndroidSystemInsets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl AndroidRuntime {
    pub fn new() -> Self {
        Self {
            persistence_dirty: false,
            last_persist: Instant::now(),
            layout: LayoutState::default(),
            layout_view: AndroidLayoutView::Unavailable,
            #[cfg(target_os = "android")]
            playlist_manager: PlaylistManager::new(),
            #[cfg(target_os = "android")]
            activity_generation: None,
            media_playlist_snapshot: None,
            media_projection_pending: true,
        }
    }

    pub fn mark_persistence(&mut self) {
        self.persistence_dirty = true;
    }

    #[cfg(target_os = "android")]
    pub fn bind_activity(&mut self, activity_generation: AndroidActivityGeneration) {
        self.activity_generation = Some(activity_generation);
    }

    #[cfg(target_os = "android")]
    pub fn activity_generation(&self) -> AndroidActivityGeneration {
        self.activity_generation
            .expect("Android runtime must be bound to its Activity before the first frame")
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
        self.persistence_dirty
            .then(|| PERSIST_INTERVAL.saturating_sub(self.last_persist.elapsed()))
    }

    pub fn playlist_changed(&self, playlist: &Playlist) -> bool {
        self.media_playlist_snapshot.as_ref() != Some(playlist)
    }

    pub fn mark_media_projection(&mut self) {
        self.media_projection_pending = true;
    }

    pub fn take_media_projection_pending(&mut self) -> bool {
        std::mem::take(&mut self.media_projection_pending)
    }

    pub fn remember_playlist(&mut self, playlist: Playlist) {
        self.media_playlist_snapshot = Some(playlist);
    }

    pub fn observe_layout(
        &mut self,
        snapshot: Option<AndroidLayoutSnapshot>,
    ) -> AndroidLayoutUpdate {
        let Some(snapshot) = snapshot else {
            self.layout_view = AndroidLayoutView::Unavailable;
            return AndroidLayoutUpdate {
                orientation_changed: false,
                repaint: AndroidLayoutRepaint::AwaitingReadiness,
            };
        };
        let orientation = snapshot.orientation;
        let orientation_changed = self.layout.orientation() != Some(orientation);
        if !snapshot.is_consistent() {
            self.layout.phase = AndroidLayoutPhase::Transitioning { orientation };
            self.layout_view = AndroidLayoutView::Transitioning {
                orientation,
                width: snapshot.width,
                height: snapshot.height,
            };
            return AndroidLayoutUpdate {
                orientation_changed,
                repaint: AndroidLayoutRepaint::AwaitingReadiness,
            };
        }
        let stabilizing = self.layout.accept(orientation);
        self.layout_view = AndroidLayoutView::Ready(snapshot);
        AndroidLayoutUpdate {
            orientation_changed,
            repaint: if stabilizing {
                AndroidLayoutRepaint::Stabilizing
            } else {
                AndroidLayoutRepaint::None
            },
        }
    }

    pub fn layout_view(&self) -> AndroidLayoutView {
        self.layout_view
    }

    pub fn ready_layout(&self) -> Option<AndroidLayoutSnapshot> {
        match self.layout_view {
            AndroidLayoutView::Ready(snapshot) => Some(snapshot),
            AndroidLayoutView::Unavailable | AndroidLayoutView::Transitioning { .. } => None,
        }
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
pub(crate) struct AndroidLayoutSnapshot {
    pub width: i32,
    pub height: i32,
    pub orientation: AndroidLayoutOrientation,
    pub insets: AndroidSystemInsets,
    pub inset_width: i32,
    pub inset_height: i32,
    pub inset_orientation: i32,
    pub config_generation: i64,
    pub inset_generation: i64,
    pub insets_fresh: bool,
}

impl AndroidLayoutSnapshot {
    fn is_consistent(self) -> bool {
        layout_snapshot_is_consistent(
            self.width,
            self.height,
            self.orientation,
            [
                self.insets.left,
                self.insets.top,
                self.insets.right,
                self.insets.bottom,
            ],
            self.inset_width,
            self.inset_height,
            self.inset_orientation,
            self.config_generation,
            self.inset_generation,
            self.insets_fresh,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AndroidLayoutView {
    Unavailable,
    Transitioning {
        orientation: AndroidLayoutOrientation,
        width: i32,
        height: i32,
    },
    Ready(AndroidLayoutSnapshot),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AndroidLayoutRepaint {
    None,
    AwaitingReadiness,
    Stabilizing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AndroidLayoutUpdate {
    pub orientation_changed: bool,
    pub repaint: AndroidLayoutRepaint,
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
        let mut runtime = AndroidRuntime::new();
        assert_eq!(runtime.persistence_delay(), None);

        runtime.mark_persistence();

        assert!(runtime
            .persistence_delay()
            .is_some_and(|delay| delay <= PERSIST_INTERVAL));
        assert!(runtime.take_persistence_due(true));
        assert_eq!(runtime.persistence_delay(), None);
    }

    #[test]
    fn layout_stabilizes_in_both_orientation_directions() {
        let mut state = LayoutState::default();

        assert!(state.accept(AndroidLayoutOrientation::Portrait));
        assert!(state.accept(AndroidLayoutOrientation::Portrait));
        assert!(state.accept(AndroidLayoutOrientation::Portrait));
        assert!(state.accept(AndroidLayoutOrientation::Portrait));
        assert!(!state.accept(AndroidLayoutOrientation::Portrait));
        assert_eq!(
            state.orientation(),
            Some(AndroidLayoutOrientation::Portrait)
        );

        state.phase = AndroidLayoutPhase::Transitioning {
            orientation: AndroidLayoutOrientation::Landscape,
        };
        assert!(state.accept(AndroidLayoutOrientation::Landscape));
        assert!(state.accept(AndroidLayoutOrientation::Landscape));
        assert!(state.accept(AndroidLayoutOrientation::Landscape));
        assert!(state.accept(AndroidLayoutOrientation::Landscape));
        assert!(!state.accept(AndroidLayoutOrientation::Landscape));
        assert_eq!(
            state.orientation(),
            Some(AndroidLayoutOrientation::Landscape)
        );
    }

    #[test]
    fn layout_phase_transition_table_is_explicit() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum PhaseKind {
            Uninitialized,
            Transitioning,
            Stabilizing(u8),
            Stable,
        }

        fn kind(phase: AndroidLayoutPhase) -> PhaseKind {
            match phase {
                AndroidLayoutPhase::Uninitialized => PhaseKind::Uninitialized,
                AndroidLayoutPhase::Transitioning { .. } => PhaseKind::Transitioning,
                AndroidLayoutPhase::Stabilizing {
                    frames_remaining, ..
                } => PhaseKind::Stabilizing(frames_remaining),
                AndroidLayoutPhase::Stable { .. } => PhaseKind::Stable,
            }
        }

        struct Case {
            initial: AndroidLayoutPhase,
            observed: AndroidLayoutOrientation,
            expected: PhaseKind,
            repaint: bool,
        }

        let portrait = AndroidLayoutOrientation::Portrait;
        let landscape = AndroidLayoutOrientation::Landscape;
        for case in [
            Case {
                initial: AndroidLayoutPhase::Uninitialized,
                observed: portrait,
                expected: PhaseKind::Stabilizing(POST_ROTATION_REPAINT_FRAMES),
                repaint: true,
            },
            Case {
                initial: AndroidLayoutPhase::Transitioning {
                    orientation: portrait,
                },
                observed: portrait,
                expected: PhaseKind::Stabilizing(POST_ROTATION_REPAINT_FRAMES),
                repaint: true,
            },
            Case {
                initial: AndroidLayoutPhase::Stabilizing {
                    orientation: portrait,
                    frames_remaining: 2,
                },
                observed: portrait,
                expected: PhaseKind::Stabilizing(1),
                repaint: true,
            },
            Case {
                initial: AndroidLayoutPhase::Stabilizing {
                    orientation: portrait,
                    frames_remaining: 1,
                },
                observed: portrait,
                expected: PhaseKind::Stable,
                repaint: true,
            },
            Case {
                initial: AndroidLayoutPhase::Stable {
                    orientation: portrait,
                },
                observed: portrait,
                expected: PhaseKind::Stable,
                repaint: false,
            },
            Case {
                initial: AndroidLayoutPhase::Stable {
                    orientation: portrait,
                },
                observed: landscape,
                expected: PhaseKind::Stabilizing(POST_ROTATION_REPAINT_FRAMES),
                repaint: true,
            },
        ] {
            let mut state = LayoutState {
                phase: case.initial,
                ..LayoutState::default()
            };
            assert_eq!(state.accept(case.observed), case.repaint);
            assert_eq!(kind(state.phase), case.expected);
        }
    }

    #[test]
    fn observing_layout_drives_lifecycle_before_rendering() {
        let mut runtime = AndroidRuntime::new();
        let portrait = AndroidLayoutSnapshot {
            width: 1080,
            height: 2400,
            orientation: AndroidLayoutOrientation::Portrait,
            insets: AndroidSystemInsets {
                left: 0,
                top: 80,
                right: 0,
                bottom: 120,
            },
            inset_width: 1080,
            inset_height: 2400,
            inset_orientation: 1,
            config_generation: 4,
            inset_generation: 4,
            insets_fresh: true,
        };

        assert_eq!(
            runtime.observe_layout(None),
            AndroidLayoutUpdate {
                orientation_changed: false,
                repaint: AndroidLayoutRepaint::AwaitingReadiness,
            }
        );
        assert_eq!(
            runtime.observe_layout(Some(portrait)),
            AndroidLayoutUpdate {
                orientation_changed: true,
                repaint: AndroidLayoutRepaint::Stabilizing,
            }
        );
        assert!(matches!(runtime.layout_view(), AndroidLayoutView::Ready(_)));

        for _ in 0..POST_ROTATION_REPAINT_FRAMES {
            runtime.observe_layout(Some(portrait));
        }
        assert_eq!(
            runtime.observe_layout(Some(portrait)).repaint,
            AndroidLayoutRepaint::None
        );

        let stale = AndroidLayoutSnapshot {
            inset_generation: 3,
            ..portrait
        };
        assert_eq!(
            runtime.observe_layout(Some(stale)).repaint,
            AndroidLayoutRepaint::AwaitingReadiness
        );
        assert!(matches!(
            runtime.layout_view(),
            AndroidLayoutView::Transitioning {
                orientation: AndroidLayoutOrientation::Portrait,
                ..
            }
        ));
    }

    #[test]
    fn layout_snapshot_rejects_stale_generation_and_orientation() {
        assert!(layout_snapshot_is_consistent(
            1080,
            2400,
            AndroidLayoutOrientation::Portrait,
            [0, 80, 0, 120],
            1080,
            2400,
            1,
            4,
            4,
            true,
        ));
        assert!(!layout_snapshot_is_consistent(
            1080,
            2400,
            AndroidLayoutOrientation::Portrait,
            [0, 80, 0, 120],
            1080,
            2400,
            1,
            5,
            4,
            true,
        ));
        assert!(!layout_snapshot_is_consistent(
            1080,
            2400,
            AndroidLayoutOrientation::Landscape,
            [0, 80, 0, 120],
            1080,
            2400,
            2,
            4,
            4,
            true,
        ));
    }
}
