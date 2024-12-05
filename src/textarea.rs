use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent,
    MouseEventKind,
};
use ratatui::{
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};
use std::cmp::Ordering;

struct History {
    data: Vec<(Vec<String>, (usize, usize))>,
    ptr: usize,
    changed: bool,
}

impl History {
    fn new() -> Self {
        History { data: vec![(vec![], (0, 0))], ptr: 0, changed: true }
    }

    fn from(base: Vec<String>) -> Self {
        History { data: vec![(base, (0, 0))], ptr: 0, changed: true }
    }

    fn push(&mut self, lines: Vec<String>, cursor: (usize, usize)) {
        if self.changed == false {
            return;
        }
        self.data = self.data.drain(0..=self.ptr).collect();
        self.data.push((lines, cursor));
        self.ptr += 1;
        self.changed = false;
    }

    fn move_backward(&mut self) {
        if self.ptr > 0 {
            self.ptr -= 1;
        }
    }

    fn move_forward(&mut self) {
        if self.ptr < self.data.len() - 1 {
            self.ptr += 1;
        }
    }

    fn current(&self) -> (Vec<String>, (usize, usize)) {
        return self.data[self.ptr].clone();
    }
}

#[derive(Clone, Copy)]
enum SelectRangeMode {
    Left,
    Right,
}

// inclusive
#[derive(Clone, Copy)]
struct SelectRange {
    start: (usize, usize),
    end: (usize, usize),
    mode: SelectRangeMode,
}

impl SelectRange {
    fn in_range(self, (i, j): (usize, usize)) -> bool {
        if !(self.start.0 <= i && i <= self.end.0) {
            return false;
        }
        if self.start.0 == i {
            if i < self.end.0 {
                return self.start.1 <= j;
            } else {
                return self.start.1 <= j && j <= self.end.1;
            }
        }
        if self.end.0 == i {
            return j <= self.end.1;
        }
        return true;
    }
}

fn cell_cmp(left: (usize, usize), right: (usize, usize)) -> Ordering {
    if left == right {
        return Ordering::Equal;
    } else if left.0 > right.0 {
        return Ordering::Greater;
    } else if left.0 == right.0 && left.1 > right.1 {
        return Ordering::Greater;
    } else {
        return Ordering::Less;
    }
}

fn cell_lt(left: (usize, usize), right: (usize, usize)) -> bool {
    matches!(cell_cmp(left, right), Ordering::Less)
}

fn cell_le(left: (usize, usize), right: (usize, usize)) -> bool {
    matches!(cell_cmp(left, right), Ordering::Less | Ordering::Equal)
}

pub struct TextArea {
    lines: Vec<String>,
    cursor: (usize, usize),
    clipboard: Vec<String>,
    history: History,
    select: Option<SelectRange>,
    block: Option<Block<'static>>,
    focused: bool,
}

macro_rules! string_remove_index {
    ($s:expr, $i:expr) => {
        $s = $s
            .chars()
            .enumerate()
            .filter_map(|(i, l)| if i == $i { None } else { Some(l) })
            .collect::<String>();
    };
}

macro_rules! string_insert {
    ($s:expr, $i:expr, $c:expr) => {
        $s = $s
            .chars()
            .take($i)
            .chain(std::iter::once($c))
            .chain($s.chars().skip($i))
            .collect::<String>();
    };
}

impl TextArea {
    pub fn new(lines: Vec<String>) -> Self {
        TextArea {
            lines: lines
                .into_iter()
                .map(|l| l.split("\n").map(String::from).collect::<Vec<_>>())
                .flatten()
                .collect(),
            cursor: (0, 0),
            clipboard: vec![],
            history: History::new(),
            select: None,
            block: None,
            focused: true,
        }
    }

    pub fn from(text: String) -> Self {
        let lines = text.split("\n").map(String::from).collect::<Vec<String>>();
        TextArea {
            lines: lines.clone(),
            cursor: (0, 0),
            clipboard: vec![],
            history: History::from(lines),
            select: None,
            block: None,
            focused: true,
        }
    }

    pub fn lines(&self) -> &[String] {
        return &self.lines;
    }

    pub fn into_lines(&self) -> Vec<String> {
        return self.lines.clone();
    }

    pub fn cursor(&self) -> (usize, usize) {
        return self.cursor;
    }

    pub fn set_block(&mut self, block: Block<'static>) {
        self.block = Some(block);
    }

    pub fn set_focus(&mut self, focus: bool) {
        self.focused = focus;
    }

    pub fn snap_cursor(&mut self) {
        let line_count = self.lines[self.cursor.0].chars().count();
        if line_count > 0 && self.cursor.1 >= line_count {
            self.cursor.1 = line_count - 1;
        }
    }

    pub fn input(&mut self, input: Input) {
        if input.ctrl || input.alt {
            return;
        }

        match input.key {
            Key::Char(char) => {
                self.history.changed = true;
                if self.lines.is_empty() {
                    self.lines.push(String::from(char));
                    return;
                }
                string_insert!(self.lines[self.cursor.0], self.cursor.1, char);
                self.cursor.1 += 1;
            }
            Key::Backspace => {
                if self.lines.is_empty() || self.cursor == (0, 0) {
                    return;
                }
                self.history.changed = true;

                if self.cursor.1 == 0 {
                    let line = self.lines.remove(self.cursor.0);
                    self.cursor.0 -= 1;
                    self.cursor.1 = self.lines[self.cursor.0].chars().count();
                    self.lines[self.cursor.0] += &line;
                    return;
                }
                string_remove_index!(
                    self.lines[self.cursor.0],
                    self.cursor.1 - 1
                );
                self.cursor.1 -= 1;
            }
            Key::Enter => {
                self.history.changed = true;
                self.lines.insert(self.cursor.0 + 1, String::new());
                self.cursor.0 += 1;
                self.cursor.1 = 0;
            }
            Key::Left => {
                self.move_cursor(CursorMove::Back);
            }
            Key::Right => {
                self.move_cursor(CursorMove::Forward);
            }
            Key::Up | Key::MouseScrollUp => {
                self.move_cursor(CursorMove::Up);
            }
            Key::Down | Key::MouseScrollDown => {
                self.move_cursor(CursorMove::Down);
            }
            Key::Tab => {
                self.history.changed = true;
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.lines[self.cursor.0].insert_str(self.cursor.1, "    ");
            }
            Key::Esc => return,
            Key::Null => return,
        };
    }

    fn update_select_range(&mut self) {
        let Some(range) = self.select.as_mut() else {
            return;
        };

        match range.mode {
            SelectRangeMode::Left => {
                let cursor_start_end = cell_lt(self.cursor, range.start)
                    && cell_le(range.start, range.end);
                let start_cursor_end = cell_lt(range.start, self.cursor)
                    && cell_le(self.cursor, range.end);
                let start_end_cursor = cell_le(range.start, range.end)
                    && cell_lt(range.end, self.cursor);

                if cursor_start_end || start_cursor_end {
                    range.start = self.cursor;
                    return;
                }
                if start_end_cursor {
                    range.mode = SelectRangeMode::Right;
                    range.start = range.end;
                    range.end = self.cursor;
                    return;
                }
            }
            SelectRangeMode::Right => {
                let cursor_start_end = cell_lt(self.cursor, range.start)
                    && cell_le(range.start, range.end);
                let start_cursor_end = cell_lt(range.start, self.cursor)
                    && cell_le(self.cursor, range.end);
                let start_end_cursor = cell_le(range.start, range.end)
                    && cell_lt(range.end, self.cursor);

                if start_end_cursor || start_cursor_end {
                    range.end = self.cursor;
                    return;
                }
                if cursor_start_end {
                    range.mode = SelectRangeMode::Left;
                    range.end = range.start;
                    range.start = self.cursor;
                    return;
                }
            }
        }
    }

    pub fn move_cursor(&mut self, cursor_move: CursorMove) {
        match cursor_move {
            CursorMove::Forward => {
                if self.lines.is_empty() {
                    return;
                }
                if self.cursor.1 < self.lines[self.cursor.0].chars().count() - 1
                {
                    self.cursor.1 += 1;
                } else if self.cursor.0 < self.lines.len() - 1 {
                    self.move_cursor(CursorMove::Down);
                    self.move_cursor(CursorMove::Head);
                }
            }
            CursorMove::Back => {
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                } else if self.cursor.0 > 0 {
                    self.move_cursor(CursorMove::Up);
                    self.move_cursor(CursorMove::End);
                }
            }
            CursorMove::Up => {
                if self.cursor.0 > 0 {
                    self.cursor.0 -= 1;
                    self.snap_cursor();
                }
            }
            CursorMove::Down => {
                if self.lines.is_empty() {
                    return;
                }
                if self.cursor.0 < self.lines.len() - 1 {
                    self.cursor.0 += 1;
                    self.snap_cursor();
                }
            }
            CursorMove::Head => {
                self.cursor.1 = 0;
            }
            CursorMove::End => {
                if self.lines.is_empty() {
                    return;
                }
                self.cursor.1 = self.lines[self.cursor.0].chars().count();
            }
            CursorMove::Top => {
                self.cursor.0 = 0;
                self.snap_cursor();
            }
            CursorMove::Bottom => {
                if self.lines.is_empty() {
                    return;
                }
                self.cursor.0 = self.lines.len() - 1;
                self.snap_cursor();
            }
            CursorMove::WordForward => {
                if self.lines.is_empty() {
                    return;
                }
                let (_, t) = self.lines[self.cursor.0].split_at(self.cursor.1);
                let mut words = t.split(' ');
                let mut dx = words.next().unwrap().chars().count() + 1;
                let mut words = words.skip_while(|w| {
                    dx += w.is_empty() as usize;
                    w.is_empty()
                });

                if let Some(_) = words.next() {
                    self.cursor.1 += dx;
                    self.update_select_range();
                    return;
                }

                for i in self.cursor.0 + 1..self.lines.len() {
                    let mut dx = 0;
                    let mut words = self.lines[i].split(' ').skip_while(|w| {
                        dx += w.is_empty() as usize;
                        w.is_empty()
                    });
                    if let Some(_) = words.next() {
                        self.cursor = (i, dx);
                        self.update_select_range();
                        return;
                    }
                }
                self.cursor = (
                    self.lines.len() - 1,
                    self.lines.last().unwrap().chars().count() - 1,
                );
            }
            CursorMove::WordEnd => {
                if self.lines.is_empty() {
                    return;
                }

                for i in self.cursor.0..self.lines.len() {
                    let s = &self.lines[self.cursor.0];
                    let (_, t) = s.split_at(self.cursor.1);
                    let mut words = t.split(' ').peekable();
                    let mut dx = 0;
                    if (i == 0 || self.cursor.1 > 0)
                        && words.peek().unwrap().chars().count() == 1
                    {
                        words.next();
                        dx += 2;
                    }
                    let word = words
                        .skip_while(|w| {
                            dx += w.is_empty() as usize;
                            w.is_empty()
                        })
                        .next();
                    if let Some(word) = word {
                        self.cursor.1 += dx + word.chars().count() - 1;
                        self.update_select_range();
                        return;
                    }
                    self.cursor.0 += 1;
                    self.cursor.1 = 0;
                }

                self.cursor = (
                    self.lines.len() - 1,
                    self.lines.last().unwrap().chars().count() - 1,
                );
            }
            CursorMove::WordBack => {
                if self.lines.is_empty() {
                    return;
                }
                for i in 0..=self.cursor.0 {
                    let s = &self.lines[self.cursor.0];
                    let (h, _) = s.split_at(self.cursor.1 + 1);
                    let mut words = h.split(' ').rev().peekable();
                    let mut dx = 0;
                    if i == 0 && words.peek().unwrap().chars().count() == 1 {
                        words.next();
                        dx += 2;
                    }
                    let word = words
                        .skip_while(|w| {
                            dx += w.is_empty() as usize;
                            w.is_empty()
                        })
                        .next();
                    if let Some(word) = word {
                        self.cursor.1 =
                            self.cursor.1 + 1 - dx - word.chars().count();
                        self.update_select_range();
                        return;
                    }
                    if self.cursor.0 > 0 {
                        self.cursor.0 -= 1;
                        self.cursor.1 =
                            self.lines[self.cursor.0].chars().count() - 1;
                    }
                }
            }
            CursorMove::ParagraphForward => {
                if self.lines.is_empty() {
                    return;
                }
                let mut found_empty_line = false;
                for i in self.cursor.0..self.lines.len() {
                    if self.lines[i].len() == 0 {
                        found_empty_line = true;
                    } else {
                        if found_empty_line {
                            self.cursor = (i - 1, 0);
                        }
                    }
                }
                self.cursor = (self.lines.len() - 1, 0)
            }
            CursorMove::ParagraphBack => {
                if self.lines.is_empty() {
                    return;
                }
                let mut found_empty_line = false;
                for i in (0..=self.cursor.0).rev() {
                    if self.lines[i].len() == 0 {
                        found_empty_line = true;
                    } else {
                        if found_empty_line {
                            self.cursor = (i + 1, 0);
                        }
                    }
                }
                self.cursor = (0, 0)
            }
            CursorMove::Jump((mut y, mut x)) => {
                y = if y >= self.lines.len() {
                    self.lines.len() - 1
                } else {
                    y
                };
                let c = self.lines[y].len();
                x = if x >= c { c - 1 } else { x };
                self.cursor = (y, x);
            }
        }
        self.update_select_range();
    }

    pub fn insert_newline_before(&mut self) {
        self.lines.insert(self.cursor.0, String::new());

        self.history.changed = true;
        self.push_history();
    }

    pub fn insert_newline_after(&mut self) {
        let i = if self.lines.len() > 0 {
            self.cursor.0 + 1
        } else {
            self.cursor.0
        };
        self.lines.insert(i, String::new());

        self.history.changed = true;
        self.push_history();
    }

    pub fn delete_line(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        self.lines.remove(self.cursor.0);
        self.cursor.1 = 0;
        if self.cursor.0 > 0 && self.cursor.0 == self.lines.len() {
            self.cursor.0 -= 1;
        }
    }

    pub fn delete_char(&mut self) {
        if self.lines.is_empty()
            || self.lines[self.cursor.0].len() == self.cursor.1
        {
            return;
        }
        string_remove_index!(self.lines[self.cursor.0], self.cursor.1);
        if self.cursor.1 > 0 && self.cursor.1 == self.lines[self.cursor.0].len()
        {
            self.cursor.1 -= 1;
        }
    }

    pub fn indent_right(&mut self) {
        if self.lines.is_empty()
            || self.lines[self.cursor.0].chars().count() == 0
        {
            return;
        }
        self.lines[self.cursor.0] =
            String::from("    ") + &self.lines[self.cursor.0];
    }

    pub fn indent_left(&mut self) {
        if self.lines.is_empty()
            || self.lines[self.cursor.0].chars().count() == 0
        {
            return;
        }
        let mut count = 0;
        self.lines[self.cursor.0] = self.lines[self.cursor.0]
            .chars()
            .skip_while(|c| {
                count += 1;
                c.is_whitespace() && count <= 4
            })
            .collect::<String>();
        self.snap_cursor();
    }

    pub fn undo(&mut self) {
        self.history.move_backward();
        let (lines, cursor) = self.history.current();
        self.lines = lines;
        self.cursor = cursor;
        self.snap_cursor();
    }

    pub fn redo(&mut self) {
        self.history.move_forward();
        let (lines, cursor) = self.history.current();
        self.lines = lines;
        self.cursor = cursor;
        self.snap_cursor();
    }

    pub fn paste_before(&mut self) {
        if self.clipboard.len() == 0 {
            return;
        }
        let clipboard = self.clipboard.clone();
        if clipboard.len() == 0 {
            return;
        }

        let clipboard_len = clipboard.len();
        let mut clipboard = clipboard.into_iter().peekable();
        let first_line = clipboard.next().unwrap();
        let (head, tail) = self.lines[self.cursor.0].split_at(self.cursor.1);
        let head = head.to_string();
        let tail = tail.to_string();

        if clipboard_len == 1 {
            self.lines[self.cursor.0] = head.to_string() + &first_line + &tail;
            self.cursor.1 += first_line.chars().count();

            self.history.changed = true;
            self.push_history();
            return;
        }

        self.lines[self.cursor.0] = head.to_string() + &first_line;
        for _ in 1..clipboard_len {
            self.cursor.0 += 1;
            self.lines.insert(self.cursor.0, clipboard.next().unwrap());
        }
        self.cursor.1 = self.lines[self.cursor.0].chars().count() - 1;
        self.lines[self.cursor.0] += &tail;

        self.history.changed = true;
        self.push_history();
    }

    pub fn paste_after(&mut self) {
        if self.clipboard.len() == 0 {
            return;
        }
        self.cursor.1 += 1;
        if self.cursor.1 > self.lines[self.cursor.0].chars().count() {
            self.cursor.1 = self.lines[self.cursor.0].chars().count()
        }
        self.paste_before();
    }

    pub fn copy(&mut self) {
        if self.select.is_none() || self.lines.len() == 0 {
            return;
        }
        let range = self.select.unwrap();
        if range.start.0 == range.end.0 {
            let clipboard_content =
                &self.lines[range.start.0][range.start.1..=range.end.1];
            self.clipboard = vec![clipboard_content.to_string()];
            return;
        }

        let first_line = std::iter::once(
            self.lines[range.start.0][range.start.1..].to_string(),
        );
        let middle_lines = self.lines[range.start.0 + 1..range.end.0]
            .iter()
            .map(String::clone);
        let last_line = std::iter::once(
            self.lines[range.end.0][..=range.end.1].to_string(),
        );
        self.clipboard =
            first_line.chain(middle_lines).chain(last_line).collect();
    }

    pub fn cut(&mut self) {
        if self.select.is_none() || self.lines.len() == 0 {
            return;
        }

        let range = self.select.unwrap();
        if range.start.0 == range.end.0 {
            self.clipboard = vec![self.lines[range.start.0]
                .chars()
                .skip(range.start.1)
                .take(range.end.1 - range.start.1)
                .collect::<String>()];
            let head = self.lines[range.start.0].chars().take(range.start.1);
            let tail = self.lines[range.start.0].chars().skip(range.end.1 + 1);
            self.lines[range.start.0] = head.chain(tail).collect::<String>();
            self.cancel_selection();
            self.snap_cursor();

            self.history.changed = true;
            self.push_history();
            return;
        }

        let removed_lines =
            self.lines.drain(range.start.0..=range.end.0).collect::<Vec<_>>();
        let first_line = removed_lines.first().unwrap().clone();
        let last_line = removed_lines.last().unwrap().clone();

        let (first_line_left, first_line_cut) =
            first_line.split_at(range.start.1);
        let (last_line_cut, last_line_left) =
            last_line.split_at(range.end.1 + 1);

        self.clipboard = std::iter::once(first_line_cut.to_string())
            .chain(
                removed_lines
                    .into_iter()
                    .skip(1)
                    .take(range.end.0 - range.start.0 - 1),
            )
            .chain(std::iter::once(last_line_cut.to_string()))
            .collect();

        self.lines.insert(
            range.start.0,
            first_line_left.to_string() + last_line_left,
        );
        self.cancel_selection();
        self.snap_cursor();

        self.history.changed = true;
        self.push_history();
    }

    pub fn start_selection(&mut self) {
        self.select = Some(SelectRange {
            start: self.cursor,
            end: self.cursor,
            mode: SelectRangeMode::Right,
        });
    }

    pub fn cancel_selection(&mut self) {
        self.cursor = self.select.unwrap().start;
        self.select = None;
    }

    pub fn push_history(&mut self) {
        self.history.push(self.lines.clone(), self.cursor);
    }
}

impl Widget for &mut TextArea {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let lines = self
            .lines
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                let mut count = 0;
                let mut line = l
                    .chars()
                    .enumerate()
                    .map(|(j, c)| {
                        count += 1;
                        let s = if self.cursor == (i, j) {
                            Style::default().reversed()
                        } else if self
                            .select
                            .is_some_and(|r| r.in_range((i, j)))
                        {
                            Style::default().bg(Color::Rgb(100, 100, 100))
                        } else if i == self.cursor.0 && self.focused {
                            Style::default().bg(Color::Rgb(45, 50, 55))
                        } else {
                            Style::default()
                        };
                        Span::styled(c.to_string(), s)
                    })
                    .fold(Line::from(""), |acc, s| acc + s);

                let show_cursor = self.focused
                    && i == self.cursor.0
                    && self.cursor.1 >= count;
                if show_cursor {
                    line += Span::from("█");
                }
                line
            })
            .collect::<Vec<_>>();
        let mut para = Paragraph::new(lines).wrap(Wrap { trim: false });
        if let Some(block) = &self.block {
            para = para.block(block.clone())
        }
        para.render(area, buf);
    }
}

#[derive(Default, Clone)]
pub struct Input {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl From<KeyEvent> for Input {
    fn from(key: KeyEvent) -> Self {
        if key.kind == KeyEventKind::Release {
            return Self::default();
        }

        Self {
            key: Key::from(key.code),
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
        }
    }
}

impl From<Event> for Input {
    fn from(event: Event) -> Self {
        match event {
            Event::Key(key) => Self::from(key),
            Event::Mouse(mouse) => Self::from(mouse),
            _ => Self::default(),
        }
    }
}

impl From<MouseEvent> for Input {
    fn from(mouse: MouseEvent) -> Self {
        Self {
            key: Key::from(mouse.kind),
            ctrl: mouse.modifiers.contains(KeyModifiers::CONTROL),
            alt: mouse.modifiers.contains(KeyModifiers::ALT),
            shift: mouse.modifiers.contains(KeyModifiers::SHIFT),
        }
    }
}

#[derive(Clone)]
pub enum Key {
    Char(char),
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Tab,
    Esc,
    MouseScrollDown,
    MouseScrollUp,
    Null,
}

impl Default for Key {
    fn default() -> Self {
        Key::Null
    }
}

impl From<KeyCode> for Key {
    fn from(code: KeyCode) -> Self {
        match code {
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Enter => Key::Enter,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Tab => Key::Tab,
            KeyCode::Esc => Key::Esc,
            _ => Key::Null,
        }
    }
}

impl From<MouseEventKind> for Key {
    fn from(kind: MouseEventKind) -> Self {
        match kind {
            MouseEventKind::ScrollDown => Key::MouseScrollDown,
            MouseEventKind::ScrollUp => Key::MouseScrollUp,
            _ => Key::Null,
        }
    }
}

pub enum CursorMove {
    Forward,
    Back,
    Up,
    Down,
    Head,
    End,
    Top,
    Bottom,
    WordForward,
    WordEnd,
    WordBack,
    ParagraphForward,
    ParagraphBack,
    Jump((usize, usize)),
}
