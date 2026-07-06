//! GTK playlist window helpers.

use super::super::*;

pub(crate) fn build_playlist_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    open_location_window: &gtk::ApplicationWindow,
) -> (gtk::ApplicationWindow, gtk::DrawingArea) {
    let (playlist_width, playlist_height) = main_state.borrow().playlist_size();
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Renascene Rust Playlist")
        .resizable(true)
        .decorated(false)
        .default_width(playlist_width * DEFAULT_SCALE)
        .default_height(playlist_height * DEFAULT_SCALE)
        .build();
    let drawing_area = gtk::DrawingArea::builder()
        .content_width(playlist_width * DEFAULT_SCALE)
        .content_height(playlist_height * DEFAULT_SCALE)
        .focusable(true)
        .build();
    let state = Rc::clone(main_state);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let state = state.borrow();
        let skin = state.active_skin();
        let shaded = state.playlist_ui.panel.shaded;
        let focused = state.playlist_focused();
        let playlist_width = state.playlist_ui.width;
        let playlist_height = state.playlist_ui.height;
        let base_height = if shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            playlist_height
        };
        match render_scaled(
            cr,
            width,
            height,
            playlist_width,
            base_height,
            |cr, pass| {
                if pass.is_bitmap() {
                    render_playlist_frame(
                        cr,
                        skin,
                        focused,
                        shaded,
                        playlist_width,
                        playlist_height,
                        Some(&state.shaded_playlist_info()),
                        Some(&state.playlist_footer_info()),
                        Some(&state.playlist_footer_time_min_text()),
                        Some(&state.playlist_footer_time_sec_text()),
                    )?;
                }
                if !shaded {
                    let row_state = state.playlist_rows_render_state();
                    render_playlist_rows(cr, skin, &row_state, pass)?;
                }
                if pass.is_text() {
                    if let Some(menu) = state.playlist_menu() {
                        let (x, y, w, h) =
                            playlist_menu_rect(menu, playlist_width, playlist_height);
                        let render_state = PlaylistMenuRenderState {
                            kind: menu.render_kind(),
                            hover: state.playlist_menu_hover(),
                        };
                        paint_scaled(cr, x, y, w, h, |menu_cr| {
                            render_playlist_menu(menu_cr, skin, render_state).map(|_| ())
                        })?;
                    }
                }
                Ok(())
            },
        ) {
            Ok(()) => app_log_trace!(
                render,
                "gtk playlist",
                width,
                height,
                playlist_width,
                playlist_height
            ),
            Err(err) => eprintln!("xmms-rs: failed to render playlist preview: {err}"),
        }
    });

    add_file_drop_controller(&drawing_area, Rc::clone(main_state), false, false);
    add_playlist_context_menu(&drawing_area, Rc::clone(main_state), main_area.clone());
    add_playlist_key_controller(&drawing_area, Rc::clone(main_state));

    {
        let main_state = Rc::clone(main_state);
        drawing_area.connect_resize(move |area, width, height| {
            let mut state = main_state.borrow_mut();
            if !state.is_panel_detached(PanelKind::Playlist) {
                return;
            }
            let scale = state.scale_factor();
            let base_height = if state.playlist_ui.panel.shaded {
                state.playlist_ui.height
            } else {
                unscale_dim(height, scale).max(PLAYLIST_MIN_HEIGHT)
            };
            if state.set_playlist_size(
                unscale_dim(width, scale).max(PLAYLIST_MIN_WIDTH),
                base_height,
            ) {
                area.queue_draw();
            }
        });
    }
    add_panel_click_controller(
        &window,
        &drawing_area,
        Rc::clone(main_state),
        main_area.clone(),
        PanelKind::Playlist,
        None,
        Some(open_location_window.clone()),
        Some(build_playlist_sort_popover(
            &drawing_area,
            main_state,
            main_area,
        )),
    );
    window.set_child(Some(&drawing_area));
    (window, drawing_area)
}
