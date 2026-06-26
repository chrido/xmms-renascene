//! Frontend-neutral playlist action mapping.

use crate::playlist::PlaylistMenuKind;

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
        assert_eq!(PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Misc, 99), None);
    }
}
