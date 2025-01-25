use std::ops::Range;

use ratatui::{
    layout::Position,
    text::Text,
    widgets::{StatefulWidget, Widget},
};

#[derive(Clone, Debug, Default)]
pub struct ListState {
    pub selected: Option<usize>,
    selected_y: i32,
    delta_index: i64,
}

impl ListState {
    pub fn new(selected: Option<usize>) -> Self {
        ListState { selected, selected_y: 0, delta_index: 0 }
    }
}

impl ListState {
    pub fn select(&mut self, s: Option<usize>) {
        self.delta_index = s
            .map(|s| s as i64 - self.selected.unwrap_or(s) as i64)
            .unwrap_or(0);
        self.selected = s;
    }

    pub fn next(&mut self) {
        if let Some(i) = self.selected.as_mut() {
            *i += 1;
            self.delta_index += 1;
        }
    }

    pub fn previous(&mut self) {
        if let Some(i) = self.selected.as_mut() {
            if *i > 0 {
                *i -= 1;
                self.delta_index -= 1;
            }
        }
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
    connected: Vec<Range<usize>>,
}

impl<T, F> List<T, F>
where
    T: Widget,
    F: Fn(ListContext) -> (T, u16),
{
    pub fn new(len: usize, f: F) -> Self {
        List { len, f, connected: vec![] }
    }

    pub fn connecting(self, connected: Vec<Range<usize>>) -> Self {
        List { connected, ..self }
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
                state.selected_y = 0;
                state.delta_index = 0;
            } else if state.selected.unwrap() >= self.len {
                state.select(Some(self.len - 1));
            }
        }
        if self.len == 0 {
            return;
        }

        let i = state.selected.map(|i| i as i64 - state.delta_index);
        if let Some(mut i) = i {
            let r#mod = state.delta_index.clamp(-1, 1) as i32;
            while i >= 0
                && i < self.len as i64
                && i != state.selected.unwrap() as i64
            {
                let (_, h) = (self.f)(ListContext {
                    index: (i as usize).saturating_sub(if r#mod == -1 {
                        1
                    } else {
                        0
                    }),
                    is_selected: false,
                });
                match r#mod {
                    1 => state.selected_y += h as i32,
                    -1 => {
                        state.selected_y =
                            (state.selected_y as u16).saturating_sub(h) as i32;
                    }
                    _ => {}
                }
                i += r#mod as i64;
            }
        }

        let mut i = state.selected.unwrap_or(0) as i64;
        let mut y = {
            let (_, h) = (self.f)(ListContext {
                index: i as usize,
                is_selected: state
                    .selected
                    .map(|s| s == i as usize)
                    .unwrap_or(false),
            });
            let y = state.selected_y + h as i32;
            let y = y.clamp(0, area.bottom() as i32);
            state.selected_y = y - h as i32;
            y
        };
        let mut bottom_y = None;
        while i >= 0 {
            let (item, height) = (self.f)(ListContext {
                index: i as usize,
                is_selected: state
                    .selected
                    .map(|s| s == i as usize)
                    .unwrap_or(false),
            });
            y -= height as i32;
            render_truncated(
                item,
                SignedRect {
                    x: area.x as i32,
                    y: y as i32,
                    height,
                    width: area.width,
                },
                area,
                buf,
            );
            if bottom_y == None {
                bottom_y = Some(y + height as i32);
            }

            let connect_down = self
                .connected
                .iter()
                .find(|r| i >= r.start as i64 && i < r.end as i64)
                .is_some()
                && i != self.len as i64 - 1;
            let connect_up = self
                .connected
                .iter()
                .find(|r| i >= r.start as i64 && i <= r.end as i64)
                .is_some()
                && i != 0;
            if connect_down {
                Text::from("┬").render(
                    ratatui::layout::Rect {
                        x: area.left() + 2,
                        y: (area.top() as i32 + y + height as i32 - 1) as u16,
                        width: 1,
                        height: 1,
                    },
                    buf,
                );
            }
            if connect_up {
                Text::from("┴").render(
                    ratatui::layout::Rect {
                        x: area.left() + 2,
                        y: (area.top() as i32 + y) as u16,
                        width: 1,
                        height: 1,
                    },
                    buf,
                );
            }
            i -= 1;
        }

        let mut i = state.selected.unwrap_or(0) + 1;
        let mut y = bottom_y.unwrap_or(0);
        while i < self.len && y < area.height as i32 {
            let (item, height) =
                (self.f)(ListContext { index: i as usize, is_selected: false });
            render_truncated(
                item,
                SignedRect {
                    x: area.left() as i32,
                    y: y as i32,
                    height,
                    width: area.width,
                },
                area,
                buf,
            );
            if self
                .connected
                .iter()
                .find(|r| i >= r.start && i < r.end)
                .is_some()
                && i != self.len as usize - 1
            {
                Text::from("┬").render(
                    ratatui::layout::Rect {
                        x: area.left() + 2,
                        y: area.top() + y as u16 + height - 1,
                        width: 1,
                        height: 1,
                    },
                    buf,
                );
            }
            if self
                .connected
                .iter()
                .find(|r| i >= r.start && i <= r.end)
                .is_some()
            {
                Text::from("┴").render(
                    ratatui::layout::Rect {
                        x: area.left() + 2,
                        y: area.top() + y as u16,
                        width: 1,
                        height: 1,
                    },
                    buf,
                );
            }

            y += height as i32;
            i += 1;
        }

        state.delta_index = 0;
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
