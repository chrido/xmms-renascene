//! GTK equalizer window helpers.

use super::super::*;

pub(crate) fn build_equalizer_presets_popover(
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> gtk::Popover {
    let action_group = gtk::gio::SimpleActionGroup::new();
    let menu = gtk::gio::Menu::new();

    for section in EQUALIZER_PRESET_MENU_SECTIONS {
        let submenu = gtk::gio::Menu::new();
        for item in section.items {
            let action = item.action;
            let action_name = action.action_name();
            submenu.append(Some(item.label), Some(&format!("eq-presets.{action_name}")));
            install_equalizer_preset_action(
                &action_group,
                action,
                action_name,
                parent,
                main_state,
                main_area,
            );
        }
        if section.label == "Load" {
            let winamp_section = gtk::gio::Menu::new();
            for (index, preset) in winamp_original_presets().into_iter().enumerate() {
                let action_name = format!("load-winamp-original-preset-{index}");
                winamp_section.append(
                    Some(&preset.name),
                    Some(&format!("eq-presets.{action_name}")),
                );
                install_equalizer_direct_preset_action(
                    &action_group,
                    action_name,
                    preset,
                    parent,
                    main_state,
                    main_area,
                );
            }
            submenu.append_section(Some("Winamp original presets"), &winamp_section);

            let preset_section = gtk::gio::Menu::new();
            for (index, preset) in main_state
                .borrow()
                .sorted_equalizer_presets(false)
                .into_iter()
                .filter(|preset| !preset.name.eq_ignore_ascii_case("Default"))
                .enumerate()
            {
                let action_name = format!("load-named-preset-{index}");
                preset_section.append(
                    Some(&preset.name),
                    Some(&format!("eq-presets.{action_name}")),
                );
                install_equalizer_named_preset_action(
                    &action_group,
                    action_name,
                    preset.name,
                    parent,
                    main_state,
                    main_area,
                );
            }
            if preset_section.n_items() > 0 {
                submenu.append_section(Some("Presets"), &preset_section);
            }
        }
        menu.append_submenu(Some(section.label), &submenu);
    }

    install_equalizer_preset_action(
        &action_group,
        EQUALIZER_CONFIGURE_PRESET_ITEM.action,
        EQUALIZER_CONFIGURE_PRESET_ITEM.action.action_name(),
        parent,
        main_state,
        main_area,
    );
    menu.append(
        Some(EQUALIZER_CONFIGURE_PRESET_ITEM.label),
        Some(&format!(
            "eq-presets.{}",
            EQUALIZER_CONFIGURE_PRESET_ITEM.action.action_name()
        )),
    );

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
    action_name: String,
    preset: EqualizerPreset,
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) {
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

fn install_equalizer_named_preset_action(
    group: &gtk::gio::SimpleActionGroup,
    action_name: String,
    preset_name: String,
    parent: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) {
    let simple_action = gtk::gio::SimpleAction::new(&action_name, None);
    let main_state = Rc::clone(main_state);
    let parent = parent.clone();
    let main_area = main_area.clone();
    simple_action.connect_activate(move |_, _| {
        main_state
            .borrow_mut()
            .load_named_equalizer_preset(&preset_name, false);
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
        EqualizerPresetAction::LoadPreset => show_equalizer_preset_list_dialog(
            parent,
            "Load preset",
            false,
            false,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::LoadAutoPreset => show_equalizer_preset_list_dialog(
            parent,
            "Load auto-preset",
            true,
            false,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::LoadDefault => {
            main_state.borrow_mut().load_equalizer_default_preset();
            queue_equalizer_areas(&equalizer_area, &main_area);
        }
        EqualizerPresetAction::LoadZero => {
            main_state.borrow_mut().load_equalizer_zero_preset();
            queue_equalizer_areas(&equalizer_area, &main_area);
        }
        EqualizerPresetAction::LoadFromFile => show_equalizer_file_dialog(
            parent,
            "Load equalizer preset",
            gtk::FileChooserAction::Open,
            "Open",
            move |state, path| state.load_equalizer_preset_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::LoadFromWinampFile => show_equalizer_file_dialog(
            parent,
            "Load WinAMP equalizer preset",
            gtk::FileChooserAction::Open,
            "Open",
            move |state, path| state.load_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::ImportWinampPresets => show_equalizer_file_dialog(
            parent,
            "Import WinAMP equalizer presets",
            gtk::FileChooserAction::Open,
            "Import",
            move |state, path| state.import_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::SavePreset => show_equalizer_save_name_dialog(
            parent,
            "Save preset",
            None,
            false,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::SaveAutoPreset => {
            let default_name = main_state.borrow().current_playlist_basename();
            show_equalizer_save_name_dialog(
                parent,
                "Save auto-preset",
                default_name,
                true,
                main_state,
                equalizer_area,
                main_area,
            );
        }
        EqualizerPresetAction::SaveDefault => {
            if let Err(err) = main_state.borrow_mut().save_equalizer_default_preset() {
                eprintln!("xmms-rs: failed to save default equalizer preset: {err}");
            }
        }
        EqualizerPresetAction::SaveToFile => show_equalizer_file_dialog(
            parent,
            "Save equalizer preset",
            gtk::FileChooserAction::Save,
            "Save",
            move |state, path| state.save_equalizer_preset_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::SaveToWinampFile => show_equalizer_file_dialog(
            parent,
            "Save WinAMP equalizer preset",
            gtk::FileChooserAction::Save,
            "Save",
            move |state, path| state.save_equalizer_winamp_file(path).map(|_| ()),
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::DeletePreset => show_equalizer_preset_list_dialog(
            parent,
            "Delete preset",
            false,
            true,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::DeleteAutoPreset => show_equalizer_preset_list_dialog(
            parent,
            "Delete auto-preset",
            true,
            true,
            main_state,
            equalizer_area,
            main_area,
        ),
        EqualizerPresetAction::Configure => {
            show_equalizer_configure_dialog(parent, main_state, equalizer_area, main_area);
        }
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

fn show_equalizer_save_name_dialog(
    parent: &gtk::DrawingArea,
    title: &'static str,
    default_name: Option<String>,
    automatic: bool,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window(title, 320, 90, &[]);
    window.set_modal(true);
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    let entry = gtk::Entry::new();
    entry.set_text(default_name.as_deref().unwrap_or(""));
    layout.append(&entry);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let ok = gtk::Button::with_label("Ok");
    let cancel = gtk::Button::with_label("Cancel");
    {
        let window = window.clone();
        let entry = entry.clone();
        ok.connect_clicked(move |_| {
            let name = entry.text().trim().to_string();
            if !name.is_empty() {
                if let Err(err) = main_state
                    .borrow_mut()
                    .save_named_equalizer_preset(name, automatic)
                {
                    eprintln!("xmms-rs: failed to save equalizer preset: {err}");
                }
            }
            queue_equalizer_areas(&equalizer_area, &main_area);
            window.close();
        });
    }
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    buttons.append(&ok);
    buttons.append(&cancel);
    layout.append(&buttons);
    window.set_child(Some(&layout));
    window.present();
}

fn show_equalizer_preset_list_dialog(
    parent: &gtk::DrawingArea,
    title: &'static str,
    automatic: bool,
    delete_mode: bool,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window(title, 350, 300, &[]);
    window.set_modal(true);
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    let presets = main_state.borrow().sorted_equalizer_presets(automatic);
    if delete_mode {
        let mut checks = Vec::new();
        for preset in presets {
            let check = gtk::CheckButton::with_label(&preset.name);
            layout.append(&check);
            checks.push((preset.name, check));
        }
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let delete = gtk::Button::with_label("Delete");
        let close = gtk::Button::with_label("Close");
        {
            let window = window.clone();
            delete.connect_clicked(move |_| {
                let names: Vec<String> = checks
                    .iter()
                    .filter(|(_, check)| check.is_active())
                    .map(|(name, _)| name.clone())
                    .collect();
                if let Err(err) = main_state
                    .borrow_mut()
                    .delete_named_equalizer_presets(names, automatic)
                {
                    eprintln!("xmms-rs: failed to delete equalizer presets: {err}");
                }
                queue_equalizer_areas(&equalizer_area, &main_area);
                window.close();
            });
        }
        {
            let window = window.clone();
            close.connect_clicked(move |_| window.close());
        }
        buttons.append(&delete);
        buttons.append(&close);
        layout.append(&buttons);
    } else {
        for preset in presets {
            let button = gtk::Button::with_label(&preset.name);
            {
                let window = window.clone();
                let name = preset.name.clone();
                let main_state = Rc::clone(&main_state);
                let equalizer_area = equalizer_area.clone();
                let main_area = main_area.clone();
                button.connect_clicked(move |_| {
                    main_state
                        .borrow_mut()
                        .load_named_equalizer_preset(&name, automatic);
                    queue_equalizer_areas(&equalizer_area, &main_area);
                    window.close();
                });
            }
            layout.append(&button);
        }
        let close = gtk::Button::with_label("Cancel");
        {
            let window = window.clone();
            close.connect_clicked(move |_| window.close());
        }
        layout.append(&close);
    }
    window.set_child(Some(&layout));
    window.present();
}

fn show_equalizer_configure_dialog(
    parent: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    equalizer_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window("Configure Equalizer", 360, 140, &[]);
    window.set_modal(true);
    if let Some(parent_window) = area_window(parent) {
        window.set_transient_for(Some(&parent_window));
    }
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    let default_file = gtk::Entry::new();
    let extension = gtk::Entry::new();
    {
        let state = main_state.borrow();
        default_file.set_text(&state.app_state.config.eqpreset_default_file);
        extension.set_text(&state.app_state.config.eqpreset_extension);
    }
    layout.append(&gtk::Label::new(Some("Directory preset file:")));
    layout.append(&default_file);
    layout.append(&gtk::Label::new(Some("File preset extension:")));
    layout.append(&extension);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let ok = gtk::Button::with_label("Ok");
    let cancel = gtk::Button::with_label("Cancel");
    {
        let window = window.clone();
        ok.connect_clicked(move |_| {
            let mut state = main_state.borrow_mut();
            let default_file = default_file
                .text()
                .trim()
                .trim_start_matches('.')
                .to_string();
            let extension = extension.text().trim().trim_start_matches('.').to_string();
            state.update_config_via_store(|config| {
                config.eqpreset_default_file = default_file;
                config.eqpreset_extension = extension;
            });
            queue_equalizer_areas(&equalizer_area, &main_area);
            window.close();
        });
    }
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    buttons.append(&ok);
    buttons.append(&cancel);
    layout.append(&buttons);
    window.set_child(Some(&layout));
    window.present();
}
