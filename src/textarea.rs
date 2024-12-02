use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent,
    MouseEventKind,
};
use ratatui::{
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};

struct History {
    data: Vec<Vec<String>>,
    ptr: usize,
}

impl History {
    fn new() -> Self {
        History { data: vec![vec![]], ptr: 0 }
    }

    fn from_base(base: Vec<String>) -> Self {
        History { data: vec![base], ptr: 0 }
    }

    fn push(&mut self, lines: Vec<String>) {
        self.data = self.data.drain(0..=self.ptr).collect();
        self.data.push(lines);
        self.ptr += 1;
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

    fn current(&self) -> Vec<String> {
        return self.data[self.ptr].clone();
    }
}

// inclusive
#[derive(Clone, Copy)]
struct SelectRange {
    start: (usize, usize),
    end: (usize, usize),
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
            history: History::from_base(lines),
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

    pub fn input(&mut self, input: Input) {
        if input.ctrl || input.alt {
            return;
        }

        match input.key {
            Key::Char(char) => {
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
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.lines[self.cursor.0].insert_str(self.cursor.1, "    ");
            }
            Key::Esc => return,
            Key::Null => return,
        };
    }

    pub fn move_cursor(&mut self, cursor_move: CursorMove) {
        match cursor_move {
            CursorMove::Forward => {
                if self.lines.is_empty() {
                    return;
                }
                if self.cursor.1
                    == self.lines[self.cursor.0].chars().count() - 1
                {
                    return;
                }
                self.cursor.1 += 1;
            }
            CursorMove::Back => {
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                }
            }
            CursorMove::Up => {
                if self.cursor.0 > 0 {
                    self.cursor.0 -= 1;
                }
            }
            CursorMove::Down => {
                if self.lines.is_empty() {
                    return;
                }
                if self.cursor.0 < self.lines.len() - 1 {
                    self.cursor.0 += 1;
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
            }
            CursorMove::Bottom => {
                if self.lines.is_empty() {
                    return;
                }
                self.cursor.0 = self.lines.len() - 1;
            }
            CursorMove::WordForward => {
                if self.lines.is_empty() {
                    return;
                }
                let mut whitespace_pos = None;
                'outer: for i in self.cursor.0..self.lines.len() {
                    let line = &self.lines[i];
                    for j in self.cursor.1..line.len() {
                        if line.chars().nth(j).unwrap().is_whitespace() {
                            whitespace_pos = Some((i, j));
                            break 'outer;
                        }
                    }
                    self.cursor.1 = 0;
                }
                if whitespace_pos.is_none() {
                    self.cursor = (
                        self.lines.len() - 1,
                        self.lines.last().unwrap().len() - 1,
                    );
                    return;
                }
                let mut whitespace_pos = whitespace_pos.unwrap();
                for i in whitespace_pos.0..self.lines.len() {
                    let line = &self.lines[i];
                    for j in whitespace_pos.1..line.len() {
                        if !line.chars().nth(j).unwrap().is_whitespace() {
                            self.cursor = (i, j);
                            return;
                        }
                    }
                    whitespace_pos.1 = 0;
                }
                self.cursor =
                    (self.lines.len() - 1, self.lines.last().unwrap().len());
            }
            CursorMove::WordEnd => {
                if self.lines.is_empty() {
                    return;
                }
                let char = self.lines[self.cursor.0].chars().nth(self.cursor.1);
                if char.is_some_and(char::is_whitespace) || char.is_none() {
                    self.move_cursor(CursorMove::WordForward);
                }
            }
            CursorMove::WordBack => {
                self.move_cursor(CursorMove::Back);
                if self.lines.is_empty() || self.cursor == (0, 0) {
                    return;
                }
                let mut line = &self.lines[self.cursor.0];
                loop {
                    if let None = line.chars().nth(self.cursor.1) {
                        if self.cursor.0 == 0 {
                            return;
                        }
                        self.cursor.0 -= 1;
                        line = &self.lines[self.cursor.0];
                        self.cursor.1 = line.len() - 1;
                    } else {
                        break;
                    }
                }

                let mut first_time = true;
                for i in (0..=self.cursor.0).rev() {
                    let line = &self.lines[i];
                    if first_time {
                        self.cursor.1 = line.len() - 1;
                    }
                    for j in (0..=self.cursor.1).rev() {
                        let curr = line.chars().nth(j).unwrap();
                        if curr.is_ascii_alphanumeric() == false {
                            self.cursor = (i, j);
                        }
                    }
                    first_time = false;
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
            CursorMove::Jump(mut y, mut x) => {
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
    }

    pub fn insert_newline(&mut self) {
        self.history.push(self.lines.clone());
        self.lines.insert(self.cursor.0, String::new());
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
        if self.cursor.1 == self.lines[self.cursor.0].len() {
            self.cursor.1 -= 1;
        }
    }

    pub fn undo(&mut self) {
        self.history.move_backward();
        self.lines = self.history.current();
    }

    pub fn redo(&mut self) {
        self.history.move_forward();
        self.lines = self.history.current();
    }

    pub fn paste(&mut self) {
        if self.clipboard.len() == 0 {
            return;
        }
        self.history.push(self.lines.clone());
        let mut clipboard = vec![];
        std::mem::swap(&mut self.clipboard, &mut clipboard);
        if clipboard.len() == 0 {
            return;
        }

        let lines_len = clipboard.len();
        let mut lines = clipboard.into_iter().peekable();
        let first_line = lines.next().unwrap();
        let (head, tail) = self.lines[self.cursor.0].split_at(self.cursor.1);
        let head = head.to_string();
        let tail = tail.to_string();

        if lines.peek().is_none() {
            self.lines[self.cursor.0].insert_str(self.cursor.1, &first_line);
            return;
        } else {
            self.lines[self.cursor.0] = head + &first_line;
        }
        self.cursor.0 += 1;
        for _ in 1..lines_len - 1 {
            self.lines.insert(self.cursor.0, lines.next().unwrap());
            self.cursor.0 += 1;
        }
        self.lines[self.cursor.0] = tail + &self.lines[self.cursor.0];
    }

    pub fn copy(&mut self) {
        if self.select.is_none() {
            return;
        }
        let range = self.select.unwrap();
        if range.start.0 == range.end.0 {
            let clipboard_content =
                &self.clipboard[range.start.0][range.start.1..=range.start.1];
            self.clipboard = vec![clipboard_content.to_string()];
            return;
        }

        let first_line = std::iter::once(
            self.clipboard[range.start.0][range.start.1..].to_string(),
        );
        let middle_lines = self.clipboard[range.start.0 + 1..range.end.0]
            .iter()
            .map(String::clone);
        let last_line = std::iter::once(
            self.clipboard[range.end.0][..=range.end.1].to_string(),
        );
        self.clipboard =
            first_line.chain(middle_lines).chain(last_line).collect();
    }

    pub fn cut(&mut self) {
        if self.select.is_none() {
            return;
        }
        self.history.push(self.lines.clone());

        let range = self.select.unwrap();
        if range.start.0 == range.end.0 {
            let head = self.lines[range.start.0].chars().take(range.start.1);
            let tail = self.lines[range.start.0].chars().skip(range.end.1 + 1);
            self.lines[range.start.0] = head.chain(tail).collect::<String>();
            return;
        }

        let removed_lines =
            self.lines.drain(range.start.0..=range.end.1).collect::<Vec<_>>();
        let first_line = removed_lines.first().unwrap().clone();
        let last_line = removed_lines.last().unwrap().clone();

        let (first_line_left, first_line_cut) =
            first_line.split_at(range.start.1);
        let (last_line_cut, last_line_left) = last_line.split_at(range.end.1);

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
        self.cursor = range.start;
        self.cancel_selection();
    }

    pub fn start_selection(&mut self) {
        self.select =
            Some(SelectRange { start: self.cursor, end: self.cursor });
    }

    pub fn cancel_selection(&mut self) {
        self.select = None;
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
                if i != self.cursor.0 {
                    return Line::from(l);
                }
                let focus_style = if self.focused {
                    Style::default().bg(Color::Rgb(45, 50, 55))
                } else {
                    Style::default()
                };
                Span::styled(
                    l.chars().take(self.cursor.1).collect::<String>(),
                    focus_style,
                ) + match l.chars().nth(self.cursor.1) {
                    _ if !self.focused => Span::from(""),
                    Some(c) => {
                        Span::styled(c.to_string(), Style::default().reversed())
                    }
                    None => Span::from("█"),
                } + Span::styled(
                    l.chars().skip(self.cursor.1 + 1).collect::<String>(),
                    focus_style,
                )
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
    Jump(usize, usize),
}
