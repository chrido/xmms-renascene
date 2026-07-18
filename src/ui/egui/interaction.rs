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
