//! GTK playlist menu helpers.

use super::super::*;

pub(crate) fn build_playlist_sort_popover(
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .build();
    style_xmms_popover(&popover);
    popover.set_parent(parent);
    let menu_box = xmms_menu_box(0);
    for sort_item in PLAYLIST_SORT_MENU_ITEMS {
        let action = sort_item.action;
        let item = xmms_menu_button(sort_item.label);
        {
            let main_state = Rc::clone(main_state);
            let parent = parent.clone();
            let main_area = main_area.clone();
            let popover = popover.clone();
            item.connect_clicked(move |_| {
                main_state
                    .borrow_mut()
                    .activate_playlist_sort_action(action);
                popover.popdown();
                parent.queue_draw();
                main_area.queue_draw();
            });
        }
        menu_box.append(&item);
    }
    popover.set_child(Some(&menu_box));
    popover
}

pub(crate) fn show_playlist_sort_menu(popover: &gtk::Popover, area: &gtk::DrawingArea) {
    let width = area.allocated_width().max(1) as f64;
    let height = area.allocated_height().max(1) as f64;
    let rect = gtk::gdk::Rectangle::new(
        (99.0 * (width / f64::from(PLAYLIST_DEFAULT_WIDTH))) as i32,
        (f64::from(PLAYLIST_DEFAULT_HEIGHT - 29) * (height / f64::from(PLAYLIST_DEFAULT_HEIGHT)))
            as i32,
        25,
        1,
    );
    popover.set_pointing_to(Some(&rect));
    popover.popup();
}
