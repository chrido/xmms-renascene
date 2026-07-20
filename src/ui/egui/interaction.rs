use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GestureAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug)]
pub(crate) enum PlaylistTouchGesture {
    Idle,
    Tracking {
        start: egui::Pos2,
        started_at: Instant,
        row: Option<usize>,
        accumulated: egui::Vec2,
        largest_observed: egui::Vec2,
        axis: Option<GestureAxis>,
        scroll_remainder: f32,
    },
}

impl Default for PlaylistTouchGesture {
    fn default() -> Self {
        Self::Idle
    }
}

pub(crate) struct PlaylistGestureRelease {
    pub delta: egui::Vec2,
    pub row: Option<usize>,
    pub duration: Duration,
}

impl PlaylistTouchGesture {
    const AXIS_THRESHOLD: f32 = 8.0;

    pub fn begin(&mut self, start: egui::Pos2, row: Option<usize>) {
        *self = Self::Tracking {
            start,
            started_at: Instant::now(),
            row,
            accumulated: egui::Vec2::ZERO,
            largest_observed: egui::Vec2::ZERO,
            axis: None,
            scroll_remainder: 0.0,
        };
    }

    pub fn start(&self) -> Option<egui::Pos2> {
        match self {
            Self::Idle => None,
            Self::Tracking { start, .. } => Some(*start),
        }
    }

    pub fn observe(&mut self, delta: egui::Vec2) {
        let Self::Tracking {
            largest_observed, ..
        } = self
        else {
            return;
        };
        if delta.length_sq() > largest_observed.length_sq() {
            *largest_observed = delta;
        }
    }

    pub fn drag(&mut self, total_delta: egui::Vec2, row_height: f32) -> i32 {
        let Self::Tracking {
            accumulated,
            axis,
            scroll_remainder,
            ..
        } = self
        else {
            return 0;
        };
        let frame_delta = total_delta - *accumulated;
        *accumulated = total_delta;
        if axis.is_none()
            && (total_delta.x.abs() >= Self::AXIS_THRESHOLD
                || total_delta.y.abs() >= Self::AXIS_THRESHOLD)
        {
            *axis = Some(if total_delta.x.abs() > total_delta.y.abs() {
                GestureAxis::Horizontal
            } else {
                GestureAxis::Vertical
            });
        }
        if *axis != Some(GestureAxis::Vertical) {
            return 0;
        }
        *scroll_remainder += -frame_delta.y / row_height;
        let rows = scroll_remainder.trunc() as i32;
        *scroll_remainder -= rows as f32;
        rows
    }

    pub fn release(&mut self, pointer: Option<egui::Pos2>) -> Option<PlaylistGestureRelease> {
        let Self::Tracking {
            start,
            started_at,
            row,
            accumulated,
            largest_observed,
            ..
        } = std::mem::take(self)
        else {
            return None;
        };
        let endpoint = pointer.map_or(accumulated, |end| end - start);
        let delta = [endpoint, accumulated, largest_observed]
            .into_iter()
            .max_by(|left, right| left.length_sq().total_cmp(&right.length_sq()))
            .unwrap_or_default();
        Some(PlaylistGestureRelease {
            delta,
            row,
            duration: started_at.elapsed(),
        })
    }

    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(gesture: &PlaylistTouchGesture) -> Option<GestureAxis> {
        match gesture {
            PlaylistTouchGesture::Idle => None,
            PlaylistTouchGesture::Tracking { axis, .. } => *axis,
        }
    }

    #[test]
    fn horizontal_drag_locks_axis_without_scrolling() {
        let mut gesture = PlaylistTouchGesture::default();
        gesture.begin(egui::pos2(10.0, 20.0), Some(3));

        assert_eq!(gesture.drag(egui::vec2(12.0, 2.0), 11.0), 0);
        assert_eq!(axis(&gesture), Some(GestureAxis::Horizontal));
        let release = gesture.release(Some(egui::pos2(22.0, 22.0))).unwrap();

        assert_eq!(release.row, Some(3));
        assert_eq!(release.delta, egui::vec2(12.0, 2.0));
        assert!(!gesture.is_active());
    }

    #[test]
    fn vertical_drag_accumulates_fractional_row_scroll() {
        let mut gesture = PlaylistTouchGesture::default();
        gesture.begin(egui::Pos2::ZERO, None);

        assert_eq!(gesture.drag(egui::vec2(1.0, -8.0), 11.0), 0);
        assert_eq!(axis(&gesture), Some(GestureAxis::Vertical));
        assert_eq!(gesture.drag(egui::vec2(1.0, -15.0), 11.0), 1);
        assert_eq!(gesture.drag(egui::vec2(1.0, -24.0), 11.0), 1);
    }

    #[test]
    fn release_without_motion_is_a_tap_and_resets_tracking() {
        let mut gesture = PlaylistTouchGesture::default();
        gesture.begin(egui::pos2(5.0, 7.0), Some(1));

        let release = gesture.release(Some(egui::pos2(5.0, 7.0))).unwrap();

        assert_eq!(release.delta, egui::Vec2::ZERO);
        assert_eq!(release.row, Some(1));
        assert!(!gesture.is_active());
        assert!(gesture.release(None).is_none());
    }

    #[test]
    fn release_uses_largest_observed_delta_when_pointer_is_cancelled() {
        let mut gesture = PlaylistTouchGesture::default();
        gesture.begin(egui::Pos2::ZERO, Some(2));
        gesture.observe(egui::vec2(50.0, 4.0));
        gesture.drag(egui::vec2(20.0, 2.0), 11.0);

        let release = gesture.release(None).unwrap();

        assert_eq!(release.delta, egui::vec2(50.0, 4.0));
        assert!(!gesture.is_active());
    }
}
