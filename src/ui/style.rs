use crate::skin::{DefaultSkin, PlaylistColors};

const XMMS_MENU_ROOT_SELECTORS: &[&str] = &[
    ".xmms-menu-popover",
    ".xmms-menu-popover contents",
    ".xmms-menu-box",
];
const XMMS_MENU_BUTTON_SELECTORS: &[&str] =
    &[".xmms-menu-button", ".xmms-menu-popover modelbutton"];
const XMMS_MENU_BUTTON_ACTIVE_SELECTORS: &[&str] = &[
    ".xmms-menu-button:hover",
    ".xmms-menu-button:active",
    ".xmms-menu-popover modelbutton:hover",
];

const SKINNED_WINDOW_SURFACE_SELECTORS: &[&str] = &[
    ".xmms-skinned-window",
    ".xmms-skinned-window box",
    ".xmms-skinned-window frame",
    ".xmms-skinned-window notebook",
    ".xmms-skinned-window notebook > stack",
    ".xmms-skinned-window scrolledwindow",
    ".xmms-skinned-window viewport",
    ".xmms-skinned-window list",
    ".xmms-skinned-window listbox",
    ".xmms-skinned-window row",
    ".xmms-skinned-window textview",
    ".xmms-skinned-window textview text",
];
const SKINNED_WINDOW_TEXT_SELECTORS: &[&str] = &[
    ".xmms-skinned-window label",
    ".xmms-skinned-window checkbutton",
    ".xmms-skinned-window radiobutton",
];
const SKINNED_WINDOW_DECORATION_SELECTORS: &[&str] = &[
    "window.xmms-skinned-window decoration",
    "window.xmms-skinned-window decoration:backdrop",
    "window.xmms-skinned-window .titlebar",
    "window.xmms-skinned-window .titlebar:backdrop",
    "window.xmms-skinned-window .default-decoration",
    "window.xmms-skinned-window .default-decoration:backdrop",
    "window.xmms-skinned-window titlebar",
    "window.xmms-skinned-window titlebar:backdrop",
    "window.xmms-skinned-window headerbar",
    "window.xmms-skinned-window headerbar:backdrop",
    "window.xmms-skinned-window windowhandle",
    "window.xmms-skinned-window windowhandle:backdrop",
];
const SKINNED_WINDOW_FRAME_SELECTORS: &[&str] = &[
    "window.xmms-skinned-window",
    "window.xmms-skinned-window:backdrop",
    "window.xmms-skinned-window.csd",
    "window.xmms-skinned-window.csd:backdrop",
];
const SKINNED_WINDOW_CONTENT_SELECTORS: &[&str] = &[
    "window.xmms-skinned-window contents",
    "window.xmms-skinned-window contents:backdrop",
];
const SKINNED_WINDOW_TITLEBAR_BORDER_SELECTORS: &[&str] = &[
    "window.xmms-skinned-window headerbar.xmms-skinned-window-titlebar",
    "window.xmms-skinned-window headerbar.xmms-skinned-window-titlebar:backdrop",
    "window.xmms-skinned-window .xmms-skinned-window-titlebar",
    "window.xmms-skinned-window .xmms-skinned-window-titlebar:backdrop",
];
const SKINNED_WINDOW_CONTROL_SELECTORS: &[&str] = &[
    ".xmms-skinned-window entry",
    ".xmms-skinned-window spinbutton",
    ".xmms-skinned-window textview",
    ".xmms-skinned-window textview text",
    ".xmms-skinned-window combobox",
    ".xmms-skinned-window button",
    ".xmms-skinned-window list",
    ".xmms-skinned-window listbox",
    ".xmms-skinned-window row",
];
const SKINNED_WINDOW_BORDERED_SELECTORS: &[&str] = &[
    ".xmms-skinned-window entry",
    ".xmms-skinned-window spinbutton",
    ".xmms-skinned-window textview",
    ".xmms-skinned-window button",
    ".xmms-skinned-window list",
    ".xmms-skinned-window listbox",
];
const SKINNED_WINDOW_ACTIVE_SELECTORS: &[&str] = &[
    ".xmms-skinned-window entry selection",
    ".xmms-skinned-window textview text selection",
    ".xmms-skinned-window button:hover",
    ".xmms-skinned-window button:active",
    ".xmms-skinned-window row:selected",
    ".xmms-skinned-window row:hover",
    ".xmms-skinned-window notebook > header > tabs > tab:checked",
];
const SKINNED_WINDOW_CHECK_SELECTORS: &[&str] = &[
    ".xmms-skinned-window checkbutton check",
    ".xmms-skinned-window radiobutton radio",
];
const SKINNED_WINDOW_CHECKED_SELECTORS: &[&str] = &[
    ".xmms-skinned-window checkbutton check:checked",
    ".xmms-skinned-window radiobutton radio:checked",
];
const SKINNED_WINDOW_SCROLLBAR_SELECTORS: &[&str] = &[
    ".xmms-skinned-window scrollbar",
    ".xmms-skinned-window scrollbar trough",
];
const SKINNED_WINDOW_DISABLED_SELECTORS: &[&str] = &[
    ".xmms-skinned-window button:disabled",
    ".xmms-skinned-window entry:disabled",
    ".xmms-skinned-window spinbutton:disabled",
];
const SKINNED_WINDOW_TITLE_SELECTORS: &[&str] = &[".xmms-skinned-window-title"];

#[derive(Debug, Clone)]
pub(super) struct SkinStyle {
    pub(super) window_bg: String,
    pub(super) text_normal: String,
    pub(super) text_current: String,
    pub(super) selection_bg: String,
    pub(super) control_border: String,
    pub(super) window_border_line: String,
    pub(super) window_border_radius: &'static str,
    pub(super) disabled_opacity: &'static str,
    pub(super) titlebar_font_weight: &'static str,
}

impl SkinStyle {
    pub(super) fn from_playlist_colors(colors: PlaylistColors) -> Self {
        let window_border_color = inverted_css_rgb(colors.normal_bg);
        let selection_bg = css_rgb(colors.selected_bg);
        let control_border = format!("1px solid {selection_bg}");
        let window_border_line = format!("1px solid {window_border_color}");
        Self {
            window_bg: css_rgb(colors.normal_bg),
            text_normal: css_rgb(colors.normal),
            text_current: css_rgb(colors.current),
            selection_bg,
            control_border,
            window_border_line,
            window_border_radius: "4px",
            disabled_opacity: "0.45",
            titlebar_font_weight: "bold",
        }
    }
}

pub(super) fn xmms_menu_css(skin: &DefaultSkin) -> String {
    let style = SkinStyle::from_playlist_colors(skin.playlist_colors());
    let mut css = String::new();

    append_css_rule(
        &mut css,
        XMMS_MENU_ROOT_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("color", style.text_normal.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        XMMS_MENU_BUTTON_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("background-image", "none"),
            ("border", "0"),
            ("border-radius", "0"),
            ("box-shadow", "none"),
            ("color", style.text_normal.as_str()),
            ("padding", "1px 12px"),
            ("min-height", "0"),
            ("text-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        XMMS_MENU_BUTTON_ACTIVE_SELECTORS,
        &[
            ("background", style.selection_bg.as_str()),
            ("color", style.text_current.as_str()),
        ],
    );
    css
}

pub(super) fn xmms_window_css(skin: &DefaultSkin) -> String {
    let style = SkinStyle::from_playlist_colors(skin.playlist_colors());
    let mut css = String::new();

    append_css_rule(
        &mut css,
        SKINNED_WINDOW_SURFACE_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("color", style.text_normal.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_TEXT_SELECTORS,
        &[("color", style.text_normal.as_str())],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_DECORATION_SELECTORS,
        &[
            ("border", "0"),
            ("border-radius", "0"),
            ("box-shadow", "none"),
            ("outline", "0"),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_FRAME_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("border", style.window_border_line.as_str()),
            ("border-radius", style.window_border_radius),
            ("box-shadow", "none"),
            ("outline", "0"),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_CONTENT_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("border", "0"),
            ("border-top-left-radius", "0"),
            ("border-top-right-radius", "0"),
            ("border-bottom-left-radius", style.window_border_radius),
            ("border-bottom-right-radius", style.window_border_radius),
            ("box-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_TITLEBAR_BORDER_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("border", "0"),
            ("border-bottom", style.window_border_line.as_str()),
            ("border-top-left-radius", style.window_border_radius),
            ("border-top-right-radius", style.window_border_radius),
            ("border-bottom-left-radius", "0"),
            ("border-bottom-right-radius", "0"),
            ("box-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_CONTROL_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("background-image", "none"),
            ("border-color", style.selection_bg.as_str()),
            ("border-radius", "0"),
            ("box-shadow", "none"),
            ("color", style.text_current.as_str()),
            ("text-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_BORDERED_SELECTORS,
        &[("border", style.control_border.as_str())],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_ACTIVE_SELECTORS,
        &[
            ("background", style.selection_bg.as_str()),
            ("color", style.text_current.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        &[".xmms-skinned-window notebook > header"],
        &[
            ("background", style.window_bg.as_str()),
            ("border-color", style.selection_bg.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        &[".xmms-skinned-window notebook > header > tabs > tab"],
        &[
            ("background", style.window_bg.as_str()),
            ("border", style.control_border.as_str()),
            ("color", style.text_normal.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_CHECK_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("border", style.control_border.as_str()),
            ("border-radius", "0"),
            ("box-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_CHECKED_SELECTORS,
        &[
            ("background", style.selection_bg.as_str()),
            ("color", style.text_current.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        &[".xmms-skinned-window separator"],
        &[
            ("background", style.selection_bg.as_str()),
            ("color", style.selection_bg.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_SCROLLBAR_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("border-color", style.selection_bg.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        &[".xmms-skinned-window scrollbar slider"],
        &[
            ("background", style.selection_bg.as_str()),
            ("border-radius", "0"),
        ],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_DISABLED_SELECTORS,
        &[("opacity", style.disabled_opacity)],
    );
    append_css_rule(
        &mut css,
        SKINNED_WINDOW_TITLE_SELECTORS,
        &[("font-weight", style.titlebar_font_weight)],
    );
    css
}

pub(super) fn append_css_rule(css: &mut String, selectors: &[&str], declarations: &[(&str, &str)]) {
    append_css_rule_groups(css, &[selectors], declarations);
}

pub(super) fn append_css_rule_groups(
    css: &mut String,
    selector_groups: &[&[&str]],
    declarations: &[(&str, &str)],
) {
    if !css.is_empty() {
        css.push('\n');
    }
    let mut first = true;
    for selectors in selector_groups {
        for selector in *selectors {
            if !first {
                css.push_str(",\n");
            }
            css.push_str(selector);
            first = false;
        }
    }
    css.push_str(" {\n");
    for (property, value) in declarations {
        css.push_str("    ");
        css.push_str(property);
        css.push_str(": ");
        css.push_str(value);
        css.push_str(";\n");
    }
    css.push_str("}\n");
}

pub(super) fn css_rgb(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

fn inverted_css_rgb(color: [u8; 3]) -> String {
    css_rgb([255 - color[0], 255 - color[1], 255 - color[2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmms_menu_css_uses_playlist_skin_colors() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let colors = skin.playlist_colors();
        let css = xmms_menu_css(&skin);

        assert!(css.contains(&format!(
            "background: #{:02x}{:02x}{:02x}",
            colors.normal_bg[0], colors.normal_bg[1], colors.normal_bg[2]
        )));
        assert!(css.contains(&format!(
            "color: #{:02x}{:02x}{:02x}",
            colors.normal[0], colors.normal[1], colors.normal[2]
        )));
        assert!(css.contains(&format!(
            "background: #{:02x}{:02x}{:02x}",
            colors.selected_bg[0], colors.selected_bg[1], colors.selected_bg[2]
        )));
        assert!(css.contains(&format!(
            "color: #{:02x}{:02x}{:02x}",
            colors.current[0], colors.current[1], colors.current[2]
        )));
    }

    #[test]
    fn xmms_window_css_uses_inverted_outer_window_border() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let colors = skin.playlist_colors();
        let css = xmms_window_css(&skin);
        let inverted_bg = [
            255 - colors.normal_bg[0],
            255 - colors.normal_bg[1],
            255 - colors.normal_bg[2],
        ];

        assert!(css.contains(".xmms-skinned-window"));
        assert!(css.contains(&format!(
            "background: #{:02x}{:02x}{:02x}",
            colors.normal_bg[0], colors.normal_bg[1], colors.normal_bg[2]
        )));
        assert!(css.contains(&format!(
            "color: #{:02x}{:02x}{:02x}",
            colors.normal[0], colors.normal[1], colors.normal[2]
        )));
        assert!(css.contains(&format!(
            "border: 1px solid #{:02x}{:02x}{:02x}",
            colors.selected_bg[0], colors.selected_bg[1], colors.selected_bg[2]
        )));
        assert!(css.contains("window.xmms-skinned-window contents"));
        assert!(css.contains("window.xmms-skinned-window.csd"));
        assert!(css.contains("border-radius: 4px"));
        assert!(css.contains("border-top-left-radius: 4px"));
        assert!(css.contains("border-bottom-left-radius: 4px"));
        assert!(css.contains("window.xmms-skinned-window headerbar.xmms-skinned-window-titlebar"));
        assert!(css.contains(&format!(
            "border-bottom: 1px solid #{:02x}{:02x}{:02x}",
            inverted_bg[0], inverted_bg[1], inverted_bg[2]
        )));
        assert!(css.contains(".xmms-skinned-window-title"));
        assert!(css.contains("font-weight: bold"));
        assert!(css.contains(&format!(
            "color: #{:02x}{:02x}{:02x}",
            colors.current[0], colors.current[1], colors.current[2]
        )));
    }
}
