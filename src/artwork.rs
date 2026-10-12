//! Frontend-neutral contracts for static, skin-colored media artwork.
//!
//! The contracts/state machine below have no filesystem, JNI, UI, or backend
//! access. The separate `settings` module exposes blocking worker-only cold
//! resolution and memory-only snapshot publication. Android owns executor glue.

pub mod audio_summary;
pub mod cover;
pub mod png;
pub mod projection;
pub mod settings;

use std::fmt;
use std::path::Path;

pub type ArtworkPalette = [[u8; 3]; 24];
pub const ARTWORK_RENDERER_VERSION: u32 = 1;
pub const NOTIFICATION_ARTWORK_SIZE: u32 = 320;
pub const URI_ARTWORK_SIZE: u32 = 512;

/// Persisted independently of the player's visualization mode. Unknown/old
/// configuration values fail closed to Off; absent settings default to Auto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArtworkPolicy {
    Off,
    Generated,
    #[default]
    Auto,
    AudioInspired,
}

impl ArtworkPolicy {
    pub fn from_config_value(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("audio-inspired") {
            Self::AudioInspired
        } else if value.trim().eq_ignore_ascii_case("auto") {
            Self::Auto
        } else if value.trim().eq_ignore_ascii_case("generated") {
            Self::Generated
        } else {
            Self::Off
        }
    }

    pub fn as_config_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Generated => "generated",
            Self::Auto => "auto",
            Self::AudioInspired => "audio-inspired",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkSpec {
    pub renderer_version: u32,
    pub stable_track_seed: u64,
    pub palette: ArtworkPalette,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkError {
    UnsupportedRenderer,
    UnsupportedDimensions,
    Cancelled,
    GenerationExhausted,
    Stopped,
    IdentityMismatch,
}

impl fmt::Display for ArtworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedRenderer => "unsupported artwork renderer version",
            Self::UnsupportedDimensions => "artwork must be a supported 320px or 512px square",
            Self::Cancelled => "artwork request cancelled",
            Self::GenerationExhausted => "artwork request generation exhausted",
            Self::Stopped => "artwork coordinator stopped",
            Self::IdentityMismatch => "artwork key and request track identity differ",
        })
    }
}

impl std::error::Error for ArtworkError {}

impl ArtworkSpec {
    /// Validate before allocating any image buffer.
    pub fn pixel_count(&self) -> Result<usize, ArtworkError> {
        if self.renderer_version != ARTWORK_RENDERER_VERSION {
            return Err(ArtworkError::UnsupportedRenderer);
        }
        if self.width != self.height
            || ![NOTIFICATION_ARTWORK_SIZE, URI_ARTWORK_SIZE].contains(&self.width)
        {
            return Err(ArtworkError::UnsupportedDimensions);
        }
        self.width
            .checked_mul(self.height)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(ArtworkError::UnsupportedDimensions)
    }
}

/// Full render key, not a digest: hash collisions cannot alias cached pictures.
/// Track revisions are deliberately absent: playlist reordering is not a new
/// picture, whereas request revisions still guard publication of stale results.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtworkKey {
    pub normalized_track_identity: String,
    pub palette: ArtworkPalette,
    pub size: u32,
    pub renderer_version: u32,
}

impl ArtworkKey {
    pub fn spec(&self) -> ArtworkSpec {
        ArtworkSpec {
            renderer_version: self.renderer_version,
            stable_track_seed: stable_track_seed(&self.normalized_track_identity),
            palette: self.palette,
            width: self.size,
            height: self.size,
        }
    }
}

/// Stable FNV-1a over a versioned, length-prefixed UTF-8 identity. This is a
/// geometry seed, NOT a cryptographic identifier or an externally readable URI.
/// Changing serialization requires a renderer-version bump.
pub fn stable_track_seed(identity: &str) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let length = (identity.len() as u64).to_le_bytes();
    b"xmms-artwork-track-v1\0"
        .iter()
        .chain(length.iter())
        .chain(identity.as_bytes())
        .fold(OFFSET, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
        })
}

/// Normalize lexically, without opening/stat-ing a path or following symlinks.
/// Imported app-private tracks are ordinary files. This does NOT promise that
/// arbitrary symlinked paths with `..` refer to the same inode.
///
/// Only local, absolute file URIs are decoded. Nonlocal/malformed file URIs and
/// all other URI schemes remain opaque (including their escapes). Plain path
/// percent characters are literal. Private Android aliases are folded only when
/// `app_files_dir` proves which package/user-zero root belongs to this app.
pub fn normalize_track_identity(value: &str, app_files_dir: Option<&Path>) -> String {
    let path = if let Some(uri) = value.strip_prefix("file://") {
        let local = if uri.starts_with('/') {
            uri
        } else if let Some(local) = uri.strip_prefix("localhost/") {
            // Keep the leading slash without introducing a temporary reference.
            return normalize_file_uri(&format!("/{local}"), value, app_files_dir);
        } else {
            return value.to_owned();
        };
        return normalize_file_uri(local, value, app_files_dir);
    } else if value.contains("://") {
        return value.to_owned();
    } else {
        value
    };
    fold_private_alias(lexical_path(path), app_files_dir)
}

fn normalize_file_uri(uri: &str, original: &str, app_files_dir: Option<&Path>) -> String {
    // Query/fragment are not part of a local path identity; keep ambiguous URIs
    // opaque instead of pretending they name an ordinary imported file.
    if uri.contains(['?', '#']) {
        return original.to_owned();
    }
    let Some(decoded) = percent_decode(uri) else {
        return original.to_owned();
    };
    if decoded.contains('\0') {
        return original.to_owned();
    }
    fold_private_alias(lexical_path(&decoded), app_files_dir)
}

fn percent_decode(value: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut input = value.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let hex = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
            let high = hex(input.next()?)?;
            let low = hex(input.next()?)?;
            bytes.push(high * 16 + low);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok()
}

fn lexical_path(value: &str) -> String {
    let absolute = value.starts_with('/');
    let mut components = Vec::new();
    for component in value.split('/') {
        match component {
            "" | "." => {}
            ".." if components.last().is_some_and(|last| *last != "..") => {
                components.pop();
            }
            ".." if absolute => {}
            _ => components.push(component),
        }
    }
    let path = components.join("/");
    if absolute {
        format!("/{path}")
    } else if path.is_empty() {
        ".".to_owned()
    } else {
        path
    }
}

fn fold_private_alias(path: String, app_files_dir: Option<&Path>) -> String {
    let Some(files_dir) = app_files_dir.and_then(Path::to_str) else {
        return path;
    };
    let files_dir = lexical_path(files_dir);
    let package = files_dir
        .strip_prefix("/data/user/0/")
        .or_else(|| files_dir.strip_prefix("/data/data/"))
        .and_then(|suffix| suffix.strip_suffix("/files"))
        .filter(|package| !package.is_empty() && !package.contains('/'));
    let Some(package) = package else {
        return path;
    };
    let legacy = format!("/data/data/{package}");
    if let Some(suffix) = path
        .strip_prefix(&legacy)
        .filter(|s| s.is_empty() || s.starts_with('/'))
    {
        format!("/data/user/0/{package}{suffix}")
    } else {
        path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkSource {
    pub track_revision: u64,
    pub normalized_track_identity: String,
    pub skin_revision: u64,
    pub policy_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdentity {
    pub service_epoch: u64,
    pub request_generation: u64,
    pub source: ArtworkSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkRequest {
    pub identity: RequestIdentity,
    pub key: ArtworkKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schedule {
    Unchanged,
    Start(ArtworkRequest),
    Pending(ArtworkRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub publish: bool,
    pub next: Option<ArtworkRequest>,
}

/// Protocol model for one running job + one replaceable pending request.
/// Cancellation leaves a running job accounted for until it actually returns.
/// An executor must not mistake a stale/foreign completion for that return.
#[derive(Debug)]
pub struct LatestArtworkQueue {
    service_epoch: u64,
    generation: u64,
    desired: Option<ArtworkRequest>,
    running: Option<ArtworkRequest>,
    pending: Option<ArtworkRequest>,
    stopped: bool,
}

impl LatestArtworkQueue {
    pub fn new(service_epoch: u64) -> Self {
        Self {
            service_epoch,
            generation: 0,
            desired: None,
            running: None,
            pending: None,
            stopped: false,
        }
    }

    pub fn request(
        &mut self,
        source: ArtworkSource,
        key: ArtworkKey,
    ) -> Result<Schedule, ArtworkError> {
        if self.stopped {
            return Err(ArtworkError::Stopped);
        }
        if source.normalized_track_identity != key.normalized_track_identity {
            return Err(ArtworkError::IdentityMismatch);
        }
        key.spec().pixel_count()?;
        if self
            .desired
            .as_ref()
            .is_some_and(|r| r.identity.source == source && r.key == key)
        {
            return Ok(Schedule::Unchanged);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ArtworkError::GenerationExhausted)?;
        let request = ArtworkRequest {
            identity: RequestIdentity {
                service_epoch: self.service_epoch,
                request_generation: generation,
                source,
            },
            key,
        };
        self.generation = generation;
        self.desired = Some(request.clone());
        if self.running.is_none() {
            self.running = Some(request.clone());
            Ok(Schedule::Start(request))
        } else {
            self.pending = Some(request.clone());
            Ok(Schedule::Pending(request))
        }
    }

    pub fn finish(
        &mut self,
        request: &ArtworkRequest,
        current: &ArtworkSource,
        succeeded: bool,
    ) -> Completion {
        if self.running.as_ref() != Some(request) {
            return Completion {
                publish: false,
                next: None,
            };
        }
        let publish = succeeded
            && !self.stopped
            && self.desired.as_ref() == Some(request)
            && request.identity.source == *current;
        self.running = None;
        // A failed/current attempt may be explicitly retried after backoff by
        // requesting the same inputs again. There is no automatic retry loop.
        if !succeeded && self.desired.as_ref() == Some(request) {
            self.desired = None;
        }
        let next = if self.stopped {
            None
        } else {
            self.pending.take()
        };
        self.running = next.clone();
        Completion { publish, next }
    }

    pub fn cancel(&mut self) {
        self.desired = None;
        self.pending = None;
    }

    pub fn stop(&mut self) {
        self.cancel();
        self.stopped = true;
    }

    pub fn running(&self) -> Option<&ArtworkRequest> {
        self.running.as_ref()
    }

    pub fn pending(&self) -> Option<&ArtworkRequest> {
        self.pending.as_ref()
    }
}

/// Immutable publication revision shared by asynchronous Activity projections
/// and direct service updates. Equality to the current authoritative stamp is
/// required, not merely "newer than the last Java Intent".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionStamp {
    pub activity_generation: u64,
    pub media_revision: u64,
}

impl ProjectionStamp {
    pub fn is_current(self, authoritative: Self) -> bool {
        self == authoritative
    }
}

#[cfg(test)]
mod protocol_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_exhaustion_does_not_replace_running_state() {
        let mut queue = LatestArtworkQueue::new(1);
        queue.generation = u64::MAX;
        let source = ArtworkSource {
            track_revision: 1,
            normalized_track_identity: "a".into(),
            skin_revision: 1,
            policy_revision: 1,
        };
        let key = ArtworkKey {
            normalized_track_identity: "a".into(),
            palette: [[0; 3]; 24],
            size: 320,
            renderer_version: 1,
        };
        assert_eq!(
            queue.request(source, key),
            Err(ArtworkError::GenerationExhausted)
        );
        assert!(queue.running().is_none());
    }
}
