//! Lifecycle-owned managed-playlist UI and filesystem operations.

use std::fs;
use std::path::{Path, PathBuf};

use crate::playlist::Playlist;
use crate::session::default_config_dir;

#[derive(Debug, Clone)]
pub(crate) enum PlaylistManagerAction {
    Close,
    Save,
    Import,
    Load(PathBuf),
    Delete(PathBuf),
    Export(PathBuf),
}

pub(crate) enum PlaylistManagerOutcome {
    None,
    ImportRequested,
    PlaylistLoaded(Playlist),
}

pub(crate) struct PlaylistManager {
    open: bool,
    name: String,
    saved_playlists: Vec<PathBuf>,
}

impl PlaylistManager {
    pub fn new() -> Self {
        Self {
            open: false,
            name: "playlist".to_string(),
            saved_playlists: discover_managed_playlists(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.refresh();
        self.open = true;
    }

    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    pub fn saved_playlists(&self) -> &[PathBuf] {
        &self.saved_playlists
    }

    pub fn handle_action(
        &mut self,
        action: PlaylistManagerAction,
        current_playlist: &Playlist,
    ) -> Result<PlaylistManagerOutcome, String> {
        match action {
            PlaylistManagerAction::Close => {
                self.open = false;
                Ok(PlaylistManagerOutcome::None)
            }
            PlaylistManagerAction::Save => {
                self.save(current_playlist)?;
                Ok(PlaylistManagerOutcome::None)
            }
            PlaylistManagerAction::Import => Ok(PlaylistManagerOutcome::ImportRequested),
            PlaylistManagerAction::Load(path) => {
                let Some(playlist) = self.load(&path)? else {
                    return Ok(PlaylistManagerOutcome::None);
                };
                self.open = false;
                Ok(PlaylistManagerOutcome::PlaylistLoaded(playlist))
            }
            PlaylistManagerAction::Delete(path) => {
                self.delete(&path)?;
                Ok(PlaylistManagerOutcome::None)
            }
            PlaylistManagerAction::Export(path) => {
                self.export(&path)?;
                Ok(PlaylistManagerOutcome::None)
            }
        }
    }

    pub fn import(&mut self, source: &Path) -> Result<(), String> {
        let directory = managed_playlist_dir();
        fs::create_dir_all(&directory)
            .map_err(|err| format!("failed to create playlist storage: {err}"))?;
        let source_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("imported.m3u8");
        let name = managed_playlist_name(source_name);
        let destination = unique_managed_playlist_path(&directory, &name);
        fs::copy(source, &destination)
            .map_err(|err| format!("failed to import playlist '{}': {err}", source.display()))?;
        self.refresh();
        Ok(())
    }

    fn save(&mut self, playlist: &Playlist) -> Result<(), String> {
        let name = managed_playlist_name(&self.name);
        if name.is_empty() {
            return Err("enter a playlist name".to_string());
        }
        let directory = managed_playlist_dir();
        fs::create_dir_all(&directory)
            .map_err(|err| format!("failed to create playlist storage: {err}"))?;
        let path = directory.join(&name);
        playlist
            .save_m3u_file(&path)
            .map_err(|err| format!("failed to save playlist '{}': {err}", path.display()))?;
        self.name = name;
        self.refresh();
        Ok(())
    }

    fn load(&self, path: &Path) -> Result<Option<Playlist>, String> {
        if !is_managed_playlist_path(path) {
            return Ok(None);
        }
        Playlist::load_m3u_file(path)
            .map(Some)
            .map_err(|err| format!("failed to load playlist '{}': {err}", path.display()))
    }

    fn delete(&mut self, path: &Path) -> Result<(), String> {
        if !is_managed_playlist_path(path) {
            return Ok(());
        }
        fs::remove_file(path)
            .map_err(|err| format!("failed to delete playlist '{}': {err}", path.display()))?;
        self.refresh();
        Ok(())
    }

    fn export(&self, path: &Path) -> Result<(), String> {
        if !is_managed_playlist_path(path) {
            return Ok(());
        }
        let base_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(managed_playlist_name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "playlist".to_string());
        let name = format!("{base_name}.m3u8");
        let contents = fs::read(path)
            .map_err(|err| format!("failed to read playlist '{}': {err}", path.display()))?;
        super::picker::save_playlist(&contents, &name)
    }

    fn refresh(&mut self) {
        self.saved_playlists = discover_managed_playlists();
    }
}

pub(crate) fn managed_playlist_name(name: &str) -> String {
    let mut name: String = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let lowercase = name.to_ascii_lowercase();
    if lowercase.ends_with(".m3u8") {
        name.truncate(name.len() - 5);
    } else if lowercase.ends_with(".m3u") {
        name.truncate(name.len() - 4);
    }
    name.trim().to_string()
}

fn managed_playlist_dir() -> PathBuf {
    default_config_dir().join("playlists")
}

fn discover_managed_playlists() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(managed_playlist_dir()) else {
        return Vec::new();
    };
    let mut playlists: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    playlists.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });
    playlists
}

fn is_managed_playlist_path(path: &Path) -> bool {
    path.parent() == Some(managed_playlist_dir().as_path())
}

fn unique_managed_playlist_path(directory: &Path, name: &str) -> PathBuf {
    let path = directory.join(name);
    if !path.exists() {
        return path;
    }
    for suffix in 2.. {
        let candidate = directory.join(format!("{name}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}
