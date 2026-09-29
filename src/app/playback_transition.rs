//! Playback lifecycle decisions independent of either frontend's timer and widgets.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaybackTransition {
    Idle,
    StoppedAt(i64),
    PendingSeek(i64),
    AwaitingSeek {
        target_ms: i64,
        previous_ms: Option<i64>,
        elapsed_ms: i64,
    },
    WaitingBetweenSongs(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransitionEvent {
    Start(i64),
    Stop,
    StoppedPosition(i64),
    SeekRequested(i64),
    SeekApplied { previous_ms: Option<i64> },
    SeekFailed { final_attempt: bool },
    EofWait(i64),
    CancelWait,
}

impl PlaybackTransition {
    pub(crate) fn transition(self, event: TransitionEvent) -> Self {
        match event {
            TransitionEvent::Start(position) | TransitionEvent::SeekRequested(position) => {
                if position > 0 {
                    Self::PendingSeek(position)
                } else {
                    Self::Idle
                }
            }
            TransitionEvent::Stop | TransitionEvent::CancelWait => Self::Idle,
            TransitionEvent::StoppedPosition(position) => {
                if position > 0 {
                    Self::StoppedAt(position)
                } else {
                    Self::Idle
                }
            }
            TransitionEvent::SeekApplied { previous_ms } => match self {
                Self::PendingSeek(target_ms) => Self::AwaitingSeek {
                    target_ms,
                    previous_ms,
                    elapsed_ms: 0,
                },
                other => other,
            },
            TransitionEvent::SeekFailed {
                final_attempt: true,
            } => Self::Idle,
            TransitionEvent::SeekFailed {
                final_attempt: false,
            } => self,
            TransitionEvent::EofWait(duration) => Self::WaitingBetweenSongs(duration),
        }
    }

    pub(crate) fn pending_seek(self) -> Option<i64> {
        if let Self::PendingSeek(position) = self {
            Some(position)
        } else {
            None
        }
    }

    pub(crate) fn wait_remaining(self) -> Option<i64> {
        if let Self::WaitingBetweenSongs(remaining) = self {
            Some(remaining)
        } else {
            None
        }
    }

    pub(crate) fn needs_fast_tick(self) -> bool {
        matches!(
            self,
            Self::WaitingBetweenSongs(_) | Self::PendingSeek(_) | Self::AwaitingSeek { .. }
        )
    }

    // Returns a confirmed position only: old backend samples must not undo a
    // requested seek or the zero position displayed during an EOF pause.
    pub(crate) fn observe_position(self, position: i64) -> (Self, Option<i64>) {
        match self {
            Self::PendingSeek(_) | Self::WaitingBetweenSongs(_) | Self::StoppedAt(_) => {
                (self, None)
            }
            Self::AwaitingSeek {
                target_ms,
                previous_ms,
                elapsed_ms,
            } => {
                // Backend samples can arrive after playback has already moved
                // beyond the old 250-ms confirmation window. A sample ahead of
                // the target also confirms a seek when it has crossed away from
                // the pre-seek position; a backward seek must not accept the
                // old, still-advancing position as confirmation.
                let crossed_from_previous = previous_ms.is_some_and(|previous| {
                    previous <= target_ms.saturating_add(250)
                        || position < previous.saturating_sub(250)
                });
                // A bounded fallback handles unavailable or ambiguous origins,
                // but only when the sample is within plausible playback time
                // since the seek. An old position far past a backward target
                // must not become valid merely because a timer expired.
                let plausible_late_sample =
                    position <= target_ms.saturating_add(elapsed_ms).saturating_add(500);
                let late_without_origin = previous_ms.is_none() && elapsed_ms >= 1_000;
                let timed_fallback = elapsed_ms >= 2_000;
                if position.abs_diff(target_ms) <= 250
                    || (position >= target_ms
                        && (crossed_from_previous
                            || (plausible_late_sample && (late_without_origin || timed_fallback))))
                {
                    (Self::Idle, Some(position.max(target_ms)))
                } else {
                    (self, None)
                }
            }
            Self::Idle => (self, Some(position)),
        }
    }

    pub(crate) fn tick(self, elapsed_ms: i64) -> (Self, bool, bool) {
        let Self::WaitingBetweenSongs(remaining) = self else {
            if let Self::AwaitingSeek {
                target_ms,
                previous_ms,
                elapsed_ms: previous_elapsed,
            } = self
            {
                return (
                    Self::AwaitingSeek {
                        target_ms,
                        previous_ms,
                        elapsed_ms: previous_elapsed.saturating_add(elapsed_ms.max(0)),
                    },
                    false,
                    false,
                );
            }
            return (self, false, false);
        };
        let remaining = remaining.saturating_sub(elapsed_ms.max(0));
        if remaining <= 0 {
            (Self::Idle, true, true)
        } else {
            (Self::WaitingBetweenSongs(remaining), true, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn awaiting(target_ms: i64, previous_ms: Option<i64>, elapsed_ms: i64) -> PlaybackTransition {
        PlaybackTransition::AwaitingSeek {
            target_ms,
            previous_ms,
            elapsed_ms,
        }
    }

    #[test]
    fn lifecycle_transition_table() {
        use PlaybackTransition as S;
        use TransitionEvent as E;
        let cases = [
            (S::Idle, E::StoppedPosition(42), S::StoppedAt(42)),
            (S::StoppedAt(42), E::Start(42), S::PendingSeek(42)),
            (
                S::PendingSeek(42),
                E::SeekApplied {
                    previous_ms: Some(0),
                },
                awaiting(42, Some(0), 0),
            ),
            (
                S::PendingSeek(42),
                E::SeekFailed {
                    final_attempt: false,
                },
                S::PendingSeek(42),
            ),
            (
                S::PendingSeek(42),
                E::SeekFailed {
                    final_attempt: true,
                },
                S::Idle,
            ),
            (awaiting(42, Some(0), 0), E::Start(0), S::Idle),
            (S::WaitingBetweenSongs(2000), E::Start(0), S::Idle),
            (S::WaitingBetweenSongs(2000), E::CancelWait, S::Idle),
            (S::WaitingBetweenSongs(2000), E::Stop, S::Idle),
            (awaiting(42, Some(0), 0), E::Stop, S::Idle),
        ];
        for (before, event, expected) in cases {
            assert_eq!(before.transition(event), expected, "{before:?} + {event:?}");
        }
    }

    #[test]
    fn tick_and_position_observation_table() {
        use PlaybackTransition as S;
        for (before, elapsed, expected) in [
            (S::Idle, 1000, (S::Idle, false, false)),
            (
                S::WaitingBetweenSongs(2000),
                -1,
                (S::WaitingBetweenSongs(2000), true, false),
            ),
            (
                S::WaitingBetweenSongs(2000),
                1000,
                (S::WaitingBetweenSongs(1000), true, false),
            ),
            (S::WaitingBetweenSongs(1000), 1000, (S::Idle, true, true)),
            (
                S::WaitingBetweenSongs(1000),
                i64::MAX,
                (S::Idle, true, true),
            ),
        ] {
            assert_eq!(before.tick(elapsed), expected);
        }
        assert_eq!(
            awaiting(5000, Some(20000), 0).tick(2000),
            (awaiting(5000, Some(20000), 2000), false, false)
        );
        for (before, sample, expected) in [
            (S::Idle, 500, (S::Idle, Some(500))),
            (S::StoppedAt(5000), 0, (S::StoppedAt(5000), None)),
            (S::PendingSeek(5000), 0, (S::PendingSeek(5000), None)),
            (
                awaiting(5000, Some(0), 0),
                0,
                (awaiting(5000, Some(0), 0), None),
            ),
            (awaiting(5000, Some(0), 0), 4800, (S::Idle, Some(5000))),
            // A forward seek may already be playing past the 250-ms window.
            (awaiting(5000, Some(100), 300), 5300, (S::Idle, Some(5300))),
            // A backward seek must not accept a stale pre-seek sample.
            (
                awaiting(5000, Some(20000), 300),
                20100,
                (awaiting(5000, Some(20000), 300), None),
            ),
            (
                awaiting(5000, Some(20000), 300),
                5300,
                (S::Idle, Some(5300)),
            ),
            (awaiting(5000, None, 1000), 5300, (S::Idle, Some(5300))),
            (
                awaiting(5000, Some(20000), 2000),
                22000,
                (awaiting(5000, Some(20000), 2000), None),
            ),
            (
                awaiting(5000, None, 2000),
                22000,
                (awaiting(5000, None, 2000), None),
            ),
            (
                S::WaitingBetweenSongs(1000),
                5000,
                (S::WaitingBetweenSongs(1000), None),
            ),
        ] {
            assert_eq!(before.observe_position(sample), expected);
        }
    }
}
