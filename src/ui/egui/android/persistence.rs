//! Android snapshot persistence.
//!
//! The mutexes serialize Activity/service writes within one process. They are
//! not a cross-process lock: readers such as widgets rely on the session layer's
//! temporary-file plus atomic-rename protocol.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::app_state::AppState;
use crate::config::Config;
use crate::playback::backend::PlaybackBackend;
use crate::playback::model::PlayerState;
use crate::playback::rodio::RodioBackend;
use crate::playlist::Playlist;
use crate::session::{default_config_dir, fallback_state_paths, save_fallback_snapshot};

static STATE_IO: OnceLock<Mutex<()>> = OnceLock::new();
static LAST_POSITION_CHECKPOINT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
static PERSISTENCE_WRITER: OnceLock<Result<PersistenceWriter, String>> = OnceLock::new();

const POSITION_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(10);

pub fn persist_app_state(state: &AppState, playback_position_ms: Option<i64>) -> io::Result<()> {
    let snapshot = state.persistence_snapshot();
    let mut config = snapshot.config;
    if let Some(position_ms) = playback_position_ms {
        config.playback_position_ms = position_ms.max(0);
    }
    persistence_writer()?.send(PersistenceWrite::Snapshot {
        config,
        playlist: snapshot.playlist.clone(),
    })
}

pub fn persist_playback_position(state: &AppState, playback_position_ms: i64) -> io::Result<()> {
    let mut config = state.persistence_snapshot().config;
    config.playback_position_ms = playback_position_ms.max(0);
    persistence_writer()?.send(PersistenceWrite::Position(config))
}

pub fn flush_persistence_writer() -> io::Result<()> {
    persistence_writer()?.flush()
}

pub fn take_persistence_error() -> io::Result<()> {
    persistence_writer()?.take_error()
}

enum PersistenceWrite {
    Snapshot { config: Config, playlist: Playlist },
    Position(Config),
}

enum WriterCommand {
    Write(PersistenceWrite),
    Flush(Sender<Result<(), String>>),
}

struct PersistenceWriter {
    sender: Sender<WriterCommand>,
    results: Mutex<Receiver<Result<(), String>>>,
}

impl PersistenceWriter {
    fn start() -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let (result_sender, results) = mpsc::channel();
        thread::Builder::new()
            .name("xmms-android-persistence".to_string())
            .spawn(move || persistence_writer_loop(receiver, result_sender))
            .map_err(|error| format!("failed to start Android persistence writer: {error}"))?;
        Ok(Self {
            sender,
            results: Mutex::new(results),
        })
    }

    fn send(&self, write: PersistenceWrite) -> io::Result<()> {
        self.take_error()?;
        self.sender
            .send(WriterCommand::Write(write))
            .map_err(|_| io::Error::other("Android persistence writer stopped"))
    }

    fn flush(&self) -> io::Result<()> {
        self.take_error()?;
        let (sender, receiver) = mpsc::channel();
        self.sender
            .send(WriterCommand::Flush(sender))
            .map_err(|_| io::Error::other("Android persistence writer stopped"))?;
        receiver
            .recv()
            .map_err(|_| io::Error::other("Android persistence writer stopped"))?
            .map_err(io::Error::other)
    }

    fn take_error(&self) -> io::Result<()> {
        let results = self
            .results
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while let Ok(result) = results.try_recv() {
            result.map_err(io::Error::other)?;
        }
        Ok(())
    }
}

fn persistence_writer() -> io::Result<&'static PersistenceWriter> {
    PERSISTENCE_WRITER
        .get_or_init(PersistenceWriter::start)
        .as_ref()
        .map_err(|error| io::Error::other(error.clone()))
}

fn persistence_writer_loop(receiver: Receiver<WriterCommand>, results: Sender<Result<(), String>>) {
    while let Ok(command) = receiver.recv() {
        let mut snapshot = None;
        let mut position = None;
        let mut flushes = Vec::new();
        collect_writer_command(command, &mut snapshot, &mut position, &mut flushes);
        while let Ok(command) = receiver.try_recv() {
            collect_writer_command(command, &mut snapshot, &mut position, &mut flushes);
        }

        let result =
            write_pending_persistence(snapshot, position).map_err(|error| error.to_string());
        if let Err(error) = &result {
            let _ = results.send(Err(error.clone()));
        }
        for flush in flushes {
            let _ = flush.send(result.clone());
        }
    }
}

fn collect_writer_command(
    command: WriterCommand,
    snapshot: &mut Option<(Config, Playlist)>,
    position: &mut Option<Config>,
    flushes: &mut Vec<Sender<Result<(), String>>>,
) {
    match command {
        WriterCommand::Write(PersistenceWrite::Snapshot { config, playlist }) => {
            *snapshot = Some((config, playlist));
            *position = None;
        }
        WriterCommand::Write(PersistenceWrite::Position(config)) => {
            *position = Some(config);
        }
        WriterCommand::Flush(sender) => flushes.push(sender),
    }
}

fn write_pending_persistence(
    snapshot: Option<(Config, Playlist)>,
    position: Option<Config>,
) -> io::Result<()> {
    let _state_io = STATE_IO
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let (config_path, playlist_path) = fallback_state_paths(&default_config_dir());
    if let Some((config, playlist)) = snapshot {
        let snapshot = crate::app_state::PersistenceSnapshot {
            config,
            playlist: &playlist,
        };
        save_fallback_snapshot(&snapshot, &config_path, &playlist_path)?;
    }
    if let Some(config) = position {
        config.save_to_file(&config_path)?;
    }
    Ok(())
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

pub(crate) fn persist_playback_position_now(
    backend: &RodioBackend,
    playlist_position: Option<usize>,
) -> io::Result<()> {
    let Some(position_ms) = backend.position_ms().map(|position| position.max(0)) else {
        return Ok(());
    };
    let mut config = {
        let _state_io = STATE_IO
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (config_path, _) = fallback_state_paths(&default_config_dir());
        match Config::load_from_file(&config_path) {
            Ok(config) => config,
            Err(err) if err.kind() == io::ErrorKind::NotFound => Config::default(),
            Err(err) => return Err(err),
        }
    };
    config.playback_position_ms = position_ms;
    config.playlist_position =
        playlist_position.map_or(-1, |position| position.min(i32::MAX as usize) as i32);
    persistence_writer()?.send(PersistenceWrite::Position(config))?;
    persistence_writer()?.flush()?;
    *LAST_POSITION_CHECKPOINT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(Instant::now());
    Ok(())
}
