//! Background local-playlist duration discovery, independent of frontend event loops.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use crate::playback::backend::AudioMetadataProbe;
#[cfg(not(feature = "rodio-backend"))]
use crate::playlist::file_uri_to_path;
use crate::playlist::{DurationIndexItem, DurationIndexResult, Playlist};

const BATCH_SIZE: usize = 16;

pub struct DurationIndexer {
    sender: Sender<Vec<DurationIndexResult>>,
    receiver: Receiver<Vec<DurationIndexResult>>,
    on_batch: Arc<dyn Fn() + Send + Sync>,
    batch_size: usize,
}

impl DurationIndexer {
    /// `on_batch` wakes the owning event loop; it must not inspect UI state on the worker.
    pub fn new(on_batch: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_batch_size(BATCH_SIZE, on_batch)
    }

    /// Use smaller batches when the owner needs each result promptly.
    pub fn with_batch_size(batch_size: usize, on_batch: impl Fn() + Send + Sync + 'static) -> Self {
        assert!(batch_size > 0, "duration index batch size must be positive");
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            on_batch: Arc::new(on_batch),
            batch_size,
        }
    }

    /// Start a worker for currently missing playlist entries.
    /// The playlist is only read here; the owner applies batches after draining.
    pub fn schedule(&self, playlist: &Playlist) {
        #[cfg(feature = "rodio-backend")]
        self.schedule_with_probe(playlist, crate::playback::rodio::RodioMetadataProbe);

        #[cfg(all(not(feature = "rodio-backend"), feature = "gstreamer-backend"))]
        self.schedule_with_factory(playlist, || {
            gstreamer::init().map_err(|err| {
                format!("failed to initialize GStreamer for playlist durations: {err}")
            })?;
            let discoverer = gstreamer_pbutils::Discoverer::new(
                gstreamer::ClockTime::from_seconds(5),
            )
            .map_err(|err| format!("failed to create playlist duration discoverer: {err}"))?;
            Ok(move |item: &DurationIndexItem| {
                let info = match discoverer.discover_uri(&item.uri) {
                    Ok(info) => info,
                    Err(err) => {
                        eprintln!(
                            "xmms-rs: failed to discover playlist item {}: {err}",
                            item.uri
                        );
                        return Ok(None);
                    }
                };
                Ok(Some(DurationIndexResult {
                    index: item.index,
                    uri: item.uri.clone(),
                    length_ms: info
                        .duration()
                        .map(|duration| duration.mseconds() as i64)
                        .unwrap_or(-1),
                    title: None,
                }))
            })
        });

        #[cfg(not(any(feature = "rodio-backend", feature = "gstreamer-backend")))]
        let _ = playlist;
    }

    /// The probe is moved to a worker. Constructing it in the worker is supported
    /// via the private factory, since GStreamer discoverers are thread-local.
    pub fn schedule_with_probe<P>(&self, playlist: &Playlist, probe: P)
    where
        P: AudioMetadataProbe + Send + 'static,
    {
        self.schedule_with_factory(playlist, || {
            Ok(move |item: &DurationIndexItem| probe.probe(item))
        });
    }

    fn schedule_with_factory<F, P>(&self, playlist: &Playlist, make_probe: F)
    where
        F: FnOnce() -> Result<P, String> + Send + 'static,
        P: FnMut(&DurationIndexItem) -> Result<Option<DurationIndexResult>, String>,
    {
        let items: Vec<_> = playlist
            .missing_duration_items()
            .into_iter()
            .filter_map(|item| local_path(&item.uri).map(|path| (item, path)))
            .collect();
        if items.is_empty() {
            return;
        }
        let sender = self.sender.clone();
        let notification = Arc::clone(&self.on_batch);
        let batch_size = self.batch_size;
        thread::spawn(move || {
            let mut probe = match make_probe() {
                Ok(probe) => probe,
                Err(err) => {
                    eprintln!("xmms-rs: {err}");
                    return;
                }
            };
            let mut results = Vec::with_capacity(batch_size);
            for (item, path) in items {
                // Existence and metadata checks can hit network mounts; keep
                // all filesystem access off the frontend event loop.
                if !path.exists() {
                    continue;
                }
                match probe(&item) {
                    Ok(Some(result)) => {
                        results.push(result);
                        if results.len() >= batch_size
                            && !send_batch(&sender, notification.as_ref(), &mut results, batch_size)
                        {
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        eprintln!("xmms-rs: failed to probe playlist item {}: {err}", item.uri)
                    }
                }
            }
            send_batch(&sender, notification.as_ref(), &mut results, batch_size);
        });
    }

    /// Nonblocking FIFO drain of completed value batches.
    pub fn drain(&self) -> Vec<Vec<DurationIndexResult>> {
        self.receiver.try_iter().collect()
    }

    /// Enqueue an externally supplied result event through the same drain path.
    pub fn enqueue_batch(&self, batch: Vec<DurationIndexResult>) {
        self.sender
            .send(batch)
            .expect("duration index receiver dropped");
    }
}

#[cfg(feature = "rodio-backend")]
fn local_path(uri: &str) -> Option<std::path::PathBuf> {
    crate::playback::rodio::resolve_local_audio_source(uri)
        .ok()
        .map(|source| source.path)
}

#[cfg(not(feature = "rodio-backend"))]
fn local_path(uri: &str) -> Option<std::path::PathBuf> {
    file_uri_to_path(uri)
}

fn send_batch(
    sender: &Sender<Vec<DurationIndexResult>>,
    on_batch: &dyn Fn(),
    results: &mut Vec<DurationIndexResult>,
    batch_size: usize,
) -> bool {
    if results.is_empty() {
        return true;
    }
    let batch = std::mem::replace(results, Vec::with_capacity(batch_size));
    if sender.send(batch).is_err() {
        return false;
    }
    on_batch();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::store::AppStore;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    struct LocalFiles(PathBuf);

    impl LocalFiles {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "xmms-duration-indexer-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }

        fn add(&self, playlist: &mut Playlist, name: &str) {
            let path = self.0.join(name);
            fs::write(&path, b"test").unwrap();
            playlist.add_path(path);
        }
    }

    impl Drop for LocalFiles {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    struct MockProbe(Sender<usize>);

    impl AudioMetadataProbe for MockProbe {
        fn probe(&self, item: &DurationIndexItem) -> Result<Option<DurationIndexResult>, String> {
            self.0.send(item.index).unwrap();
            Ok(Some(DurationIndexResult {
                index: item.index,
                uri: item.uri.clone(),
                length_ms: (item.index as i64 + 1) * 1000,
                title: None,
            }))
        }
    }

    #[test]
    fn skips_known_and_nonlocal_entries_before_probing() {
        let files = LocalFiles::new();
        let mut playlist = Playlist::new();
        files.add(&mut playlist, "missing.ogg");
        files.add(&mut playlist, "known.ogg");
        playlist.entries_mut()[1].length_ms = 42_000;
        playlist.add_uri("https://example.org/remote.ogg");
        playlist.add_uri("file:///not-a-real-duration-indexer-file.ogg");
        let (calls, received) = mpsc::channel();
        let (wakeup, notified) = mpsc::channel();
        let indexer = DurationIndexer::new(move || {
            wakeup.send(()).unwrap();
        });
        indexer.schedule_with_probe(&playlist, MockProbe(calls));
        notified.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(received.try_iter().collect::<Vec<_>>(), vec![0]);
        assert_eq!(indexer.drain()[0][0].index, 0);
        assert!(indexer.drain().is_empty());
    }

    #[test]
    fn batches_results_and_notifies_once_per_batch() {
        let files = LocalFiles::new();
        let mut playlist = Playlist::new();
        for index in 0..(BATCH_SIZE + 2) {
            files.add(&mut playlist, &format!("{index}.ogg"));
        }
        let (calls, received) = mpsc::channel();
        let (wakeup, notified) = mpsc::channel();
        let indexer = DurationIndexer::new(move || {
            wakeup.send(()).unwrap();
        });
        indexer.schedule_with_probe(&playlist, MockProbe(calls));
        notified.recv_timeout(Duration::from_secs(5)).unwrap();
        notified.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(notified.try_recv().is_err());
        assert_eq!(received.try_iter().count(), BATCH_SIZE + 2);
        let batches = indexer.drain();
        assert_eq!(
            batches.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![BATCH_SIZE, 2]
        );
        assert_eq!(batches[0][0].index, 0);
        assert_eq!(batches[1][1].index, BATCH_SIZE + 1);
        assert!(indexer.drain().is_empty());
    }

    struct SlowSecondProbe {
        entered: Sender<usize>,
        release: Receiver<()>,
    }

    impl AudioMetadataProbe for SlowSecondProbe {
        fn probe(&self, item: &DurationIndexItem) -> Result<Option<DurationIndexResult>, String> {
            self.entered.send(item.index).unwrap();
            if item.index == 1 {
                self.release.recv().unwrap();
            }
            Ok(Some(DurationIndexResult {
                index: item.index,
                uri: item.uri.clone(),
                length_ms: 1_000,
                title: None,
            }))
        }
    }

    #[test]
    fn single_result_batches_deliver_progress_before_next_probe_finishes() {
        let files = LocalFiles::new();
        let mut playlist = Playlist::new();
        files.add(&mut playlist, "first.ogg");
        files.add(&mut playlist, "slow.ogg");
        let (entered, calls) = mpsc::channel();
        let (release, proceed) = mpsc::channel();
        let (wakeup, notified) = mpsc::channel();
        let indexer = DurationIndexer::with_batch_size(1, move || {
            wakeup.send(()).unwrap();
        });
        indexer.schedule_with_probe(
            &playlist,
            SlowSecondProbe {
                entered,
                release: proceed,
            },
        );
        assert_eq!(calls.recv_timeout(Duration::from_secs(5)).unwrap(), 0);
        notified.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(calls.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        assert_eq!(indexer.drain()[0][0].index, 0);
        release.send(()).unwrap();
        notified.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(indexer.drain()[0][0].index, 1);
    }

    #[test]
    fn stale_worker_values_are_guarded_on_apply_and_reapply_is_idempotent() {
        let files = LocalFiles::new();
        let mut playlist = Playlist::new();
        files.add(&mut playlist, "original.ogg");
        let (calls, _received) = mpsc::channel();
        let (wakeup, notified) = mpsc::channel();
        let indexer = DurationIndexer::new(move || {
            wakeup.send(()).unwrap();
        });
        indexer.schedule_with_probe(&playlist, MockProbe(calls));
        playlist.clear();
        files.add(&mut playlist, "replacement.ogg");
        notified.recv_timeout(Duration::from_secs(5)).unwrap();
        let result = indexer.drain().pop().unwrap().pop().unwrap();
        let mut store = AppStore::default();
        store.state_mut().playlist = playlist;
        let revision = store.revision();
        assert!(store
            .apply_duration_index_results(vec![result.clone()])
            .changes
            .is_empty());
        assert_eq!(store.revision(), revision);
        assert_eq!(store.state().playlist.entries()[0].length_ms, -1);

        files.add(&mut store.state_mut().playlist, "original.ogg");
        assert!(!store
            .apply_duration_index_results(vec![result.clone()])
            .changes
            .is_empty());
        let revision = store.revision();
        assert!(store
            .apply_duration_index_results(vec![result])
            .changes
            .is_empty());
        assert_eq!(store.revision(), revision);
    }
}
