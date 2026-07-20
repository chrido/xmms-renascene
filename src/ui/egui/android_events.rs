//! Typed Android ingress.
//!
//! JNI callbacks append to this inbox and request a repaint. The egui runtime
//! drains at most one bounded batch at the start of the next frame. Local egui
//! commands are dispatched in the current frame; queued Android commands keep
//! FIFO order and are dispatched before that frame's local input.

//! Ordered Android-to-egui event ingress.
//!
//! Local egui controls dispatch during the current frame. JNI callbacks only
//! enqueue here, so notification and platform events are applied at the start
//! of the next frame. Ordered commands remain FIFO; replaceable volume and
//! spectrum samples are coalesced. A frame drains at most 256 events and
//! requests another repaint when work remains.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use crate::app::effect::FileDialogRequest;
use crate::playback::model::{PlaybackEvent, PlayerState};

pub struct AndroidPickerResult {
    pub request: FileDialogRequest,
    pub paths: Vec<PathBuf>,
    pub error: Option<String>,
}

impl AndroidPickerResult {
    pub fn is_cancelled(&self) -> bool {
        self.error.is_none() && self.paths.is_empty()
    }
}

pub enum AndroidPlatformEvent {
    Picker(AndroidPickerResult),
    MediaControl(AndroidMediaControlEvent),
    Playback(PlaybackEvent),
    ExternalVolumeChanged(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AndroidMediaControlEvent {
    pub control: AndroidMediaControl,
    pub backend_executed: bool,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidPlaybackState {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
}

impl From<PlayerState> for AndroidPlaybackState {
    fn from(state: PlayerState) -> Self {
        match state {
            PlayerState::Stopped => Self::Stopped,
            PlayerState::Playing => Self::Playing,
            PlayerState::Paused => Self::Paused,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidMediaControl {
    PausePlayback,
    ResumePlayback,
    NextTrack,
    PreviousTrack,
    SeekToMs(i64),
    PlayMediaItem(usize),
    StopPlayback,
    PlaylistEof,
}

pub const MAX_PLATFORM_EVENTS_PER_FRAME: usize = 256;

#[derive(Default)]
pub struct AndroidEventInbox {
    events: VecDeque<AndroidPlatformEvent>,
}

impl AndroidEventInbox {
    pub fn push(&mut self, event: AndroidPlatformEvent) {
        match &event {
            AndroidPlatformEvent::ExternalVolumeChanged(_) => self
                .events
                .retain(|queued| !matches!(queued, AndroidPlatformEvent::ExternalVolumeChanged(_))),
            AndroidPlatformEvent::Playback(PlaybackEvent::Spectrum(_)) => {
                self.events.retain(|queued| {
                    !matches!(
                        queued,
                        AndroidPlatformEvent::Playback(PlaybackEvent::Spectrum(_))
                    )
                })
            }
            _ => {}
        }
        self.events.push_back(event);
    }

    pub fn drain_frame(&mut self) -> (Vec<AndroidPlatformEvent>, bool) {
        let count = self.events.len().min(MAX_PLATFORM_EVENTS_PER_FRAME);
        let events = self.events.drain(..count).collect();
        (events, !self.events.is_empty())
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn remove_media_controls(&mut self) {
        self.events
            .retain(|event| !matches!(event, AndroidPlatformEvent::MediaControl(_)));
    }
}

#[derive(Default)]
pub struct PickerOperationRegistry {
    next_id: u64,
    active: HashMap<i32, u64>,
}

impl PickerOperationRegistry {
    pub fn replace_activity(&mut self) {
        self.active.clear();
    }

    pub fn begin(&mut self, request_code: i32) -> Option<u64> {
        if self.active.contains_key(&request_code) {
            return None;
        }
        self.next_id = self.next_id.saturating_add(1);
        self.active.insert(request_code, self.next_id);
        Some(self.next_id)
    }

    pub fn cancel(&mut self, request_code: i32, operation_id: u64) {
        if self.active.get(&request_code) == Some(&operation_id) {
            self.active.remove(&request_code);
        }
    }

    pub fn complete(&mut self, request_code: i32, operation_id: u64) -> bool {
        if self.active.get(&request_code) != Some(&operation_id) {
            return false;
        }
        self.active.remove(&request_code);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(control: AndroidMediaControl) -> AndroidPlatformEvent {
        AndroidPlatformEvent::MediaControl(AndroidMediaControlEvent {
            control,
            backend_executed: false,
        })
    }

    #[test]
    fn inbox_preserves_ordered_commands_and_coalesces_replaceable_events() {
        let mut inbox = AndroidEventInbox::default();
        inbox.push(media(AndroidMediaControl::NextTrack));
        inbox.push(AndroidPlatformEvent::ExternalVolumeChanged(10));
        inbox.push(AndroidPlatformEvent::ExternalVolumeChanged(20));
        inbox.push(media(AndroidMediaControl::PreviousTrack));

        let (events, more) = inbox.drain_frame();

        assert!(!more);
        assert!(matches!(
            events.as_slice(),
            [
                AndroidPlatformEvent::MediaControl(AndroidMediaControlEvent {
                    control: AndroidMediaControl::NextTrack,
                    ..
                }),
                AndroidPlatformEvent::ExternalVolumeChanged(20),
                AndroidPlatformEvent::MediaControl(AndroidMediaControlEvent {
                    control: AndroidMediaControl::PreviousTrack,
                    ..
                })
            ]
        ));
    }

    #[test]
    fn inbox_bounds_each_frame_without_dropping_events() {
        let mut inbox = AndroidEventInbox::default();
        for _ in 0..=MAX_PLATFORM_EVENTS_PER_FRAME {
            inbox.push(media(AndroidMediaControl::NextTrack));
        }

        let (first, more) = inbox.drain_frame();
        let (second, remaining) = inbox.drain_frame();

        assert_eq!(first.len(), MAX_PLATFORM_EVENTS_PER_FRAME);
        assert!(more);
        assert_eq!(second.len(), 1);
        assert!(!remaining);
    }

    #[test]
    fn local_player_input_supersedes_only_queued_media_controls() {
        let mut inbox = AndroidEventInbox::default();
        inbox.push(media(AndroidMediaControl::NextTrack));
        inbox.push(AndroidPlatformEvent::ExternalVolumeChanged(42));
        inbox.push(media(AndroidMediaControl::PreviousTrack));

        inbox.remove_media_controls();
        let (events, more) = inbox.drain_frame();

        assert!(!more);
        assert!(matches!(
            events.as_slice(),
            [AndroidPlatformEvent::ExternalVolumeChanged(42)]
        ));
    }

    #[test]
    fn replacing_activity_rejects_delayed_picker_results() {
        let mut operations = PickerOperationRegistry::default();
        let stale = operations.begin(100).unwrap();

        operations.replace_activity();
        let current = operations.begin(100).unwrap();

        assert!(!operations.complete(100, stale));
        assert!(operations.complete(100, current));
        assert!(!operations.complete(100, current));
    }

    #[test]
    fn overlapping_picker_requests_of_the_same_kind_are_rejected() {
        let mut operations = PickerOperationRegistry::default();
        let current = operations.begin(107).unwrap();

        assert_eq!(operations.begin(107), None);
        assert!(operations.complete(107, current));
        assert!(operations.begin(107).is_some());
    }

    #[test]
    fn picker_operation_transition_table_rejects_stale_completions() {
        struct Case {
            replace_activity: bool,
            cancel_first: bool,
            completion_is_current: bool,
            accepted: bool,
        }

        for case in [
            Case {
                replace_activity: false,
                cancel_first: false,
                completion_is_current: true,
                accepted: true,
            },
            Case {
                replace_activity: false,
                cancel_first: true,
                completion_is_current: true,
                accepted: false,
            },
            Case {
                replace_activity: true,
                cancel_first: false,
                completion_is_current: false,
                accepted: false,
            },
            Case {
                replace_activity: true,
                cancel_first: false,
                completion_is_current: true,
                accepted: true,
            },
        ] {
            let mut operations = PickerOperationRegistry::default();
            let first = operations.begin(100).unwrap();
            if case.cancel_first {
                operations.cancel(100, first);
            }
            let current = if case.replace_activity {
                operations.replace_activity();
                operations.begin(100).unwrap()
            } else {
                first
            };
            let completion = if case.completion_is_current {
                current
            } else {
                first
            };

            assert_eq!(
                operations.complete(100, completion),
                case.accepted,
                "replace_activity={}, cancel_first={}, completion_is_current={}",
                case.replace_activity,
                case.cancel_first,
                case.completion_is_current
            );
        }
    }

    #[test]
    fn empty_successful_picker_result_is_cancellation() {
        let result = AndroidPickerResult {
            request: FileDialogRequest::ImportSkin,
            paths: Vec::new(),
            error: None,
        };

        assert!(result.is_cancelled());
    }
}
