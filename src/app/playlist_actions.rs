//! Frontend-neutral playlist action mapping.

use crate::app::command::{AppCommand, PlayerCommand, PlaylistCommand};
use crate::playlist::{PlaylistMenuKind, PlaylistSortKey};

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
        return vec![
            PlaylistCommand::SetPosition(index).into(),
            PlayerCommand::StartCurrentTrack.into(),
        ];
    }
    if multi_select_modifier {
        return vec![PlaylistCommand::ToggleEntrySelection(index).into()];
    }
    vec![
        PlaylistCommand::SelectNone.into(),
        PlaylistCommand::ToggleEntrySelection(index).into(),
    ]
}

pub const PLAYLIST_SORT_MENU_ITEMS: &[PlaylistSortMenuItem] = &[
    PlaylistSortMenuItem {
        label: "Sort List: By Title",
        action: PlaylistSortAction::ListByTitle,
    },
    PlaylistSortMenuItem {
        label: "Sort List: By Filename",
        action: PlaylistSortAction::ListByFilename,
    },
    PlaylistSortMenuItem {
        label: "Sort List: By Path + Filename",
        action: PlaylistSortAction::ListByPath,
    },
    PlaylistSortMenuItem {
        label: "Sort List: By Date",
        action: PlaylistSortAction::ListByDate,
    },
    PlaylistSortMenuItem {
        label: "Sort Selection: By Title",
        action: PlaylistSortAction::SelectionByTitle,
    },
    PlaylistSortMenuItem {
        label: "Sort Selection: By Filename",
        action: PlaylistSortAction::SelectionByFilename,
    },
    PlaylistSortMenuItem {
        label: "Sort Selection: By Path + Filename",
        action: PlaylistSortAction::SelectionByPath,
    },
    PlaylistSortMenuItem {
        label: "Sort Selection: By Date",
        action: PlaylistSortAction::SelectionByDate,
    },
    PlaylistSortMenuItem {
        label: "Randomize List",
        action: PlaylistSortAction::RandomizeList,
    },
    PlaylistSortMenuItem {
        label: "Reverse List",
        action: PlaylistSortAction::ReverseList,
    },
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
        assert_eq!(
            playlist_row_click_commands(3, false, false),
            vec![
                PlaylistCommand::SelectNone.into(),
                PlaylistCommand::ToggleEntrySelection(3).into(),
            ]
        );
        assert_eq!(
            playlist_row_click_commands(3, false, true),
            vec![PlaylistCommand::ToggleEntrySelection(3).into()]
        );
        assert_eq!(
            playlist_row_click_commands(3, true, false),
            vec![
                PlaylistCommand::SetPosition(3).into(),
                PlayerCommand::StartCurrentTrack.into(),
            ]
        );
        assert_eq!(
            playlist_row_click_commands(3, true, true),
            vec![
                PlaylistCommand::SetPosition(3).into(),
                PlayerCommand::StartCurrentTrack.into(),
            ]
        );
    }

    #[test]
    fn playlist_sort_actions_map_to_playlist_commands() {
        assert_eq!(
            PlaylistSortAction::ListByTitle.command(),
            PlaylistCommand::Sort(PlaylistSortKey::Title)
        );
        assert_eq!(
            PlaylistSortAction::SelectionByFilename.command(),
            PlaylistCommand::SortSelected(PlaylistSortKey::Filename)
        );
        assert_eq!(
            PlaylistSortAction::RandomizeList.command(),
            PlaylistCommand::Randomize
        );
        assert_eq!(
            PlaylistSortAction::ReverseList.command(),
            PlaylistCommand::Reverse
        );
    }

    #[test]
    fn playlist_sort_menu_items_cover_expected_labels() {
        let labels: Vec<_> = PLAYLIST_SORT_MENU_ITEMS
            .iter()
            .map(|item| item.label)
            .collect();
        assert_eq!(
            labels,
            vec![
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
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Add, 0),
            Some(PlaylistMenuCommand::OpenLocationWindow)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Add, 2),
            Some(PlaylistMenuCommand::OpenFileDialog)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Remove, 3),
            Some(PlaylistMenuCommand::RemoveSelectedOrCurrent)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::List, 1),
            Some(PlaylistMenuCommand::SavePlaylist)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Misc, 99),
            None
        );
    }
}
