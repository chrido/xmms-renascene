//! Host regression tests for the service -> inbox -> foreground player handoff.
use super::super::android_events::{AndroidEventInbox, AndroidPlatformEvent};
use super::super::android_media::{
    AndroidActivityGeneration, AndroidMediaPlaylist, AndroidMediaPlaylistState,
};
use super::*;
use crate::app::playback_transition::PlaybackTransition;
use crate::app::store::ExternalPlaybackSnapshot;
use crate::app::view_model::main_player_view_model;
use crate::playback::model::{EqualizerBackendState, StreamInfo};
use crate::playlist::TrackDirection;

struct NoTransportBackend;

impl PlaybackBackend for NoTransportBackend {
    fn play_uri(&self, _: &str) -> Result<(), String> {
        panic!("snapshot restarted audio")
    }
    fn pause(&self) -> Result<(), String> {
        panic!("snapshot paused audio")
    }
    fn unpause(&self) -> Result<(), String> {
        panic!("snapshot resumed audio")
    }
    fn stop(&self) -> Result<(), String> {
        panic!("snapshot stopped audio")
    }
    fn seek(&self, _: i64) -> Result<(), String> {
        panic!("snapshot sought audio")
    }
    fn set_volume(&self, _: i32) -> Result<(), String> {
        panic!("snapshot changed volume")
    }
    fn set_balance(&self, _: i32) -> Result<(), String> {
        panic!("snapshot changed balance")
    }
    fn set_equalizer(&self, _: EqualizerBackendState) -> Result<(), String> {
        panic!("snapshot changed equalizer")
    }
}

fn frontend() -> EguiFrontendState {
    let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
    let mut state = AppState::default();
    for uri in ["file:///one.ogg", "file:///two.ogg", "file:///three.ogg"] {
        state.playlist.add_uri(uri);
    }
    state.playlist.set_position(0);
    state.player.mark_playing();
    app.core = FrontendRuntime::new(state, Some(Box::new(NoTransportBackend)));
    app
}

fn snapshot(
    playlist: Playlist,
    player_state: PlayerState,
    position_ms: i64,
) -> ExternalPlaybackSnapshot {
    ExternalPlaybackSnapshot {
        playlist,
        player_state,
        position_ms,
        duration_ms: Some(180_000),
        stream_info: StreamInfo {
            bitrate: Some(192),
            frequency: Some(44_100),
            channels: Some(2),
        },
    }
}

#[test]
fn android_background_next_reconciles_title_after_resume_or_recreation() {
    for (recreated, missed_control_event) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let mut app = frontend();
        let old_title = main_player_view_model(app.core.state()).title;
        let mut service = AndroidMediaPlaylistState::default();
        service.initialize_authoritative(AndroidMediaPlaylist::new(
            app.core.state().playlist.clone(),
            Vec::new(),
        ));
        let first = AndroidActivityGeneration(1);
        service.replace_activity(first, true);
        service.activity_paused_or_exited(first);
        let mut played = Vec::new();
        service
            .authoritative_mut()
            .unwrap()
            .change_track(
                TrackDirection::Next,
                |uri| {
                    played.push(uri.to_string());
                    Ok(())
                },
                || panic!("Next should advance"),
            )
            .unwrap();
        assert_eq!(played, ["file:///two.ogg"]);

        let current = snapshot(
            service.media().unwrap().playlist.clone(),
            PlayerState::Playing,
            3_400,
        );
        let mut inbox = AndroidEventInbox::default();
        if !missed_control_event {
            inbox.push(AndroidPlatformEvent::PlaybackSnapshot(current.clone()));
        }
        if recreated {
            app = frontend(); // A fresh UI still has the old persisted selection.
            inbox.replace_activity();
            service.replace_activity(AndroidActivityGeneration(2), true);
        } else {
            service.activity_resumed(first);
        }
        // The resume snapshot must repair the UI even if it has missed earlier
        // events, and must be harmless if the control snapshot was retained.
        inbox.push(AndroidPlatformEvent::PlaybackSnapshot(current.clone()));
        let (events, more) = inbox.drain_frame();
        assert!(!more);
        assert_eq!(events.len(), if missed_control_event { 1 } else { 2 });
        for event in events {
            let AndroidPlatformEvent::PlaybackSnapshot(current) = event else {
                panic!("unexpected event")
            };
            app.handle_android_playback_snapshot(current);
        }
        let view = main_player_view_model(app.core.state());
        assert_ne!(view.title, old_title);
        assert!(view.title.contains("two"));
        assert_eq!(app.core.state().playlist, current.playlist);
        assert_eq!(app.core.state().config.playback_position_ms, 3_400);
        assert_eq!(app.core.state().player.duration_ms(), Some(180_000));
        assert_eq!(app.core.state().player.channels(), 2);
        assert!(app.runtime.dirty_targets().contains(&RenderTarget::All));

        // Render the real player, not just the media-session projection.
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default()
                .show(ui, |ui| main_player::show_main_player(ui, &mut app));
        });
        assert_eq!(
            app.render_cache.main.as_ref().unwrap().state.title,
            view.title
        );
    }
}

#[test]
fn android_snapshot_preserves_queue_and_shuffle_selection() {
    let mut app = frontend();
    let mut playlist = app.core.state().playlist.clone();
    playlist.set_shuffle(true);
    playlist.set_repeat(true);
    playlist.enqueue(2);
    playlist.enqueue(1);
    let mut service = AndroidMediaPlaylistState::default();
    service.initialize_authoritative(AndroidMediaPlaylist::new(playlist, Vec::new()));
    let mut inbox = AndroidEventInbox::default();
    for _ in 0..2 {
        service
            .authoritative_mut()
            .unwrap()
            .change_track(TrackDirection::Next, |_| Ok(()), || Ok(()))
            .unwrap();
        inbox.push(AndroidPlatformEvent::PlaybackSnapshot(snapshot(
            service.media().unwrap().playlist.clone(),
            PlayerState::Playing,
            0,
        )));
    }
    // Local input must not discard already-executed service state.
    inbox.remove_unexecuted_media_controls();
    for event in inbox.drain_frame().0 {
        let AndroidPlatformEvent::PlaybackSnapshot(snapshot) = event else {
            panic!("unexpected event")
        };
        app.handle_android_playback_snapshot(snapshot);
    }
    assert!(app.core.state().playlist.shuffle());
    assert_eq!(app.core.state().playlist.queued_indices(), [2, 1]);
    assert_eq!(app.core.state().playlist, service.media().unwrap().playlist);
}

#[test]
fn android_resume_snapshot_cancels_stale_seek_and_preserves_paused_state() {
    let mut app = frontend();
    app.core.request_backend_seek(42_000);
    let mut playlist = app.core.state().playlist.clone();
    playlist.set_position(2);
    app.handle_android_playback_snapshot(snapshot(playlist, PlayerState::Paused, 5_600));
    assert_eq!(app.core.transition(), PlaybackTransition::Idle);
    app.handle_playback_events([PlaybackEvent::AsyncDone]);
    assert_eq!(app.core.state().player.state(), PlayerState::Paused);
    assert_eq!(app.core.state().config.playback_position_ms, 5_600);
    assert!(main_player_view_model(app.core.state())
        .title
        .contains("three"));
}

#[test]
fn android_snapshot_expires_unprepared_local_playback_start() {
    let mut app = frontend();
    let mut update = app.core.handle(
        RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
        EffectExecution::AlreadyExecuted,
    );
    let stale_start = update.pending_playback_starts.pop().unwrap();
    let mut playlist = app.core.state().playlist.clone();
    playlist.set_position(1);
    app.handle_android_playback_snapshot(snapshot(playlist, PlayerState::Playing, 900));
    let update = app.core.handle(
        RuntimeEvent::PlaybackStartPrepared(stale_start),
        EffectExecution::LOCAL,
    );
    assert!(update.playback_effects.is_empty());
    assert_eq!(app.core.state().playlist.position(), Some(1));
}

#[test]
fn android_eof_snapshot_cancels_local_countdown_and_stop_clears_metadata() {
    let mut app = frontend();
    let mut config = app.core.state().config.clone();
    config.pause_between_songs = true;
    config.pause_between_songs_time = 2;
    app.process_store_event(RuntimeEvent::Preferences(config), EffectExecution::LOCAL);
    app.process_store_event(RuntimeEvent::PlaylistEof, EffectExecution::LOCAL);
    assert!(app.core.transition().wait_remaining().is_some());
    let mut playlist = app.core.state().playlist.clone();
    playlist.set_position(1);
    app.handle_android_playback_snapshot(snapshot(playlist.clone(), PlayerState::Playing, 900));
    app.process_store_event(RuntimeEvent::TransitionTick(3_000), EffectExecution::LOCAL);
    assert_eq!(app.core.state().playlist.position(), Some(1));
    assert_eq!(app.core.state().config.playback_position_ms, 900);
    app.handle_android_playback_snapshot(snapshot(playlist, PlayerState::Stopped, 0));
    assert_eq!(app.core.state().player.state(), PlayerState::Stopped);
    assert_eq!(app.core.state().player.duration_ms(), None);
    assert_eq!(app.core.state().player.bitrate(), 0);
    assert_eq!(app.core.state().config.playback_position_ms, 0);
}
