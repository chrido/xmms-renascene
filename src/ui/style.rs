use std::cell::RefCell;

use gtk::prelude::*;

use crate::skin::{DefaultSkin, PlaylistColors};

thread_local! {
    static XMMS_MENU_CSS_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static XMMS_WINDOW_CSS_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

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

const FILE_INFO_SURFACE_SELECTORS: &[&str] = &[
    ".xmms-file-info",
    ".xmms-file-info box",
    ".xmms-file-info frame",
    "window.xmms-file-info contents",
];
const FILE_INFO_DECORATION_SELECTORS: &[&str] = &[
    "window.xmms-file-info decoration",
    "window.xmms-file-info decoration:backdrop",
    "window.xmms-file-info .titlebar",
    "window.xmms-file-info .titlebar:backdrop",
    "window.xmms-file-info .default-decoration",
    "window.xmms-file-info .default-decoration:backdrop",
    "window.xmms-file-info titlebar",
    "window.xmms-file-info titlebar:backdrop",
    "window.xmms-file-info headerbar",
    "window.xmms-file-info headerbar:backdrop",
    "window.xmms-file-info windowhandle",
    "window.xmms-file-info windowhandle:backdrop",
];
const FILE_INFO_FRAME_SELECTORS: &[&str] = &[
    "window.xmms-file-info",
    "window.xmms-file-info:backdrop",
    "window.xmms-file-info.csd",
    "window.xmms-file-info.csd:backdrop",
];
const FILE_INFO_CONTENT_SELECTORS: &[&str] = &[
    "window.xmms-file-info contents",
    "window.xmms-file-info contents:backdrop",
];
const FILE_INFO_TITLEBAR_BORDER_SELECTORS: &[&str] = &[
    "window.xmms-file-info headerbar.xmms-skinned-window-titlebar",
    "window.xmms-file-info headerbar.xmms-skinned-window-titlebar:backdrop",
    "window.xmms-file-info .xmms-skinned-window-titlebar",
    "window.xmms-file-info .xmms-skinned-window-titlebar:backdrop",
];
const FILE_INFO_OUTLINE_SELECTORS: &[&str] = &[
    "window.xmms-file-info .titlebar",
    "window.xmms-file-info .titlebar:backdrop",
    "window.xmms-file-info .default-decoration",
    "window.xmms-file-info .default-decoration:backdrop",
    "window.xmms-file-info titlebar",
    "window.xmms-file-info titlebar:backdrop",
    "window.xmms-file-info headerbar",
    "window.xmms-file-info headerbar:backdrop",
    "window.xmms-file-info windowhandle",
    "window.xmms-file-info windowhandle:backdrop",
];
const FILE_INFO_SEPARATOR_SELECTORS: &[&str] = &[
    "window.xmms-file-info .titlebar separator",
    "window.xmms-file-info .titlebar separator:backdrop",
    "window.xmms-file-info headerbar separator",
    "window.xmms-file-info headerbar separator:backdrop",
];
const FILE_INFO_CONTROL_SELECTORS: &[&str] = &[".xmms-file-info entry", ".xmms-file-info button"];
const FILE_INFO_NORMAL_TEXT_SELECTORS: &[&str] =
    &[".xmms-file-info label", ".xmms-file-info button"];
const FILE_INFO_CURRENT_TEXT_SELECTORS: &[&str] = &[
    ".xmms-file-info entry",
    ".xmms-file-info entry selection",
    ".xmms-file-info button:hover",
    ".xmms-file-info button:active",
];
const FILE_INFO_ACTIVE_SELECTORS: &[&str] = &[
    ".xmms-file-info entry selection",
    ".xmms-file-info button:hover",
    ".xmms-file-info button:active",
];

#[derive(Debug, Clone)]
struct SkinStyle {
    window_bg: String,
    text_normal: String,
    text_current: String,
    selection_bg: String,
    control_border: String,
    window_border_line: String,
    window_border_radius: &'static str,
    disabled_opacity: &'static str,
    titlebar_font_weight: &'static str,
}

impl SkinStyle {
    fn from_playlist_colors(colors: PlaylistColors) -> Self {
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

pub(super) fn refresh_xmms_skin_css(skin: &DefaultSkin) {
    install_xmms_menu_css(skin);
    install_xmms_window_css(skin);
}

fn install_xmms_menu_css(skin: &DefaultSkin) {
    install_css_provider_slot(&XMMS_MENU_CSS_PROVIDER, &xmms_menu_css(skin));
}

fn install_xmms_window_css(skin: &DefaultSkin) {
    install_css_provider_slot(&XMMS_WINDOW_CSS_PROVIDER, &xmms_window_css(skin));
}

fn install_css_provider_slot(
    slot: &'static std::thread::LocalKey<RefCell<Option<gtk::CssProvider>>>,
    css: &str,
) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    slot.with(|provider| {
        let mut provider = provider.borrow_mut();
        let provider = provider.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            provider
        });
        provider.load_from_data(css);
    });
}

fn xmms_menu_css(skin: &DefaultSkin) -> String {
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

fn xmms_window_css(skin: &DefaultSkin) -> String {
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

pub(super) fn install_file_info_css(colors: PlaylistColors) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&file_info_css(colors));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn file_info_css(colors: PlaylistColors) -> String {
    let style = SkinStyle::from_playlist_colors(colors);
    let mut css = String::new();

    append_css_rule(
        &mut css,
        FILE_INFO_SURFACE_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("color", style.text_normal.as_str()),
        ],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_DECORATION_SELECTORS,
        &[
            ("border", "0"),
            ("border-radius", "0"),
            ("box-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_FRAME_SELECTORS,
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
        FILE_INFO_CONTENT_SELECTORS,
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
        FILE_INFO_TITLEBAR_BORDER_SELECTORS,
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
        FILE_INFO_CONTROL_SELECTORS,
        &[
            ("border", style.control_border.as_str()),
            ("box-shadow", "none"),
        ],
    );
    append_css_rule(&mut css, FILE_INFO_OUTLINE_SELECTORS, &[("outline", "0")]);
    append_css_rule_groups(
        &mut css,
        &[FILE_INFO_SEPARATOR_SELECTORS],
        &[("border", "0")],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_SEPARATOR_SELECTORS,
        &[("background", "transparent"), ("min-height", "0")],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_CONTROL_SELECTORS,
        &[
            ("background", style.window_bg.as_str()),
            ("background-image", "none"),
            ("border-radius", "0"),
            ("text-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_NORMAL_TEXT_SELECTORS,
        &[("color", style.text_normal.as_str())],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_CURRENT_TEXT_SELECTORS,
        &[("color", style.text_current.as_str())],
    );
    append_css_rule(
        &mut css,
        &[".xmms-file-info entry"],
        &[("caret-color", style.text_current.as_str())],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_ACTIVE_SELECTORS,
        &[("background", style.selection_bg.as_str())],
    );
    append_css_rule(
        &mut css,
        &[".xmms-skinned-window-title"],
        &[("font-weight", style.titlebar_font_weight)],
    );
    append_css_rule(
        &mut css,
        &[".xmms-file-info button:disabled"],
        &[("opacity", style.disabled_opacity)],
    );
    css
}

pub(super) fn style_color_shelf_button(button: &gtk::Button, color: Option<[u8; 4]>) {
    let provider = gtk::CssProvider::new();
    if let Some([r, g, b, a]) = color {
        button.set_label("");
        let alpha = f64::from(a) / 255.0;
        provider.load_from_data(&format!(
            "button {{
                background: rgba({r}, {g}, {b}, {alpha:.3});
                background-image: none;
                border: 1px solid #222222;
                border-radius: 3px;
                padding: 0;
                min-width: 0;
                min-height: 0;
            }}
            button:hover {{
                background: rgba({r}, {g}, {b}, {alpha:.3});
                background-image: none;
            }}"
        ));
    } else {
        button.set_label("");
        provider.load_from_data(
            "button {
                background: transparent;
                background-image: none;
                border: 1px dashed #777777;
                border-radius: 3px;
                padding: 0;
                min-width: 0;
                min-height: 0;
            }
            button:hover {
                background: rgba(255, 255, 255, 0.08);
                background-image: none;
            }",
        );
    }
    button
        .style_context()
        .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

pub(super) fn style_skin_editor_custom_color_button(button: &gtk::Button, rgba: [u8; 4]) {
    let [r, g, b, a] = rgba;
    let text = if color_luminance([r, g, b]) > 140.0 && a > 127 {
        "#000000"
    } else {
        "#ffffff"
    };
    let alpha = f64::from(a) / 255.0;
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&format!(
        "button {{
            background: rgba({r}, {g}, {b}, {alpha:.3});
            background-image: none;
            color: {text};
            border: 1px solid #222222;
            border-radius: 3px;
            padding: 4px;
            text-shadow: none;
        }}
        button:hover {{
            background: rgba({r}, {g}, {b}, {alpha:.3});
            background-image: none;
            color: {text};
        }}"
    ));
    button
        .style_context()
        .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

pub(super) fn style_skin_color_button(button: &gtk::Button, rgb: [u8; 3]) {
    let provider = gtk::CssProvider::new();
    let text = if color_luminance(rgb) > 140.0 {
        "#000000"
    } else {
        "#ffffff"
    };
    provider.load_from_data(&format!(
        "button {{
            background: #{:02x}{:02x}{:02x};
            background-image: none;
            color: {text};
            border: 1px solid #222222;
            border-radius: 3px;
            padding: 1px;
            min-width: 0;
            min-height: 0;
            text-shadow: none;
        }}
        button:hover {{
            background: #{:02x}{:02x}{:02x};
            background-image: none;
            color: {text};
        }}",
        rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2]
    ));
    button
        .style_context()
        .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

fn color_luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * f64::from(rgb[0]) + 0.7152 * f64::from(rgb[1]) + 0.0722 * f64::from(rgb[2])
}

fn append_css_rule(css: &mut String, selectors: &[&str], declarations: &[(&str, &str)]) {
    append_css_rule_groups(css, &[selectors], declarations);
}

fn append_css_rule_groups(
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

fn css_rgb(color: [u8; 3]) -> String {
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

    #[test]
    fn file_info_css_uses_playlist_colors_and_inverted_window_borders() {
        let css = file_info_css(PlaylistColors {
            normal: [1, 2, 3],
            current: [4, 5, 6],
            normal_bg: [7, 8, 9],
            selected_bg: [10, 11, 12],
        });
        assert!(css.contains("#010203"));
        assert!(css.contains("#040506"));
        assert!(css.contains("#070809"));
        assert!(css.contains("#0a0b0c"));
        assert!(css.contains("window.xmms-file-info.csd"));
        assert!(css.contains("border-radius: 4px"));
        assert!(css.contains("border-top-left-radius: 4px"));
        assert!(css.contains("border-bottom-left-radius: 4px"));
        assert!(css.contains("window.xmms-file-info headerbar.xmms-skinned-window-titlebar"));
        assert!(css.contains("border-bottom: 1px solid #f8f7f6"));
        assert!(css.contains(".xmms-skinned-window-title"));
        assert!(css.contains("font-weight: bold"));
    }
}
