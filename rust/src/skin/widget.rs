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
}
