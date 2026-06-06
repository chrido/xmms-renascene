use super::SkinPixmapKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WidgetRect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
            && self.width > 0
            && self.height > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    id: WidgetId,
    rect: WidgetRect,
    visible: bool,
    redraw: bool,
}

impl Widget {
    pub fn new(id: WidgetId, rect: WidgetRect) -> Self {
        Self {
            id,
            rect,
            visible: true,
            redraw: false,
        }
    }

    pub fn id(&self) -> WidgetId {
        self.id
    }

    pub fn rect(&self) -> WidgetRect {
        self.rect
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn needs_redraw(&self) -> bool {
        self.redraw
    }

    pub fn queue_draw(&mut self) {
        self.redraw = true;
    }

    pub fn clear_redraw(&mut self) {
        self.redraw = false;
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.visible && self.rect.contains(x, y)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WidgetList {
    widgets: Vec<Widget>,
    next_id: usize,
}

impl WidgetList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, rect: WidgetRect) -> WidgetId {
        let id = WidgetId(self.next_id);
        self.next_id += 1;
        self.widgets.push(Widget::new(id, rect));
        id
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Widget> {
        self.widgets.iter()
    }

    pub fn iter_visible(&self) -> impl Iterator<Item = &Widget> {
        self.widgets.iter().filter(|widget| widget.is_visible())
    }

    pub fn get(&self, id: WidgetId) -> Option<&Widget> {
        self.widgets.iter().find(|widget| widget.id == id)
    }

    pub fn get_mut(&mut self, id: WidgetId) -> Option<&mut Widget> {
        self.widgets.iter_mut().find(|widget| widget.id == id)
    }

    pub fn find_at(&self, x: i32, y: i32) -> Option<&Widget> {
        self.widgets
            .iter()
            .rev()
            .find(|widget| widget.contains(x, y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinSource {
    pub kind: SkinPixmapKind,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushButton {
    widget: Widget,
    normal: SkinSource,
    pressed_source: SkinSource,
    pressed: bool,
    inside: bool,
    allow_draw: bool,
}

impl PushButton {
    pub fn new(
        id: WidgetId,
        rect: WidgetRect,
        normal: SkinSource,
        pressed_source: SkinSource,
    ) -> Self {
        Self {
            widget: Widget::new(id, rect),
            normal,
            pressed_source,
            pressed: false,
            inside: false,
            allow_draw: true,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    pub fn is_inside(&self) -> bool {
        self.inside
    }

    pub fn allow_draw(&self) -> bool {
        self.allow_draw
    }

    pub fn set_allow_draw(&mut self, allow_draw: bool) {
        if self.allow_draw != allow_draw {
            self.allow_draw = allow_draw;
            self.widget.queue_draw();
        }
    }

    pub fn current_source(&self) -> Option<SkinSource> {
        if !self.allow_draw {
            return None;
        }
        if self.pressed && self.inside {
            Some(self.pressed_source)
        } else {
            Some(self.normal)
        }
    }

    pub fn press(&mut self, button: u32) {
        if button != 1 {
            return;
        }
        self.pressed = true;
        self.inside = true;
        self.widget.queue_draw();
    }

    pub fn release(&mut self, button: u32) -> bool {
        if button != 1 {
            return false;
        }
        let activated = self.pressed && self.inside;
        self.pressed = false;
        self.widget.queue_draw();
        activated
    }

    pub fn motion(&mut self, x: i32, y: i32) {
        if !self.pressed {
            return;
        }
        let was_inside = self.inside;
        self.inside = self.widget.rect().contains(x, y);
        if was_inside != self.inside {
            self.widget.queue_draw();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleButtonSources {
    pub normal_unselected: SkinSource,
    pub pressed_unselected: SkinSource,
    pub normal_selected: SkinSource,
    pub pressed_selected: SkinSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleButton {
    widget: Widget,
    sources: ToggleButtonSources,
    selected: bool,
    pressed: bool,
    inside: bool,
}

impl ToggleButton {
    pub fn new(id: WidgetId, rect: WidgetRect, sources: ToggleButtonSources) -> Self {
        Self {
            widget: Widget::new(id, rect),
            sources,
            selected: false,
            pressed: false,
            inside: false,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    pub fn is_inside(&self) -> bool {
        self.inside
    }

    pub fn set_selected(&mut self, selected: bool) {
        if self.selected != selected {
            self.selected = selected;
            self.widget.queue_draw();
        }
    }

    pub fn current_source(&self) -> SkinSource {
        match (self.selected, self.pressed && self.inside) {
            (true, true) => self.sources.pressed_selected,
            (true, false) => self.sources.normal_selected,
            (false, true) => self.sources.pressed_unselected,
            (false, false) => self.sources.normal_unselected,
        }
    }

    pub fn press(&mut self, button: u32) {
        if button != 1 {
            return;
        }
        self.pressed = true;
        self.inside = true;
        self.widget.queue_draw();
    }

    pub fn release(&mut self, button: u32) -> Option<bool> {
        if button != 1 {
            return None;
        }
        let activated = self.pressed && self.inside;
        if activated {
            self.selected = !self.selected;
        }
        self.pressed = false;
        self.widget.queue_draw();

        activated.then_some(self.selected)
    }

    pub fn motion(&mut self, x: i32, y: i32) {
        if !self.pressed {
            return;
        }
        let was_inside = self.inside;
        self.inside = self.widget.rect().contains(x, y);
        if was_inside != self.inside {
            self.widget.queue_draw();
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisMode {
    Analyzer = 0,
    Scope = 1,
    Off = 2,
    Milkdrop = 3,
}

impl VisMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Scope,
            2 => Self::Off,
            3 => Self::Milkdrop,
            _ => Self::Analyzer,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerStyle {
    Bars = 0,
    Lines = 1,
}

impl VisAnalyzerStyle {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Lines,
            _ => Self::Bars,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerMode {
    Normal = 0,
    Fire = 1,
    VerticalLines = 2,
}

impl VisAnalyzerMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Fire,
            2 => Self::VerticalLines,
            _ => Self::Normal,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisScopeMode {
    Dot = 0,
    Line = 1,
    Solid = 2,
}

impl VisScopeMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Dot,
            2 => Self::Solid,
            _ => Self::Line,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisFalloffSpeed {
    Slowest = 0,
    Slow = 1,
    Medium = 2,
    Fast = 3,
    Fastest = 4,
}

impl VisFalloffSpeed {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Slowest,
            1 => Self::Slow,
            3 => Self::Fast,
            4 => Self::Fastest,
            _ => Self::Medium,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisVuMode {
    Normal = 0,
    Smooth = 1,
}

impl VisVuMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Smooth,
            _ => Self::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_rect_contains_matches_c_exclusive_edges() {
        let rect = WidgetRect {
            x: 10,
            y: 20,
            width: 5,
            height: 3,
        };

        assert!(rect.contains(10, 20));
        assert!(rect.contains(14, 22));
        assert!(!rect.contains(15, 22));
        assert!(!rect.contains(14, 23));
        assert!(!rect.contains(9, 20));
    }

    #[test]
    fn widget_list_hit_tests_visible_widgets_in_reverse_order() {
        let mut list = WidgetList::new();
        let bottom = list.add(WidgetRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        });
        let top = list.add(WidgetRect {
            x: 5,
            y: 5,
            width: 10,
            height: 10,
        });

        assert_eq!(list.find_at(6, 6).unwrap().id(), top);
        list.get_mut(top).unwrap().set_visible(false);
        assert_eq!(list.find_at(6, 6).unwrap().id(), bottom);
    }

    #[test]
    fn widget_queue_draw_tracks_redraw_flag() {
        let mut list = WidgetList::new();
        let id = list.add(WidgetRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });

        assert!(!list.get(id).unwrap().needs_redraw());
        list.get_mut(id).unwrap().queue_draw();
        assert!(list.get(id).unwrap().needs_redraw());
        list.get_mut(id).unwrap().clear_redraw();
        assert!(!list.get(id).unwrap().needs_redraw());
    }

    #[test]
    fn push_button_tracks_press_motion_release_like_c() {
        let normal = SkinSource {
            kind: SkinPixmapKind::CButtons,
            x: 0,
            y: 0,
        };
        let pressed = SkinSource {
            kind: SkinPixmapKind::CButtons,
            x: 0,
            y: 18,
        };
        let mut button = PushButton::new(
            WidgetId(1),
            WidgetRect {
                x: 10,
                y: 10,
                width: 5,
                height: 5,
            },
            normal,
            pressed,
        );

        assert_eq!(button.current_source(), Some(normal));
        button.press(2);
        assert!(!button.is_pressed());

        button.press(1);
        assert!(button.is_pressed());
        assert!(button.is_inside());
        assert_eq!(button.current_source(), Some(pressed));

        button.motion(20, 20);
        assert!(!button.is_inside());
        assert_eq!(button.current_source(), Some(normal));
        assert!(!button.release(1));

        button.press(1);
        assert!(button.release(1));
        assert!(!button.is_pressed());
    }

    #[test]
    fn push_button_allow_draw_controls_source_and_queues_redraw() {
        let source = SkinSource {
            kind: SkinPixmapKind::Titlebar,
            x: 0,
            y: 0,
        };
        let mut button = PushButton::new(
            WidgetId(1),
            WidgetRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            source,
            source,
        );

        button.set_allow_draw(false);
        assert!(!button.allow_draw());
        assert!(button.widget().needs_redraw());
        assert_eq!(button.current_source(), None);
    }

    #[test]
    fn toggle_button_toggles_on_left_release_inside() {
        let sources = ToggleButtonSources {
            normal_unselected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 0,
                y: 0,
            },
            pressed_unselected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 0,
                y: 15,
            },
            normal_selected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 28,
                y: 0,
            },
            pressed_selected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 28,
                y: 15,
            },
        };
        let mut button = ToggleButton::new(
            WidgetId(2),
            WidgetRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            sources,
        );

        assert_eq!(button.current_source(), sources.normal_unselected);
        button.press(1);
        assert_eq!(button.current_source(), sources.pressed_unselected);
        assert_eq!(button.release(1), Some(true));
        assert!(button.is_selected());
        assert_eq!(button.current_source(), sources.normal_selected);

        button.press(1);
        assert_eq!(button.current_source(), sources.pressed_selected);
        assert_eq!(button.release(1), Some(false));
        assert!(!button.is_selected());
    }

    #[test]
    fn toggle_button_release_outside_does_not_toggle() {
        let source = SkinSource {
            kind: SkinPixmapKind::ShufRep,
            x: 0,
            y: 0,
        };
        let sources = ToggleButtonSources {
            normal_unselected: source,
            pressed_unselected: source,
            normal_selected: source,
            pressed_selected: source,
        };
        let mut button = ToggleButton::new(
            WidgetId(2),
            WidgetRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            sources,
        );

        button.press(1);
        button.motion(12, 12);
        assert_eq!(button.release(1), None);
        assert!(!button.is_selected());
        button.set_selected(true);
        assert!(button.is_selected());
    }
}
