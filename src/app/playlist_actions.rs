//! Frontend-neutral playlist action mapping.

use crate::app::command::{AppCommand, PlayerCommand, PlaylistCommand};
use crate::player::PlayerState;
use crate::playlist::{Playlist, PlaylistMenuKind, PlaylistSortKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistSortAction {
    ListByTitle,
    ListByFilename,
    ListByPath,
    ListByDate,
    SelectionByTitle,
    SelectionByFilename,
    SelectionByPath,
    SelectionByDate,
    RandomizeList,
    ReverseList,
}

impl PlaylistSortAction {
    pub fn command(self) -> PlaylistCommand {
        match self {
            Self::ListByTitle => PlaylistCommand::Sort(PlaylistSortKey::Title),
            Self::ListByFilename => PlaylistCommand::Sort(PlaylistSortKey::Filename),
            Self::ListByPath => PlaylistCommand::Sort(PlaylistSortKey::Path),
            Self::ListByDate => PlaylistCommand::Sort(PlaylistSortKey::Date),
            Self::SelectionByTitle => PlaylistCommand::SortSelected(PlaylistSortKey::Title),
            Self::SelectionByFilename => PlaylistCommand::SortSelected(PlaylistSortKey::Filename),
            Self::SelectionByPath => PlaylistCommand::SortSelected(PlaylistSortKey::Path),
            Self::SelectionByDate => PlaylistCommand::SortSelected(PlaylistSortKey::Date),
            Self::RandomizeList => PlaylistCommand::Randomize,
            Self::ReverseList => PlaylistCommand::Reverse,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistSortMenuItem {
    pub label: &'static str,
    pub action: PlaylistSortAction,
}

pub fn playlist_row_click_commands(
    index: usize,
    double_click: bool,
    multi_select_modifier: bool,
) -> Vec<AppCommand> {
    if double_click {
        return vec![PlaylistCommand::ActivateEntry(index).into()];
    }
    if multi_select_modifier {
        return vec![PlaylistCommand::ToggleEntrySelection(index).into()];
    }
    vec![
        PlaylistCommand::SelectNone.into(),
        PlaylistCommand::SelectEntry(index).into(),
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistSelectedPlaybackCommands {
    pub selected_index: usize,
    pub commands: Vec<AppCommand>,
}

pub fn playlist_play_first_selected_commands(
    playlist: &Playlist,
    player_state: PlayerState,
) -> Option<PlaylistSelectedPlaybackCommands> {
    let selected_index = playlist.entries().iter().position(|entry| entry.selected)?;
    let commands = match (playlist.position() == Some(selected_index), player_state) {
        (true, PlayerState::Paused) => vec![PlayerCommand::Play.into()],
        (true, PlayerState::Playing) => Vec::new(),
        _ => playlist_row_click_commands(selected_index, true, false),
    };
    Some(PlaylistSelectedPlaybackCommands {
        selected_index,
        commands,
    })
}

const fn sort_item(label: &'static str, action: PlaylistSortAction) -> PlaylistSortMenuItem {
    PlaylistSortMenuItem { label, action }
}

pub const PLAYLIST_SORT_MENU_ITEMS: &[PlaylistSortMenuItem] = &[
    sort_item("Sort List: By Title", PlaylistSortAction::ListByTitle),
    sort_item("Sort List: By Filename", PlaylistSortAction::ListByFilename),
    sort_item(
        "Sort List: By Path + Filename",
        PlaylistSortAction::ListByPath,
    ),
    sort_item("Sort List: By Date", PlaylistSortAction::ListByDate),
    sort_item(
        "Sort Selection: By Title",
        PlaylistSortAction::SelectionByTitle,
    ),
    sort_item(
        "Sort Selection: By Filename",
        PlaylistSortAction::SelectionByFilename,
    ),
    sort_item(
        "Sort Selection: By Path + Filename",
        PlaylistSortAction::SelectionByPath,
    ),
    sort_item(
        "Sort Selection: By Date",
        PlaylistSortAction::SelectionByDate,
    ),
    sort_item("Randomize List", PlaylistSortAction::RandomizeList),
    sort_item("Reverse List", PlaylistSortAction::ReverseList),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistMenuCommand {
    OpenLocationWindow,
    OpenDirectoryDialog,
    OpenFileDialog,
    ShowSortMenu,
    ShowFileInfo,
    OpenOptions,
    ClearList,
    CropToSelection,
    RemoveSelectedOrCurrent,
    InvertSelection,
    SelectNone,
    SelectAll,
    SavePlaylist,
    LoadPlaylist,
}

impl PlaylistMenuCommand {
    pub fn from_menu_item(menu: PlaylistMenuKind, item: usize) -> Option<Self> {
        match (menu, item) {
            (PlaylistMenuKind::Add, 0) => Some(Self::OpenLocationWindow),
            (PlaylistMenuKind::Add, 1) => Some(Self::OpenDirectoryDialog),
            (PlaylistMenuKind::Add, 2) => Some(Self::OpenFileDialog),
            (PlaylistMenuKind::Misc, 0) => Some(Self::ShowSortMenu),
            (PlaylistMenuKind::Misc, 1) => Some(Self::ShowFileInfo),
            (PlaylistMenuKind::Misc, 2) => Some(Self::OpenOptions),
            (PlaylistMenuKind::Remove, 1) => Some(Self::ClearList),
            (PlaylistMenuKind::Remove, 2) => Some(Self::CropToSelection),
            (PlaylistMenuKind::Remove, 3) => Some(Self::RemoveSelectedOrCurrent),
            (PlaylistMenuKind::Select, 0) => Some(Self::InvertSelection),
            (PlaylistMenuKind::Select, 1) => Some(Self::SelectNone),
            (PlaylistMenuKind::Select, 2) => Some(Self::SelectAll),
            (PlaylistMenuKind::List, 0) => Some(Self::ClearList),
            (PlaylistMenuKind::List, 1) => Some(Self::SavePlaylist),
            (PlaylistMenuKind::List, 2) => Some(Self::LoadPlaylist),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_row_click_commands_match_frontend_selection_semantics() {
        let single = vec![
            PlaylistCommand::SelectNone.into(),
            PlaylistCommand::SelectEntry(3).into(),
        ];
        let play = vec![PlaylistCommand::ActivateEntry(3).into()];
        assert_eq!(playlist_row_click_commands(3, false, false), single);
        assert_eq!(
            playlist_row_click_commands(3, false, true),
            vec![PlaylistCommand::ToggleEntrySelection(3).into()]
        );
        assert_eq!(playlist_row_click_commands(3, true, false), play);
        assert_eq!(playlist_row_click_commands(3, true, true), play);
    }

    #[test]
    fn selected_playback_commands_resume_paused_current_entry() {
        let mut playlist = Playlist::new();
        for index in 0..3 {
            playlist.add_uri(format!("file:///tmp/{index}.ogg"));
        }
        playlist.entries_mut()[1].selected = true;
        playlist.set_position(1);

        assert_eq!(
            playlist_play_first_selected_commands(&playlist, PlayerState::Paused),
            Some(PlaylistSelectedPlaybackCommands {
                selected_index: 1,
                commands: vec![PlayerCommand::Play.into()],
            })
        );
    }

    #[test]
    fn selected_playback_commands_leave_playing_current_entry_unchanged() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///tmp/current.ogg");
        playlist.entries_mut()[0].selected = true;
        playlist.set_position(0);

        assert_eq!(
            playlist_play_first_selected_commands(&playlist, PlayerState::Playing),
            Some(PlaylistSelectedPlaybackCommands {
                selected_index: 0,
                commands: Vec::new(),
            })
        );
    }

    #[test]
    fn selected_playback_commands_restart_stopped_current_entry() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///tmp/current.ogg");
        playlist.entries_mut()[0].selected = true;
        playlist.set_position(0);

        assert_eq!(
            playlist_play_first_selected_commands(&playlist, PlayerState::Stopped),
            Some(PlaylistSelectedPlaybackCommands {
                selected_index: 0,
                commands: vec![PlaylistCommand::ActivateEntry(0).into()],
            })
        );
    }

    #[test]
    fn selected_playback_commands_start_first_different_selected_entry() {
        let mut playlist = Playlist::new();
        for index in 0..3 {
            playlist.add_uri(format!("file:///tmp/{index}.ogg"));
        }
        playlist.set_position(0);
        playlist.entries_mut()[2].selected = true;
        playlist.entries_mut()[1].selected = true;

        assert_eq!(
            playlist_play_first_selected_commands(&playlist, PlayerState::Paused),
            Some(PlaylistSelectedPlaybackCommands {
                selected_index: 1,
                commands: vec![PlaylistCommand::ActivateEntry(1).into()],
            })
        );

        playlist.select_all(false);
        assert_eq!(
            playlist_play_first_selected_commands(&playlist, PlayerState::Stopped),
            None
        );
    }

    #[test]
    fn playlist_sort_actions_map_to_playlist_commands() {
        for (action, command) in [
            (
                PlaylistSortAction::ListByTitle,
                PlaylistCommand::Sort(PlaylistSortKey::Title),
            ),
            (
                PlaylistSortAction::SelectionByFilename,
                PlaylistCommand::SortSelected(PlaylistSortKey::Filename),
            ),
            (
                PlaylistSortAction::RandomizeList,
                PlaylistCommand::Randomize,
            ),
            (PlaylistSortAction::ReverseList, PlaylistCommand::Reverse),
        ] {
            assert_eq!(action.command(), command);
        }
    }

    #[test]
    fn playlist_sort_menu_items_cover_expected_labels() {
        let labels: Vec<_> = PLAYLIST_SORT_MENU_ITEMS
            .iter()
            .map(|item| item.label)
            .collect();
        assert_eq!(
            labels,
            [
                "Sort List: By Title",
                "Sort List: By Filename",
                "Sort List: By Path + Filename",
                "Sort List: By Date",
                "Sort Selection: By Title",
                "Sort Selection: By Filename",
                "Sort Selection: By Path + Filename",
                "Sort Selection: By Date",
                "Randomize List",
                "Reverse List",
            ]
        );
    }

    #[test]
    fn playlist_sort_menu_items_all_dispatch_commands() {
        for item in PLAYLIST_SORT_MENU_ITEMS {
            match item.action.command() {
                PlaylistCommand::Sort(_)
                | PlaylistCommand::SortSelected(_)
                | PlaylistCommand::Randomize
                | PlaylistCommand::Reverse => {}
                other => panic!("unexpected sort command for {}: {other:?}", item.label),
            }
        }
    }

    #[test]
    fn playlist_menu_command_maps_menu_indices() {
        for (kind, index, command) in [
            (
                PlaylistMenuKind::Add,
                0,
                PlaylistMenuCommand::OpenLocationWindow,
            ),
            (
                PlaylistMenuKind::Add,
                2,
                PlaylistMenuCommand::OpenFileDialog,
            ),
            (
                PlaylistMenuKind::Remove,
                3,
                PlaylistMenuCommand::RemoveSelectedOrCurrent,
            ),
            (PlaylistMenuKind::List, 1, PlaylistMenuCommand::SavePlaylist),
        ] {
            assert_eq!(
                PlaylistMenuCommand::from_menu_item(kind, index),
                Some(command)
            );
        }
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Misc, 99),
            None
        );
    }
}
