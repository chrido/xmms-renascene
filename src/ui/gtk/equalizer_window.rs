//! GTK equalizer window helpers.

use super::super::*;

pub(crate) fn build_equalizer_presets_popover(
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> gtk::Popover {
    let action_group = gtk::gio::SimpleActionGroup::new();
    let menu = gtk::gio::Menu::new();

    for item in EQUALIZER_PRESET_FILE_ITEMS {
        let action = item.action;
        let action_name = action.action_name();
        menu.append(Some(item.label), Some(&format!("eq-presets.{action_name}")));
        install_equalizer_preset_action(
            &action_group,
            action,
            action_name,
            parent,
            main_state,
            main_area,
        );
    }

    let presets = gtk::gio::Menu::new();
    for (index, preset) in built_in_equalizer_presets().into_iter().enumerate() {
        install_equalizer_direct_preset_action(
            &action_group,
            &presets,
            format!("load-built-in-preset-{index}"),
            preset,
            parent,
            main_state,
            main_area,
        );
    }
    menu.append_section(None, &presets);

    parent.insert_action_group("eq-presets", Some(&action_group));
    let popover_menu = gtk::PopoverMenu::from_model_full(&menu, gtk::PopoverMenuFlags::NESTED);
    popover_menu.set_autohide(true);
    popover_menu.set_has_arrow(false);
    let popover: gtk::Popover = popover_menu.upcast();
    style_xmms_popover(&popover);
    popover.set_parent(parent);
    popover
}

fn install_equalizer_preset_action(
    group: &gtk::gio::SimpleActionGroup,
    action: EqualizerPresetAction,
    action_name: &'static str,
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) {
    let simple_action = gtk::gio::SimpleAction::new(action_name, None);
    let main_state = Rc::clone(main_state);
    let parent = parent.clone();
    let main_area = main_area.clone();
    simple_action.connect_activate(move |_, _| {
        activate_equalizer_preset_action(
            action,
            &parent,
            Rc::clone(&main_state),
            parent.clone(),
            main_area.clone(),
        );
    });
    group.add_action(&simple_action);
}

fn install_equalizer_direct_preset_action(
    group: &gtk::gio::SimpleActionGroup,
    menu: &gtk::gio::Menu,
    action_name: String,
    preset: EqualizerPreset,
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) {
    menu.append(
        Some(&preset.name),
        Some(&format!("eq-presets.{action_name}")),
    );
    let simple_action = gtk::gio::SimpleAction::new(&action_name, None);
    let main_state = Rc::clone(main_state);
    let parent = parent.clone();
    let main_area = main_area.clone();
    simple_action.connect_activate(move |_, _| {
        main_state
            .borrow_mut()
            .apply_equalizer_preset_values(&preset);
        parent.queue_draw();
        main_area.queue_draw();
    });
    group.add_action(&simple_action);
}

pub(crate) fn show_equalizer_presets_menu(popover: &gtk::Popover, area: &gtk::DrawingArea) {
    let scale_x = area.allocated_width().max(1) as f64 / f64::from(EQUALIZER_WINDOW_WIDTH);
    let scale_y = area.allocated_height().max(1) as f64 / f64::from(EQUALIZER_WINDOW_HEIGHT);
    let rect = gtk::gdk::Rectangle::new(
        (217.0 * scale_x) as i32,
        (30.0 * scale_y) as i32,
        (44.0 * scale_x).max(1.0) as i32,
        1,
    );
    popover.set_pointing_to(Some(&rect));
    popover.popup();
}

pub(crate) fn show_docked_equalizer_presets_menu(
    popover: &gtk::Popover,
    area: &gtk::DrawingArea,
    state: &MainWindowUiState,
) {
    let (base_width, base_height) = state.docked_panel_size();
    let scale_x = area.allocated_width().max(1) as f64 / f64::from(base_width);
    let scale_y = area.allocated_height().max(1) as f64 / f64::from(base_height);
    let y_offset = main_window_height(state.shaded);
    let rect = gtk::gdk::Rectangle::new(
        (217.0 * scale_x) as i32,
        (f64::from(y_offset + 30) * scale_y) as i32,
        (44.0 * scale_x).max(1.0) as i32,
        1,
    );
    popover.set_pointing_to(Some(&rect));
    popover.popup();
}

fn activate_equalizer_preset_action(
    action: EqualizerPresetAction,
    parent: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    match action {
        EqualizerPresetAction::Load => show_equalizer_file_dialog(
            parent,
            "Load equalizer preset",
            gtk::FileChooserAction::Open,
            "Open",
            move |state, path| state.load_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::Save => show_equalizer_file_dialog(
            parent,
            "Save equalizer preset",
            gtk::FileChooserAction::Save,
            "Save",
            move |state, path| state.save_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
    }
}

fn queue_equalizer_areas(equalizer_area: &gtk::DrawingArea, main_area: &gtk::DrawingArea) {
    equalizer_area.queue_draw();
    main_area.queue_draw();
}

fn area_window(parent: &gtk::DrawingArea) -> Option<gtk::Window> {
    parent
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
}

fn show_equalizer_file_dialog(
    parent: &gtk::DrawingArea,
    title: &'static str,
    action: gtk::FileChooserAction,
    accept: &'static str,
    handler: impl Fn(&mut MainWindowUiState, &Path) -> io::Result<()> + 'static,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let parent_window = area_window(parent);
    let dialog = gtk::FileChooserNative::new(
        Some(title),
        parent_window.as_ref(),
        action,
        Some(accept),
        Some("Cancel"),
    );
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Winamp EQF files"));
    filter.add_pattern("*.eqf");
    dialog.add_filter(&filter);
    if action == gtk::FileChooserAction::Save {
        dialog.set_current_name("preset.eqf");
    }
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|file| file.path()) {
                if let Err(err) = handler(&mut main_state.borrow_mut(), &path) {
                    eprintln!(
                        "xmms-rs: equalizer file action failed for {}: {err}",
                        path.display()
                    );
                }
            }
        }
        queue_equalizer_areas(&equalizer_area, &main_area);
        dialog_for_response.destroy();
    });
    dialog.show();
}
