//! Frontend-neutral input event domains.
//!
//! Concrete UI toolkits translate their native keyboard/pointer events into
//! these shared input intents before dispatching commands or frontend actions.

use crate::app::command::{AppCommand, PanelCommand, PlayerCommand, PlaylistCommand, UiCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppShortcut {
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    OpenFiles,
    ToggleRepeat,
    ToggleShuffle,
    Preferences,
    OpenLocation,
    ToggleNoAdvance,
    ShadeMain,
    JumpTime,
    SkinBrowser,
    OpenDirectory,
    PresentMain,
    TogglePlaylist,
    ToggleEqualizer,
    ShadePlaylist,
    ShadeEqualizer,
    ToggleTimerRemaining,
    ToggleSticky,
    DoubleScale,
    HalfScale,
    ToggleEasyMove,
    StartOfList,
    FileInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutSpec {
    pub accelerator: &'static str,
    pub shortcut: AppShortcut,
}

macro_rules! shortcut_specs {
    ($($accelerator:literal => $shortcut:ident),* $(,)?) => {
        &[
            $(ShortcutSpec {
                accelerator: $accelerator,
                shortcut: AppShortcut::$shortcut,
            }),*
        ]
    };
}

impl AppShortcut {
    pub fn command(self) -> Option<AppCommand> {
        let command = match self {
            AppShortcut::Previous => PlayerCommand::PreviousTrack.into(),
            AppShortcut::Play => PlayerCommand::Play.into(),
            AppShortcut::Pause => PlayerCommand::Pause.into(),
            AppShortcut::Stop => PlayerCommand::Halt.into(),
            AppShortcut::Next => PlayerCommand::NextTrack.into(),
            AppShortcut::ToggleRepeat => PlaylistCommand::ToggleRepeat.into(),
            AppShortcut::ToggleShuffle => PlaylistCommand::ToggleShuffle.into(),
            AppShortcut::Preferences => UiCommand::SetPreferencesVisible(true).into(),
            AppShortcut::ToggleNoAdvance => PlaylistCommand::ToggleNoAdvance.into(),
            AppShortcut::ShadeMain => PanelCommand::ToggleMainShade.into(),
            AppShortcut::SkinBrowser => UiCommand::SetSkinBrowserVisible(true).into(),
            AppShortcut::TogglePlaylist => PanelCommand::TogglePlaylistVisibility.into(),
            AppShortcut::ToggleEqualizer => PanelCommand::ToggleEqualizerVisibility.into(),
            AppShortcut::ShadePlaylist => PanelCommand::TogglePlaylistShade.into(),
            AppShortcut::ShadeEqualizer => PanelCommand::ToggleEqualizerShade.into(),
            AppShortcut::FileInfo => UiCommand::SetFileInfoVisible(true).into(),
            AppShortcut::OpenFiles
            | AppShortcut::OpenLocation
            | AppShortcut::JumpTime
            | AppShortcut::OpenDirectory
            | AppShortcut::PresentMain
            | AppShortcut::ToggleTimerRemaining
            | AppShortcut::ToggleSticky
            | AppShortcut::DoubleScale
            | AppShortcut::HalfScale
            | AppShortcut::ToggleEasyMove
            | AppShortcut::StartOfList => return None,
        };
        Some(command)
    }
}

pub const APP_SHORTCUTS: &[ShortcutSpec] = shortcut_specs! {
    "z" => Previous,
    "x" => Play,
    "c" => Pause,
    "v" => Stop,
    "b" => Next,
    "l" => OpenFiles,
    "r" => ToggleRepeat,
    "s" => ToggleShuffle,
    "<Control>p" => Preferences,
    "<Control>l" => OpenLocation,
    "<Control>n" => ToggleNoAdvance,
    "<Control>w" => ShadeMain,
    "<Control>j" => JumpTime,
    "<Alt>s" => SkinBrowser,
    "<Shift>l" => OpenDirectory,
    "<Alt>w" => PresentMain,
    "<Alt>e" => TogglePlaylist,
    "<Alt>g" => ToggleEqualizer,
    "<Control><Shift>w" => ShadePlaylist,
    "<Control><Alt>w" => ShadeEqualizer,
    "<Control>r" => ToggleTimerRemaining,
    "<Control>a" => ToggleSticky,
    "<Control>d" => DoubleScale,
    "<Control>m" => HalfScale,
    "<Control>e" => ToggleEasyMove,
    "<Control>z" => StartOfList,
    "<Control>3" => FileInfo,
    "Insert" => OpenFiles,
    "<Shift>Insert" => OpenDirectory,
    "<Alt>Insert" => OpenLocation,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_shortcut_table_contains_core_transport_keys() {
        assert!(APP_SHORTCUTS
            .iter()
            .any(|spec| spec.accelerator == "x" && spec.shortcut == AppShortcut::Play));
        assert!(APP_SHORTCUTS
            .iter()
            .any(|spec| spec.accelerator == "<Control>p"
                && spec.shortcut == AppShortcut::Preferences));
    }
}
