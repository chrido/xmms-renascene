//! Android artwork JNI implementation. Snapshot/cancellation calls are memory
//! only. Cold resolution + rasterization are synchronous WORKER-only calls; no
//! service/Activity GlobalRef is retained and no playback lock covers disk/render.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::artwork::settings::{process_artwork_settings, resolve_artwork_settings};
use crate::artwork::{
    normalize_track_identity, stable_track_seed, ArtworkPolicy, ArtworkSpec,
    ARTWORK_RENDERER_VERSION, NOTIFICATION_ARTWORK_SIZE,
};
use crate::render::artwork::render_artwork_cancellable;
use crate::session::fallback_state_paths;

use super::{activity, media_session};

/// Foreground hook: use the already-loaded palette, then notify only an existing
/// playback service through an application-context broadcast. No skin I/O here.
pub fn publish_foreground(
    selector: Option<String>,
    policy: ArtworkPolicy,
    palette: [[u8; 3]; 24],
) -> Result<(), String> {
    let update = process_artwork_settings()
        .publish_foreground(selector, policy, palette)
        .map_err(|err| err.to_string())?;
    if !update.changed {
        return Ok(());
    }
    let (vm, activity) = activity::reference()?;
    let mut env = vm.attach_current_thread().map_err(|err| err.to_string())?;
    env.call_method(activity.as_obj(), "refreshPlaybackArtwork", "()V", &[])
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[derive(Default)]
struct Cancellation {
    epoch: u64,
    generation: u64,
    active: bool,
}

fn cancellation() -> &'static Mutex<Cancellation> {
    static STATE: OnceLock<Mutex<Cancellation>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(Cancellation::default()))
}

pub(crate) fn select(epoch: u64, generation: u64) {
    let mut current = cancellation()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    // A destroyed old service must not cancel the replacement service's worker.
    if epoch > current.epoch || (epoch == current.epoch && generation >= current.generation) {
        *current = Cancellation {
            epoch,
            generation,
            active: true,
        };
    }
}

pub(crate) fn cancel(epoch: u64) {
    let mut current = cancellation()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if current.epoch == epoch {
        current.active = false;
    }
}

fn cancelled(epoch: u64, generation: u64) -> bool {
    let current = cancellation()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    !current.active || current.epoch != epoch || current.generation != generation
}

pub(crate) fn resolve_on_worker(
    files_dir: &Path,
    epoch: u64,
    generation: u64,
) -> Result<bool, String> {
    if cancelled(epoch, generation) {
        return Err("cancelled".into());
    }
    let store = process_artwork_settings();
    let Some(token) = store.begin_cold_resolution() else {
        return Ok(false);
    };
    let (config, _) = fallback_state_paths(&files_dir.join("config"));
    let resolved = resolve_artwork_settings(&config);
    // Serialize cancellation against the memory-only commit, not disk work.
    // Lock order is cancellation -> settings; no path takes the inverse order,
    // and no JNI/backend call or diagnostic emission happens under these locks.
    let commit = {
        let current = cancellation()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !current.active || current.epoch != epoch || current.generation != generation {
            return Err("cancelled".into());
        }
        store
            .commit_cold(&token, resolved)
            .map_err(|err| err.to_string())?
    };
    // Java emits a one-shot Log.w for the accepted fallback: arbitrary worker
    // stderr is not reliably routed to logcat by NativeActivity's Rust thread.
    Ok(commit.is_some_and(|commit| commit.diagnostic.is_some()))
}

/// No backend/ordering lock is acquired here: MediaPlaylist is copied under its
/// own short lock and the settings snapshot separately cloned. Consumers compare
/// the full identity again at completion, so interleaving updates invalidate it.
pub(crate) fn snapshot(files_dir: &Path) -> Option<String> {
    snapshot_item(files_dir, None)
}

pub(crate) fn snapshot_item(files_dir: &Path, index: Option<usize>) -> Option<String> {
    let settings = process_artwork_settings().snapshot();
    let media = match index {
        Some(index) => media_session::artwork_media_item_identity(index),
        None => media_session::artwork_media_identity(),
    };
    let (track, content_revision, _activity_generation, _position) = media
        .map(|(uri, revision, activity, position)| {
            (
                normalize_track_identity(&uri, Some(files_dir)),
                revision,
                activity,
                position,
            )
        })
        .unwrap_or_else(|| (String::new(), 0, None, -1));
    if track.len() > 8192 {
        return None;
    }
    // Title/duration corrections, reordering duplicate URIs and Home/resume
    // ownership transitions do not change the picture. Projection tickets
    // separately guard metadata/selection revisions at publication time.
    // Cover/audio-derived outputs refresh on library reload/replacement, even
    // when the URI is unchanged. Never stat/read audio files on the main thread.
    // External edits are observed on the next library reload, not by polling.
    let source_revision = match settings.policy {
        ArtworkPolicy::Auto | ArtworkPolicy::AudioInspired => content_revision,
        _ => 0,
    };
    let media_identity =
        serde_json::json!({"track":track,"sourceRevision":source_revision}).to_string();
    let identity =
        serde_json::json!({"media":media_identity, "settings":settings.revision}).to_string();
    let key = serde_json::json!({"track":track, "palette":settings.palette, "size":NOTIFICATION_ARTWORK_SIZE, "version":ARTWORK_RENDERER_VERSION, "policy":settings.policy.as_config_value(), "audioSummaryVersion":crate::artwork::audio_summary::AUDIO_SUMMARY_VERSION,"sourceRevision":source_revision}).to_string();
    let value = serde_json::json!({
        "resolved":settings.palette.is_some(), "enabled":settings.policy != ArtworkPolicy::Off,
        "hasTrack":!track.is_empty(), "identity":identity, "mediaIdentity":media_identity, "key":key,
    }).to_string();
    (value.len() <= 65536).then_some(value)
}

pub(crate) fn resolve_browse_on_worker(files_dir: &Path) -> Result<(), String> {
    let store = process_artwork_settings();
    let Some(token) = store.begin_cold_resolution() else {
        return Ok(());
    };
    let (config, _) = fallback_state_paths(&files_dir.join("config"));
    let resolved = resolve_artwork_settings(&config);
    store
        .commit_cold(&token, resolved)
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub(crate) fn render_on_worker(
    files_dir: &Path,
    frozen: &str,
    epoch: u64,
    generation: u64,
    output_size: u32,
) -> Result<Vec<i32>, String> {
    render_item(files_dir, frozen, epoch, generation, output_size, None)
}

pub(crate) fn render_item(
    files_dir: &Path,
    frozen: &str,
    epoch: u64,
    generation: u64,
    output_size: u32,
    index: Option<usize>,
) -> Result<Vec<i32>, String> {
    let is_cancelled = || index.is_none() && cancelled(epoch, generation);
    if frozen.len() > 65536 || is_cancelled() {
        return Err("cancelled".into());
    }
    // Private JNI input is still validated: a fabricated key cannot borrow a
    // valid identity to render a different track/palette.
    if snapshot_item(files_dir, index).as_deref() != Some(frozen) {
        return Err("stale snapshot".into());
    }
    let value: serde_json::Value = serde_json::from_str(frozen).map_err(|_| "invalid snapshot")?;
    if value["resolved"] != true || value["enabled"] != true || value["hasTrack"] != true {
        return Err("artwork disabled".into());
    }
    let key: serde_json::Value = serde_json::from_str(value["key"].as_str().ok_or("missing key")?)
        .map_err(|_| "invalid key")?;
    let mut palette = [[0; 3]; 24];
    let colors = key["palette"]
        .as_array()
        .filter(|a| a.len() == 24)
        .ok_or("invalid palette")?;
    for (target, color) in palette.iter_mut().zip(colors) {
        let channels = color
            .as_array()
            .filter(|a| a.len() == 3)
            .ok_or("invalid color")?;
        for (out, channel) in target.iter_mut().zip(channels) {
            *out = u8::try_from(channel.as_u64().ok_or("invalid channel")?)
                .map_err(|_| "invalid channel")?;
        }
    }
    let track = key["track"].as_str().ok_or("missing track")?;
    let size =
        u32::try_from(key["size"].as_u64().ok_or("missing size")?).map_err(|_| "invalid size")?;
    if size != NOTIFICATION_ARTWORK_SIZE || ![320, 512].contains(&output_size) {
        return Err("unsupported bitmap size".into());
    }
    let size = output_size;
    let version = u32::try_from(key["version"].as_u64().ok_or("missing version")?)
        .map_err(|_| "invalid version")?;
    let mut spec = ArtworkSpec {
        renderer_version: version,
        stable_track_seed: stable_track_seed(track),
        palette,
        width: size,
        height: size,
    };
    if key["policy"] == "auto" && !is_cancelled() {
        // Auto reads covers only from normalized local files, never network/SAF
        // URIs. Extraction is bounded; corrupt/unsupported covers use Generated.
        if track.starts_with('/') {
            if let Ok(Some(pixels)) =
                crate::artwork::cover::load_embedded_cover(Path::new(track), size)
            {
                if is_cancelled() || snapshot_item(files_dir, index).as_deref() != Some(frozen) {
                    return Err("stale cover completion".into());
                }
                return Ok(pixels);
            }
        }
    }
    if key["policy"] == "audio-inspired" && track.starts_with('/') {
        if let Ok(Some(summary)) = crate::artwork::audio_summary::summarize_wav(Path::new(track)) {
            spec.stable_track_seed = summary.geometry_seed(spec.stable_track_seed);
        }
    }
    let image = render_artwork_cancellable(&spec, is_cancelled).map_err(|err| err.to_string())?;
    if is_cancelled() || snapshot_item(files_dir, index).as_deref() != Some(frozen) {
        return Err("stale completion".into());
    }
    Ok(image
        .into_argb()
        .into_iter()
        .map(|word| word as i32)
        .collect())
}
