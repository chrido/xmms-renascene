//! Android media-playlist authority state shared by runtime code and host tests.
//!
//! The foreground Activity owns domain transitions while the playlist is a
//! mirror. Once that Activity is paused, replaced, destroyed, or its egui
//! runtime exits, the service-side copy becomes authoritative until the current
//! Activity resumes.

use crate::playlist::{Playlist, TrackDirection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AndroidActivityGeneration(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AndroidMediaPlaylistAuthority {
    Uninitialized,
    Mirror(AndroidActivityGeneration),
    Authoritative,
}

#[derive(Debug, Clone)]
pub(crate) struct AndroidMediaPlaylist {
    pub(crate) playlist: Playlist,
    pub(crate) titles: Vec<String>,
}

impl AndroidMediaPlaylist {
    pub(crate) fn new(playlist: Playlist, titles: Vec<String>) -> Self {
        Self { playlist, titles }
    }

    pub(crate) fn current_entry(&self) -> Option<(String, String, i64)> {
        let position = self.playlist.position()?;
        let entry = self.playlist.entries().get(position)?;
        let title = self
            .titles
            .get(position)
            .cloned()
            .unwrap_or_else(|| entry.title.clone());
        Some((entry.filename.clone(), title, entry.length_ms))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) enum AndroidMediaPlaylistState {
    #[default]
    Uninitialized,
    Mirror {
        activity: AndroidActivityGeneration,
        media: AndroidMediaPlaylist,
    },
    Authoritative(AndroidMediaPlaylist),
}

impl AndroidMediaPlaylistState {
    pub(crate) fn authority(&self) -> AndroidMediaPlaylistAuthority {
        match self {
            Self::Uninitialized => AndroidMediaPlaylistAuthority::Uninitialized,
            Self::Mirror { activity, .. } => AndroidMediaPlaylistAuthority::Mirror(*activity),
            Self::Authoritative(_) => AndroidMediaPlaylistAuthority::Authoritative,
        }
    }

    pub(crate) fn initialize_authoritative(&mut self, media: AndroidMediaPlaylist) -> bool {
        if !matches!(self, Self::Uninitialized) {
            return false;
        }
        *self = Self::Authoritative(media);
        true
    }

    pub(crate) fn replace_activity(&mut self, activity: AndroidActivityGeneration, resumed: bool) {
        self.transition_loaded(resumed.then_some(activity));
    }

    pub(crate) fn activity_resumed(&mut self, activity: AndroidActivityGeneration) {
        self.transition_loaded(Some(activity));
    }

    pub(crate) fn activity_paused_or_exited(&mut self, activity: AndroidActivityGeneration) {
        let Self::Mirror {
            activity: owner, ..
        } = self
        else {
            return;
        };
        if *owner == activity {
            self.transition_loaded(None);
        }
    }

    pub(crate) fn is_mirror_for(&self, activity: AndroidActivityGeneration) -> bool {
        matches!(
            self,
            Self::Mirror {
                activity: owner,
                ..
            } if *owner == activity
        )
    }

    pub(crate) fn sync_mirror(
        &mut self,
        activity: AndroidActivityGeneration,
        media: AndroidMediaPlaylist,
    ) -> bool {
        let Self::Mirror {
            activity: owner,
            media: current,
        } = self
        else {
            return false;
        };
        if *owner != activity {
            return false;
        }
        *current = media;
        true
    }

    pub(crate) fn media(&self) -> Option<&AndroidMediaPlaylist> {
        match self {
            Self::Uninitialized => None,
            Self::Mirror { media, .. } | Self::Authoritative(media) => Some(media),
        }
    }

    pub(crate) fn authoritative_mut(&mut self) -> Option<AndroidAuthoritativeMediaPlaylist<'_>> {
        match self {
            Self::Authoritative(media) => Some(AndroidAuthoritativeMediaPlaylist { media }),
            Self::Uninitialized | Self::Mirror { .. } => None,
        }
    }

    fn transition_loaded(&mut self, mirror: Option<AndroidActivityGeneration>) {
        let previous = std::mem::take(self);
        *self = match previous {
            Self::Uninitialized => Self::Uninitialized,
            Self::Mirror { media, .. } | Self::Authoritative(media) => match mirror {
                Some(activity) => Self::Mirror { activity, media },
                None => Self::Authoritative(media),
            },
        };
    }
}

pub(crate) struct AndroidAuthoritativeMediaPlaylist<'a> {
    media: &'a mut AndroidMediaPlaylist,
}

impl AndroidAuthoritativeMediaPlaylist<'_> {
    pub(crate) fn start_current(
        &mut self,
        play_uri: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut updated = self.media.clone();
        if updated.playlist.position().is_none() && !updated.playlist.is_empty() {
            updated.playlist.set_position(0);
        }
        let position = updated
            .playlist
            .position()
            .ok_or_else(|| "Android media playlist has no current entry".to_string())?;
        let uri = updated.playlist.entries()[position].filename.clone();
        play_uri(&uri)?;
        *self.media = updated;
        Ok(())
    }

    pub(crate) fn change_track(
        &mut self,
        direction: TrackDirection,
        play_uri: impl FnOnce(&str) -> Result<(), String>,
        seek_start: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let mut updated = self.media.clone();
        let advanced = updated.playlist.move_track(direction);
        if !advanced {
            return seek_start();
        }
        let position = updated
            .playlist
            .position()
            .ok_or_else(|| "Android media playlist has no current entry".to_string())?;
        let uri = updated.playlist.entries()[position].filename.clone();
        play_uri(&uri)?;
        *self.media = updated;
        Ok(())
    }

    pub(crate) fn play_media_item(
        &mut self,
        index: usize,
        play_uri: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut updated = self.media.clone();
        let uri = updated
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.filename.clone())
            .ok_or_else(|| format!("Android media item index {index} is out of range"))?;
        updated.playlist.set_position(index);
        play_uri(&uri)?;
        *self.media = updated;
        Ok(())
    }

    pub(crate) fn advance_after_end_of_stream(
        &mut self,
        play_uri: impl FnOnce(&str) -> Result<(), String>,
        stop: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let mut updated = self.media.clone();
        if !updated.playlist.eof_reached() {
            return stop();
        }
        let position = updated
            .playlist
            .position()
            .ok_or_else(|| "Android media playlist has no entry after EOF".to_string())?;
        let uri = updated.playlist.entries()[position].filename.clone();
        play_uri(&uri)?;
        *self.media = updated;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_ACTIVITY: AndroidActivityGeneration = AndroidActivityGeneration(1);
    const SECOND_ACTIVITY: AndroidActivityGeneration = AndroidActivityGeneration(2);

    fn media_with_tracks() -> AndroidMediaPlaylist {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///one.ogg");
        playlist.add_uri("file:///two.ogg");
        playlist.add_uri("file:///three.ogg");
        playlist.set_position(0);
        AndroidMediaPlaylist::new(
            playlist,
            vec!["One".to_string(), "Two".to_string(), "Three".to_string()],
        )
    }

    fn loaded_state() -> AndroidMediaPlaylistState {
        let mut state = AndroidMediaPlaylistState::default();
        assert!(state.initialize_authoritative(media_with_tracks()));
        state
    }

    #[test]
    fn foreground_service_controls_are_deferred_without_playlist_mutation() {
        let mut state = loaded_state();
        state.replace_activity(FIRST_ACTIVITY, true);

        assert_eq!(
            state.authority(),
            AndroidMediaPlaylistAuthority::Mirror(FIRST_ACTIVITY)
        );
        assert!(state.authoritative_mut().is_none());
        assert_eq!(state.media().unwrap().playlist.position(), Some(0));
    }

    #[test]
    fn foreground_and_background_control_race_follows_serialized_pause_order() {
        for pause_before_control in [false, true] {
            let mut state = loaded_state();
            state.replace_activity(FIRST_ACTIVITY, true);
            if pause_before_control {
                state.activity_paused_or_exited(FIRST_ACTIVITY);
            }

            let executed = if let Some(mut authoritative) = state.authoritative_mut() {
                authoritative
                    .change_track(TrackDirection::Next, |_| Ok(()), || Ok(()))
                    .unwrap();
                true
            } else {
                false
            };

            assert_eq!(executed, pause_before_control);
            assert_eq!(
                state.media().unwrap().playlist.position(),
                Some(usize::from(pause_before_control))
            );
        }
    }

    #[test]
    fn activity_replacement_rejects_stale_exit_and_projection() {
        let mut state = loaded_state();
        state.replace_activity(FIRST_ACTIVITY, true);
        state.replace_activity(SECOND_ACTIVITY, false);

        assert_eq!(
            state.authority(),
            AndroidMediaPlaylistAuthority::Authoritative
        );
        state.activity_paused_or_exited(FIRST_ACTIVITY);
        state.activity_resumed(SECOND_ACTIVITY);
        assert_eq!(
            state.authority(),
            AndroidMediaPlaylistAuthority::Mirror(SECOND_ACTIVITY)
        );

        let mut stale = media_with_tracks();
        stale.playlist.set_position(2);
        assert!(!state.sync_mirror(FIRST_ACTIVITY, stale));
        assert_eq!(state.media().unwrap().playlist.position(), Some(0));

        state.activity_paused_or_exited(FIRST_ACTIVITY);
        assert_eq!(
            state.authority(),
            AndroidMediaPlaylistAuthority::Mirror(SECOND_ACTIVITY)
        );
        state.activity_paused_or_exited(SECOND_ACTIVITY);
        assert_eq!(
            state.authority(),
            AndroidMediaPlaylistAuthority::Authoritative
        );
    }

    #[test]
    fn authoritative_eof_advances_once_and_commits_after_backend_success() {
        let mut state = loaded_state();
        let mut played = Vec::new();

        state
            .authoritative_mut()
            .unwrap()
            .advance_after_end_of_stream(
                |uri| {
                    played.push(uri.to_string());
                    Ok(())
                },
                || panic!("playlist should advance"),
            )
            .unwrap();

        assert_eq!(played, vec!["file:///two.ogg"]);
        assert_eq!(state.media().unwrap().playlist.position(), Some(1));
    }

    #[test]
    fn play_media_item_is_deferred_in_foreground_and_authoritative_in_background() {
        let mut state = loaded_state();
        state.replace_activity(FIRST_ACTIVITY, true);
        assert!(state.authoritative_mut().is_none());
        assert_eq!(state.media().unwrap().playlist.position(), Some(0));

        state.activity_paused_or_exited(FIRST_ACTIVITY);
        let mut played = Vec::new();
        state
            .authoritative_mut()
            .unwrap()
            .play_media_item(2, |uri| {
                played.push(uri.to_string());
                Ok(())
            })
            .unwrap();

        assert_eq!(played, vec!["file:///three.ogg"]);
        assert_eq!(state.media().unwrap().playlist.position(), Some(2));
    }

    #[test]
    fn authoritative_start_selects_first_entry_after_cold_process_restore() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///one.ogg");
        let mut state = AndroidMediaPlaylistState::default();
        state
            .initialize_authoritative(AndroidMediaPlaylist::new(playlist, vec!["One".to_string()]));
        let mut played = Vec::new();

        state
            .authoritative_mut()
            .unwrap()
            .start_current(|uri| {
                played.push(uri.to_string());
                Ok(())
            })
            .unwrap();

        assert_eq!(played, vec!["file:///one.ogg"]);
        assert_eq!(state.media().unwrap().playlist.position(), Some(0));
    }
}
