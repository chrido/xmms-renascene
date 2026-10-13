//! Revisioned artwork settings. Snapshot/publication methods are memory-only;
//! blocking cold resolution belongs on the service's background artwork worker.
//! `config.skin` is the only durable selector: no palette snapshot is persisted.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use crate::config::Config;
use crate::skin::{DefaultSkin, DEFAULT_VIS_COLORS};

use super::{ArtworkError, ArtworkPalette, ArtworkPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkSettingsSnapshot {
    pub selector: Option<String>,
    pub palette: Option<ArtworkPalette>,
    pub policy: ArtworkPolicy,
    pub revision: u64,
    pub skin_revision: u64,
    pub policy_revision: u64,
}

impl Default for ArtworkSettingsSnapshot {
    fn default() -> Self {
        Self {
            selector: None,
            palette: None,
            policy: ArtworkPolicy::Off,
            revision: 0,
            skin_revision: 0,
            policy_revision: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsDiagnosticSource {
    Configuration,
    Palette,
}

/// Path-free, structured diagnostic. Only the winner of a cold resolution gets
/// this record from `commit_cold`; stale/duplicate attempts cannot spam logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsDiagnostic {
    pub source: SettingsDiagnosticSource,
    pub kind: io::ErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArtworkSettings {
    pub selector: Option<String>,
    pub palette: ArtworkPalette,
    pub policy: ArtworkPolicy,
    pub diagnostic: Option<SettingsDiagnostic>,
}

#[derive(Debug)]
pub struct SettingsUpdate {
    pub snapshot: Arc<ArtworkSettingsSnapshot>,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct ColdSettingsToken {
    // Pointer identity protects against foreign-store tokens as well as stale
    // revisions; it survives moving a store and requires no global test counter.
    expected: Arc<ArtworkSettingsSnapshot>,
}

#[derive(Debug)]
pub struct ColdSettingsCommit {
    pub snapshot: Arc<ArtworkSettingsSnapshot>,
    pub diagnostic: Option<SettingsDiagnostic>,
}

#[derive(Debug, Default)]
pub struct ArtworkSettingsStore {
    current: Mutex<Arc<ArtworkSettingsSnapshot>>,
}

impl ArtworkSettingsStore {
    pub fn snapshot(&self) -> Arc<ArtworkSettingsSnapshot> {
        Arc::clone(
            &self
                .current
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
        )
    }

    /// Publish an already resolved foreground palette. No archive/config I/O.
    pub fn publish_foreground(
        &self,
        selector: Option<String>,
        policy: ArtworkPolicy,
        palette: ArtworkPalette,
    ) -> Result<SettingsUpdate, ArtworkError> {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        replace(&mut current, selector, policy, palette)
    }

    /// Capture before scheduling cold disk work. None means the process already
    /// has resolved settings and must use memory, not reload potentially old disk.
    pub fn begin_cold_resolution(&self) -> Option<ColdSettingsToken> {
        let current = self.snapshot();
        current
            .palette
            .is_none()
            .then_some(ColdSettingsToken { expected: current })
    }

    /// Accept only the still-unresolved snapshot captured by this token. Do not
    /// hold this lock while loading config/archives. The caller emits any returned
    /// diagnostic once; a stale completion returns None and emits nothing.
    pub fn commit_cold(
        &self,
        token: &ColdSettingsToken,
        resolved: ResolvedArtworkSettings,
    ) -> Result<Option<ColdSettingsCommit>, ArtworkError> {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !Arc::ptr_eq(&current, &token.expected) || current.palette.is_some() {
            return Ok(None);
        }
        let update = replace(
            &mut current,
            resolved.selector,
            resolved.policy,
            resolved.palette,
        )?;
        Ok(Some(ColdSettingsCommit {
            snapshot: update.snapshot,
            diagnostic: resolved.diagnostic,
        }))
    }
}

fn replace(
    current: &mut Arc<ArtworkSettingsSnapshot>,
    selector: Option<String>,
    policy: ArtworkPolicy,
    palette: ArtworkPalette,
) -> Result<SettingsUpdate, ArtworkError> {
    let skin_changed = current.selector != selector || current.palette.as_ref() != Some(&palette);
    let policy_changed = current.policy != policy;
    if !skin_changed && !policy_changed {
        return Ok(SettingsUpdate {
            snapshot: Arc::clone(current),
            changed: false,
        });
    }
    let advance = |revision: u64| {
        revision
            .checked_add(1)
            .ok_or(ArtworkError::GenerationExhausted)
    };
    let revision = advance(current.revision)?;
    let skin_revision = if skin_changed {
        advance(current.skin_revision)?
    } else {
        current.skin_revision
    };
    let policy_revision = if policy_changed {
        advance(current.policy_revision)?
    } else {
        current.policy_revision
    };
    let snapshot = Arc::new(ArtworkSettingsSnapshot {
        selector,
        palette: Some(palette),
        policy,
        revision,
        skin_revision,
        policy_revision,
    });
    *current = Arc::clone(&snapshot);
    Ok(SettingsUpdate {
        snapshot,
        changed: true,
    })
}

pub const MAX_ARTWORK_CONFIG_BYTES: u64 = 64 * 1024;

/// BLOCKING: call only from a background artwork worker. This function reads
/// bounded saved config + palette text, not the playlist or skin bitmaps. It
/// never writes config, publishes global state, or holds the settings lock.
/// Use `commit_cold` separately; its token prevents older disk data winning a
/// foreground-update race and returns a diagnostic at most once.
pub fn resolve_artwork_settings(config_path: &Path) -> ResolvedArtworkSettings {
    let (config, mut diagnostic) = match read_config_bounded(config_path) {
        Ok(config) => (config, None),
        Err(err) if err.kind() == io::ErrorKind::NotFound => (Config::default(), None),
        Err(err) => (
            Config {
                artwork_policy: ArtworkPolicy::Off,
                ..Config::default()
            },
            Some(SettingsDiagnostic {
                source: SettingsDiagnosticSource::Configuration,
                kind: err.kind(),
            }),
        ),
    };
    let palette = match config.skin.as_deref() {
        Some(selector) => match DefaultSkin::load_vis_colors_from_path(Path::new(selector)) {
            Ok(palette) => palette,
            Err(err) => {
                diagnostic = Some(SettingsDiagnostic {
                    source: SettingsDiagnosticSource::Palette,
                    kind: err.kind(),
                });
                DEFAULT_VIS_COLORS
            }
        },
        None => DEFAULT_VIS_COLORS,
    };
    ResolvedArtworkSettings {
        selector: config.skin,
        palette,
        policy: config.artwork_policy,
        diagnostic,
    }
}

fn read_config_bounded(path: &Path) -> io::Result<Config> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "artwork config is not a regular file",
        ));
    }
    let mut text = String::new();
    File::open(path)?
        .take(MAX_ARTWORK_CONFIG_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_ARTWORK_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "artwork config exceeds byte limit",
        ));
    }
    Ok(Config::from_key_file_str(&text))
}

/// Process-wide state used by Android workers and foreground skin hooks. Host
/// tests construct independent stores instead of mutating this global singleton.
pub fn process_artwork_settings() -> &'static ArtworkSettingsStore {
    static SETTINGS: OnceLock<ArtworkSettingsStore> = OnceLock::new();
    SETTINGS.get_or_init(ArtworkSettingsStore::default)
}

#[cfg(test)]
mod tests;
