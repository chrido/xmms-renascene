//! Android snapshot persistence.
//!
//! The mutexes serialize Activity/service writes within one process. They are
//! not a cross-process lock: readers such as widgets rely on the session layer's
//! temporary-file plus atomic-rename protocol.

use std::io;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::app_state::AppState;
use crate::config::Config;
use crate::playback::backend::PlaybackBackend;
use crate::playback::model::PlayerState;
use crate::playback::rodio::RodioBackend;
use crate::session::{default_config_dir, fallback_state_paths, save_fallback_snapshot};

static STATE_IO: OnceLock<Mutex<()>> = OnceLock::new();
static LAST_POSITION_CHECKPOINT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

const POSITION_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(10);

pub fn persist_app_state(state: &AppState, playback_position_ms: Option<i64>) -> io::Result<()> {
    let _state_io = STATE_IO
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let (config_path, playlist_path) = fallback_state_paths(&default_config_dir());
    let snapshot = state.persistence_snapshot();
    let snapshot = match playback_position_ms {
        Some(position_ms) => snapshot.with_playback_position(position_ms),
        None => snapshot,
    };
    save_fallback_snapshot(&snapshot, &config_path, &playlist_path)
}

pub(crate) fn checkpoint_playback_position(
    backend: &RodioBackend,
    playlist_position: impl FnOnce() -> Option<usize>,
) {
    let now = Instant::now();
    let mut last_checkpoint = LAST_POSITION_CHECKPOINT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if backend.state() != PlayerState::Playing {
        *last_checkpoint = None;
        return;
    }
    let Some(last) = *last_checkpoint else {
        *last_checkpoint = Some(now);
        return;
    };
    if now.saturating_duration_since(last) < POSITION_CHECKPOINT_INTERVAL {
        return;
    }
    let Some(position_ms) = backend.position_ms().map(|position| position.max(0)) else {
        return;
    };

    let _state_io = STATE_IO
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let (config_path, _) = fallback_state_paths(&default_config_dir());
    let mut config = match Config::load_from_file(&config_path) {
        Ok(config) => config,
        Err(err) if err.kind() == io::ErrorKind::NotFound => Config::default(),
        Err(err) => {
            eprintln!("xmms-rs: failed to load Android position checkpoint: {err}");
            return;
        }
    };
    config.playback_position_ms = position_ms;
    config.playlist_position =
        playlist_position().map_or(-1, |position| position.min(i32::MAX as usize) as i32);
    match config.save_to_file(&config_path) {
        Ok(()) => *last_checkpoint = Some(now),
        Err(err) => eprintln!("xmms-rs: failed to save Android position checkpoint: {err}"),
    }
}
