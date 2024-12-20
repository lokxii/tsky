use ratatui::{
    layout::Position,
    widgets::{StatefulWidget, Widget},
};

#[derive(Clone)]
pub struct ListState {
    pub selected: Option<usize>,
    selected_y: Option<i32>,
    height: u16,
    prev_height: u16,
}

impl Default for ListState {
    fn default() -> Self {
        ListState {
            selected: None,
            selected_y: None,
            height: 0,
            prev_height: 0,
        }
    }
}

impl ListState {
    pub fn select(&mut self, i: Option<usize>) {
        self.selected = i;
        if let None = self.selected_y {
            self.selected_y = Some(0);
        }
    }

    pub fn next(&mut self) {
        self.selected.as_mut().map(|i| *i += 1);
        self.selected_y.as_mut().map(|y| *y += self.height as i32);
    }

    pub fn previous(&mut self) {
        self.selected.as_mut().map(|i| {
            if *i > 0 {
                *i -= 1
            }
        });
        self.selected_y.as_mut().map(|y| {
            *y -= self.prev_height as i32;
            if *y < 0 {
                *y = 0
            }
        });
    }
}

pub struct ListContext {
    pub index: usize,
    pub is_selected: bool,
}

pub struct List<T, F>
where
    T: Widget,
    F: Fn(ListContext) -> (T, u16),
{
    len: usize,
    f: F,
}

impl<T, F> List<T, F>
where
    T: Widget,
    F: Fn(ListContext) -> (T, u16),
{
    pub fn new(len: usize, f: F) -> Self {
        List { len, f }
    }
}

impl<T, F> StatefulWidget for List<T, F>
where
    T: Widget,
    F: Fn(ListContext) -> (T, u16),
{
    type State = ListState;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        if state.selected.is_some() {
            if self.len == 0 {
                state.selected = None;
                state.selected_y = None;
            } else if state.selected.unwrap() >= self.len {
                state.select(Some(self.len - 1));
                state.selected_y.as_mut().map(|y| *y -= state.height as i32);
            }
        }
        if self.len == 0 {
            return;
        }

        let mut i = state.selected.unwrap_or(0) as i32;
        let mut y = state.selected_y.unwrap_or(0);
        let mut bottom_y = 0;
        let mut first = true;

        while i >= 0 {
            let (item, height) = (self.f)(ListContext {
                index: i as usize,
                is_selected: state
                    .selected
                    .map(|s| i == s as i32)
                    .unwrap_or(false),
            });

            if first {
                state.height = height;
                if i > 0 {
                    let (_, h) = (self.f)(ListContext {
                        index: i as usize - 1,
                        is_selected: false,
                    });
                    state.prev_height = h;
                }
                bottom_y = y as u16 + height;
                if bottom_y > area.height {
                    y = if area.height >= height {
                        (area.height - height) as i32
                    } else {
                        0
                    };
                    state.selected_y = Some(y);
                }
                first = false;
            } else {
                y -= height as i32;
            }

            render_truncated(
                item,
                SignedRect {
                    x: area.left() as i32,
                    y: area.top() as i32 + y,
                    width: area.width,
                    height,
                },
                area,
                buf,
            );
            i -= 1;
        }

        let mut i = state.selected.map(|i| i + 1).unwrap_or(1);
        let mut y = bottom_y;
        while i < self.len && y < area.height {
            let (item, height) =
                (self.f)(ListContext { index: i as usize, is_selected: false });

            render_truncated(
                item,
                SignedRect {
                    x: area.left() as i32,
                    y: (area.top() + y) as i32,
                    width: area.width,
                    height,
                },
                area,
                buf,
            );
            i += 1;
            y += height;
        }
    }
}

#[derive(Clone, Copy)]
struct SignedRect {
    x: i32,
    y: i32,
    width: u16,
    height: u16,
}

fn render_truncated<T>(
    widget: T,
    widget_area: SignedRect,
    available_area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
) where
    T: Widget,
{
    let mut internal_buf =
        ratatui::buffer::Buffer::empty(ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: widget_area.width,
            height: widget_area.height,
        });
    widget.render(internal_buf.area, &mut internal_buf);

    for y in widget_area.y..widget_area.y + widget_area.height as i32 {
        for x in widget_area.x..widget_area.x + widget_area.width as i32 {
            if !(y as u16 >= available_area.top()
                && (y as u16) < available_area.bottom()
                && x as u16 >= available_area.left()
                && (x as u16) < available_area.right())
            {
                continue;
            }
            if let Some(to) = buf.cell_mut(Position::new(x as u16, y as u16)) {
                if let Some(from) = internal_buf.cell(Position::new(
                    (x - widget_area.x) as u16,
                    (y - widget_area.y) as u16,
                )) {
                    *to = from.clone();
                }
            }
        }
    }
}
