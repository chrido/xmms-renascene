//! Synchronous app-widget rendering boundary.
//!
//! Android widget providers may run without an Activity and require pixels
//! during the JNI call, so rendering is a deliberate synchronous JNI exception.
//! Skin and marquee caches are process-wide widget caches; durable state is
//! loaded from atomically replaced snapshots on every render.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::app::view_model::TitleMarquee;
use crate::playback::model::PlayerState;
use crate::session::{fallback_state_paths, load_saved_state};
use crate::skin::layout::MainPushButton;
use crate::skin::DefaultSkin;

use super::activity;

static WIDGET_SKIN: OnceLock<Mutex<Option<(Option<String>, DefaultSkin)>>> = OnceLock::new();
static WIDGET_TITLE_MARQUEE: OnceLock<Mutex<TitleMarquee>> = OnceLock::new();

pub fn refresh_player_widgets() -> Result<(), String> {
    *WIDGET_SKIN
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = None;

    let (vm, activity) = activity::reference()?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android widget refresh thread: {err}"))?;
    if let Err(err) = env.call_method(activity.as_obj(), "refreshPlayerWidgets", "()V", &[]) {
        match env.exception_check() {
            Ok(true) => {
                let _ = env.exception_describe();
                env.exception_clear().map_err(|clear_err| {
                    format!(
                        "failed to refresh Android player widgets: {err}; \
                         failed to clear Java exception: {clear_err}"
                    )
                })?;
            }
            Ok(false) => {}
            Err(check_err) => {
                return Err(format!(
                    "failed to refresh Android player widgets: {err}; \
                     failed to check for a Java exception: {check_err}"
                ));
            }
        }
        return Err(format!("failed to refresh Android player widgets: {err}"));
    }
    Ok(())
}

pub(crate) fn render_player_widget(
    files_dir: &Path,
    cache_dir: &Path,
    pressed_control: i32,
) -> Result<egui::ColorImage, String> {
    let pressed = match pressed_control {
        1 => Some(MainPushButton::Pause),
        2 => Some(MainPushButton::Play),
        3 => Some(MainPushButton::Next),
        4 => Some(MainPushButton::Previous),
        6 => Some(MainPushButton::Stop),
        _ => None,
    };
    with_widget_skin(files_dir, cache_dir, |skin| {
        super::super::skin_texture::render_transport_buttons_color_image(skin, pressed)
            .map_err(|err| format!("failed to render widget transport buttons: {err}"))
    })
}

pub(crate) fn render_player_info_widget(
    files_dir: &Path,
    cache_dir: &Path,
    title: &str,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    title_offset_px: i32,
) -> Result<egui::ColorImage, String> {
    let state = super::super::skin_texture::player_info_render_state(
        title,
        bitrate,
        frequency,
        channels,
        title_offset_px,
    );
    with_widget_skin(files_dir, cache_dir, |skin| {
        super::super::skin_texture::render_player_info_color_image(skin, &state)
            .map_err(|err| format!("failed to render widget player information: {err}"))
    })
}

pub(crate) fn update_title_marquee(title: &str, playback_state: i32, elapsed_ms: i64) -> i64 {
    let player_state = match playback_state {
        1 => PlayerState::Playing,
        2 => PlayerState::Paused,
        _ => PlayerState::Stopped,
    };
    let elapsed = Duration::from_millis(elapsed_ms.max(0) as u64);
    let mut marquee = WIDGET_TITLE_MARQUEE
        .get_or_init(|| Mutex::new(TitleMarquee::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let changed = marquee.update(
        title,
        crate::render::MAIN_TITLE_TEXT_WIDTH,
        player_state,
        true,
        elapsed,
    );
    let active = marquee.is_scrolling(player_state, true);
    (i64::from(marquee.offset_px()) << 2) | i64::from(changed) << 1 | i64::from(active)
}

fn with_widget_skin<T>(
    files_dir: &Path,
    cache_dir: &Path,
    render: impl FnOnce(&DefaultSkin) -> Result<T, String>,
) -> Result<T, String> {
    std::env::set_var("XMMS_RS_CONFIG_DIR", files_dir.join("config"));
    std::env::set_var("XMMS_RS_CACHE_DIR", cache_dir);
    let (config_path, playlist_path) = fallback_state_paths(&files_dir.join("config"));
    let app_state =
        load_saved_state(&config_path, &playlist_path, false).map_err(|err| err.to_string())?;
    let skin_key = app_state.config.skin.clone();
    let mut cached_skin = WIDGET_SKIN
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if cached_skin
        .as_ref()
        .is_none_or(|(cached_key, _)| cached_key != &skin_key)
    {
        let skin = match skin_key.as_deref() {
            Some(path) => DefaultSkin::load_from_path(Path::new(path))
                .map_err(|err| format!("failed to load widget skin '{path}': {err}"))?,
            None => DefaultSkin::load_bundled()
                .map_err(|err| format!("failed to load bundled widget skin: {err}"))?,
        };
        *cached_skin = Some((skin_key, skin));
    }
    render(&cached_skin.as_ref().expect("widget skin initialized").1)
}
