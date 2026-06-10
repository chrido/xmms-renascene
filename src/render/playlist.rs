use cairo::Context;

use super::core::{
    blit_surface_rect, render_text, set_rgb, surface_from_xpm, RenderError, RenderPass,
};
use crate::skin::layout::{
    playlist_window_height, PLAYLIST_HEIGHT_BASE, PLAYLIST_MIN_HEIGHT, PLAYLIST_MIN_WIDTH,
    PLAYLIST_WIDTH_STEP,
};
use crate::skin::{DefaultSkin, SkinPixmapKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistMenuRenderState {
    pub kind: PlaylistMenuRenderKind,
    pub hover: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRowRenderEntry {
    pub title: String,
    pub length_ms: i64,
    pub selected: bool,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRowsRenderState {
    pub entries: Vec<PlaylistRowRenderEntry>,
    pub scroll_offset: usize,
    pub scrollbar_dragging: bool,
    pub search_query: Option<String>,
    pub show_numbers: bool,
    pub font_family: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistMenuRenderKind {
    Add,
    Remove,
    Select,
    Misc,
    List,
}

impl PlaylistMenuRenderKind {
    pub fn item_count(self) -> usize {
        match self {
            Self::Add | Self::Select | Self::Misc | Self::List => 3,
            Self::Remove => 4,
        }
    }
}

pub fn render_playlist_frame(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
    width: i32,
    height: i32,
    shaded_info: Option<&str>,
    footer_info: Option<&str>,
    footer_time_min: Option<&str>,
    footer_time_sec: Option<&str>,
) -> Result<bool, RenderError> {
    let width = width.max(PLAYLIST_MIN_WIDTH);
    let height = playlist_window_height(shaded, height);
    let Some(pledit) = skin.get(SkinPixmapKind::PlEdit) else {
        return Ok(false);
    };
    let pledit = surface_from_xpm(pledit)?;

    if shaded {
        blit_surface_rect(cr, &pledit, 72, 42, 0, 0, 25, 14)?;
        let mut x = 25;
        let right_cap_x = width - 50;
        while x < right_cap_x {
            let tile_width = (right_cap_x - x).min(25);
            blit_surface_rect(cr, &pledit, 72, 57, x, 0, tile_width, 14)?;
            x += tile_width;
        }
        blit_surface_rect(
            cr,
            &pledit,
            99,
            if focused { 42 } else { 57 },
            width - 50,
            0,
            50,
            14,
        )?;
        draw_playlist_shaded_info(cr, skin, width, shaded_info.unwrap_or(""))?;
        return Ok(true);
    }

    let title_y = if focused { 0 } else { 21 };

    let colors = skin.playlist_colors();
    cr.set_source_rgb(
        f64::from(colors.normal_bg[0]) / 255.0,
        f64::from(colors.normal_bg[1]) / 255.0,
        f64::from(colors.normal_bg[2]) / 255.0,
    );
    cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    cr.fill()?;

    blit_surface_rect(cr, &pledit, 0, title_y, 0, 0, 25, 20)?;
    let mut count = (width - 150) / 25;
    for i in 0..count / 2 {
        blit_surface_rect(cr, &pledit, 127, title_y, (i * 25) + 25, 0, 25, 20)?;
        blit_surface_rect(
            cr,
            &pledit,
            127,
            title_y,
            (i * 25) + (width / 2) + 50,
            0,
            25,
            20,
        )?;
    }
    if count & 1 == 1 {
        blit_surface_rect(
            cr,
            &pledit,
            127,
            title_y,
            ((count / 2) * 25) + 25,
            0,
            12,
            20,
        )?;
        blit_surface_rect(
            cr,
            &pledit,
            127,
            title_y,
            (width / 2) + ((count / 2) * 25) + 50,
            0,
            13,
            20,
        )?;
    }
    blit_surface_rect(cr, &pledit, 26, title_y, (width / 2) - 50, 0, 100, 20)?;
    blit_surface_rect(cr, &pledit, 153, title_y, width - 25, 0, 25, 20)?;

    for i in 0..(height - 58) / 29 {
        let ydest = (i * 29) + 20;
        blit_surface_rect(cr, &pledit, 0, 42, 0, ydest, 12, 29)?;
        blit_surface_rect(cr, &pledit, 32, 42, width - 19, ydest, 19, 29)?;
    }

    blit_surface_rect(cr, &pledit, 0, 72, 0, height - 38, 125, 38)?;

    count = (width - PLAYLIST_MIN_WIDTH) / PLAYLIST_WIDTH_STEP;
    if count >= 3 {
        count -= 3;
        blit_surface_rect(cr, &pledit, 205, 0, width - 225, height - 38, 75, 38)?;
    }
    for i in 0..count {
        blit_surface_rect(
            cr,
            &pledit,
            179,
            0,
            (i * PLAYLIST_WIDTH_STEP) + 125,
            height - 38,
            PLAYLIST_WIDTH_STEP,
            38,
        )?;
    }
    blit_surface_rect(cr, &pledit, 126, 72, width - 150, height - 38, 150, 38)?;

    draw_playlist_footer_info(cr, skin, width, height, footer_info.unwrap_or(""))?;
    draw_playlist_footer_time(
        cr,
        skin,
        width,
        height,
        footer_time_min.unwrap_or("   "),
        footer_time_sec.unwrap_or("  "),
    )?;

    Ok(true)
}

pub fn render_playlist_rows(
    cr: &Context,
    skin: &DefaultSkin,
    state: &PlaylistRowsRenderState,
    pass: RenderPass,
) -> Result<bool, RenderError> {
    let width = state.width.max(PLAYLIST_MIN_WIDTH);
    let height = state.height.max(PLAYLIST_MIN_HEIGHT);
    let list_x = 12;
    let list_y = 20;
    let list_w = width - 31;
    let list_h = height - 58;
    let entry_h = 11;
    let colors = skin.playlist_colors();

    if pass.is_bitmap() {
        set_rgb(cr, colors.normal_bg);
        cr.rectangle(
            f64::from(list_x),
            f64::from(list_y),
            f64::from(list_w),
            f64::from(list_h),
        );
        cr.fill()?;
    }

    let visible = (list_h / entry_h).max(0) as usize;
    let baseline = if pass.is_text() {
        set_playlist_font(cr, &state.font_family);
        cr.font_extents()?.ascent().ceil() as i32
    } else {
        0
    };
    for row in 0..visible {
        let Some(entry) = state.entries.get(row + state.scroll_offset) else {
            break;
        };
        let y = list_y + row as i32 * entry_h;
        if pass.is_bitmap() {
            if entry.selected {
                set_rgb(cr, colors.selected_bg);
                cr.rectangle(
                    f64::from(list_x),
                    f64::from(y),
                    f64::from(list_w),
                    f64::from(entry_h),
                );
                cr.fill()?;
            }
            continue;
        }

        if entry.current {
            set_rgb(cr, colors.current);
        } else {
            set_rgb(cr, colors.normal);
        }

        let mut text_w = list_w;
        if entry.length_ms > 0 {
            let duration = format_duration(entry.length_ms);
            let extents = cr.text_extents(&duration)?;
            cr.move_to(
                f64::from(list_x + list_w) - extents.width() - 2.0,
                f64::from(y + baseline),
            );
            cr.show_text(&duration)?;
            text_w = (f64::from(list_w) - extents.width() - 5.0).max(1.0) as i32;
        }

        let mut title = normalize_playlist_text(&entry.title);
        if state.show_numbers {
            title = format!("{}. {title}", row + state.scroll_offset + 1);
        }
        let title = ellipsize_text(cr, title, text_w)?;
        cr.move_to(f64::from(list_x), f64::from(y + baseline));
        cr.show_text(&title)?;
    }

    if pass.is_bitmap() {
        if let Some((thumb_y, thumb_h)) =
            playlist_scrollbar_geometry(state.entries.len(), visible, state.scroll_offset, height)
        {
            if let Some(pledit) = skin.get(SkinPixmapKind::PlEdit) {
                let pledit = surface_from_xpm(pledit)?;
                blit_surface_rect(
                    cr,
                    &pledit,
                    if state.scrollbar_dragging { 52 } else { 61 },
                    53,
                    width - 15,
                    thumb_y,
                    8,
                    thumb_h,
                )?;
            }
        }
    }

    if let Some(query) = state.search_query.as_ref() {
        draw_playlist_search(cr, colors, query, width, height, pass)?;
    }

    Ok(true)
}

fn draw_playlist_search(
    cr: &Context,
    colors: crate::skin::PlaylistColors,
    query: &str,
    width: i32,
    height: i32,
    pass: RenderPass,
) -> Result<(), RenderError> {
    let x = 12;
    let y = height - 48;
    let w = width - 31;
    let h = 14;

    if pass.is_bitmap() {
        set_rgb(cr, colors.normal_bg);
        cr.rectangle(f64::from(x), f64::from(y), f64::from(w), f64::from(h));
        cr.fill()?;

        set_rgb(cr, colors.selected_bg);
        cr.rectangle(
            f64::from(x) + 0.5,
            f64::from(y) + 0.5,
            f64::from(w - 1),
            f64::from(h - 1),
        );
        cr.stroke()?;
        return Ok(());
    }

    set_playlist_font(cr, "Helvetica");
    set_rgb(cr, colors.normal);
    let display = ellipsize_text(cr, format!("/{query}"), w - 6)?;
    let extents = cr.text_extents(&display)?;
    cr.move_to(
        f64::from(x + 3),
        f64::from(y) + (f64::from(h) - extents.height()) / 2.0 - extents.y_bearing(),
    );
    cr.show_text(&display)?;
    Ok(())
}

fn playlist_scrollbar_geometry(
    total_entries: usize,
    visible_entries: usize,
    scroll_offset: usize,
    height: i32,
) -> Option<(i32, i32)> {
    if total_entries <= visible_entries || visible_entries == 0 {
        return None;
    }
    let list_y = 20;
    let list_h = height.max(PLAYLIST_MIN_HEIGHT) - PLAYLIST_HEIGHT_BASE;
    let thumb_h = 18;
    let max_scroll = total_entries - visible_entries;
    let max_thumb_pos = (list_h - thumb_h).max(0);
    let thumb_y = list_y
        + ((scroll_offset.min(max_scroll) as i32 * max_thumb_pos) / max_scroll.max(1) as i32);
    Some((thumb_y, thumb_h))
}

fn draw_playlist_shaded_info(
    cr: &Context,
    skin: &DefaultSkin,
    width: i32,
    text: &str,
) -> Result<(), RenderError> {
    render_text(cr, skin, text, 4, 4, width - 35)
}

fn draw_playlist_footer_info(
    cr: &Context,
    skin: &DefaultSkin,
    width: i32,
    height: i32,
    text: &str,
) -> Result<(), RenderError> {
    render_text(cr, skin, text, width - 143, height - 28, 85)
}

fn draw_playlist_footer_time(
    cr: &Context,
    skin: &DefaultSkin,
    width: i32,
    height: i32,
    minutes: &str,
    seconds: &str,
) -> Result<(), RenderError> {
    render_text(cr, skin, minutes, width - 82, height - 15, 15)?;
    render_text(cr, skin, seconds, width - 64, height - 15, 10)
}

fn set_playlist_font(cr: &Context, family: &str) {
    cr.select_font_face(
        if family.trim().is_empty() {
            "Helvetica"
        } else {
            family.trim()
        },
        cairo::FontSlant::Normal,
        cairo::FontWeight::Bold,
    );
    cr.set_font_size(9.0);
}

fn format_duration(length_ms: i64) -> String {
    let total_seconds = (length_ms / 1000).max(0);
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn normalize_playlist_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

fn ellipsize_text(cr: &Context, mut text: String, max_width: i32) -> Result<String, RenderError> {
    if max_width <= 0 {
        return Ok(String::new());
    }
    if cr.text_extents(&text)?.width() <= f64::from(max_width) {
        return Ok(text);
    }
    while text.chars().count() > 4 {
        let mut truncated = text.chars().collect::<Vec<_>>();
        truncated.truncate(truncated.len() - 1);
        text = truncated.into_iter().collect();
        let candidate = format!("{text}...");
        if cr.text_extents(&candidate)?.width() <= f64::from(max_width) {
            return Ok(candidate);
        }
    }
    Ok(text)
}

pub fn render_playlist_menu(
    cr: &Context,
    skin: &DefaultSkin,
    state: PlaylistMenuRenderState,
) -> Result<bool, RenderError> {
    let Some(pledit) = skin.get(SkinPixmapKind::PlEdit) else {
        return Ok(false);
    };
    let pledit = surface_from_xpm(pledit)?;
    let items = playlist_menu_items(state.kind);
    for (idx, item) in items.iter().enumerate() {
        let selected = state.hover == Some(idx);
        let (src_x, src_y) = if selected {
            (item.selected_x, item.selected_y)
        } else {
            (item.normal_x, item.normal_y)
        };
        blit_surface_rect(cr, &pledit, src_x, src_y, 3, idx as i32 * 18, 22, 18)?;
    }
    let (border_x, border_y) = playlist_menu_border_source(state.kind);
    blit_surface_rect(
        cr,
        &pledit,
        border_x,
        border_y,
        0,
        0,
        3,
        items.len() as i32 * 18,
    )
}

#[derive(Debug, Clone, Copy)]
struct PlaylistMenuItemSource {
    normal_x: i32,
    normal_y: i32,
    selected_x: i32,
    selected_y: i32,
}

fn playlist_menu_items(kind: PlaylistMenuRenderKind) -> &'static [PlaylistMenuItemSource] {
    match kind {
        PlaylistMenuRenderKind::Add => &[
            PlaylistMenuItemSource {
                normal_x: 0,
                normal_y: 111,
                selected_x: 23,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 0,
                normal_y: 130,
                selected_x: 23,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 0,
                normal_y: 149,
                selected_x: 23,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::Remove => &[
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 168,
                selected_x: 77,
                selected_y: 168,
            },
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 111,
                selected_x: 77,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 130,
                selected_x: 77,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 149,
                selected_x: 77,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::Select => &[
            PlaylistMenuItemSource {
                normal_x: 104,
                normal_y: 111,
                selected_x: 127,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 104,
                normal_y: 130,
                selected_x: 127,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 104,
                normal_y: 149,
                selected_x: 127,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::Misc => &[
            PlaylistMenuItemSource {
                normal_x: 154,
                normal_y: 111,
                selected_x: 177,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 154,
                normal_y: 130,
                selected_x: 177,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 154,
                normal_y: 149,
                selected_x: 177,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::List => &[
            PlaylistMenuItemSource {
                normal_x: 204,
                normal_y: 111,
                selected_x: 227,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 204,
                normal_y: 130,
                selected_x: 227,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 204,
                normal_y: 149,
                selected_x: 227,
                selected_y: 149,
            },
        ],
    }
}

fn playlist_menu_border_source(kind: PlaylistMenuRenderKind) -> (i32, i32) {
    match kind {
        PlaylistMenuRenderKind::Add => (48, 111),
        PlaylistMenuRenderKind::Remove => (100, 111),
        PlaylistMenuRenderKind::Select => (150, 111),
        PlaylistMenuRenderKind::Misc => (200, 111),
        PlaylistMenuRenderKind::List => (250, 111),
    }
}
