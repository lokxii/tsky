use bsky_sdk::BskyAgent;
use ratatui::crossterm::event;

use crate::app::{AppEvent, EventReceiver};

use super::textarea::{CursorMove, Input, Key, TextArea};

pub enum InputMode {
    Normal,
    Insert,
    Visual,
}

pub struct Vim {
    pub textarea: TextArea,
    pub mode: InputMode,
    pub allowed_input: fn(Input) -> bool,
}

impl Vim {
    pub fn new(f: fn(Input) -> bool) -> Self {
        Self {
            textarea: TextArea::from(String::new()),
            mode: InputMode::Normal,
            allowed_input: f,
        }
    }
}

impl EventReceiver for &mut Vim {
    async fn handle_events(
        self,
        event: event::Event,
        _: BskyAgent,
    ) -> AppEvent {
        match event.into() {
            Input { key: Key::Esc, .. } => match self.mode {
                InputMode::Insert => {
                    self.mode = InputMode::Normal;
                    self.textarea.push_history();
                    self.textarea.snap_cursor();
                }
                InputMode::Normal => {}
                InputMode::Visual => {
                    let cursor = self.textarea.cursor();
                    self.textarea.cancel_selection();
                    self.textarea.move_cursor(CursorMove::Jump(cursor));
                    self.mode = InputMode::Normal;
                }
            },

            i if matches!(self.mode, InputMode::Insert) => {
                self.textarea.input(i, &self.allowed_input);
            }

            // normal mode
            Input { key: Key::Backspace, .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    return AppEvent::ColumnPopLayer;
                }
            }
            Input { key: Key::Char('i'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.mode = InputMode::Insert;
                }
            }
            Input { key: Key::Char('A'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.move_cursor(CursorMove::End);
                    self.mode = InputMode::Insert;
                }
            }
            Input { key: Key::Char('o'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.insert_newline_after();
                    self.textarea.move_cursor(CursorMove::Down);
                    self.textarea.snap_cursor();
                    self.mode = InputMode::Insert;
                }
            }
            Input { key: Key::Char('O'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.insert_newline_before();
                    self.textarea.snap_cursor();
                    self.mode = InputMode::Insert;
                }
            }
            Input { key: Key::Char('p'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.paste_after();
                }
            }
            Input { key: Key::Char('P'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.paste_before();
                }
            }
            Input { key: Key::Char('u'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.undo();
                }
            }
            Input { key: Key::Char('r'), ctrl: true, .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.redo();
                }
            }
            Input { key: Key::Char('v'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.start_selection();
                    self.mode = InputMode::Visual;
                }
            }
            Input { key: Key::Char('x'), .. } => {
                if matches!(self.mode, InputMode::Normal) {
                    self.textarea.delete_char();
                }
            }
            Input { key: Key::Char('>'), .. } => {
                if matches!(self.mode, InputMode::Normal)
                    && matches!(
                        event::read().unwrap().into(),
                        Input { key: Key::Char('>'), .. }
                    )
                {
                    self.textarea.indent_right();
                }
            }
            Input { key: Key::Char('<'), .. } => {
                if matches!(self.mode, InputMode::Normal)
                    && matches!(
                        event::read().unwrap().into(),
                        Input { key: Key::Char('<'), .. }
                    )
                {
                    self.textarea.indent_left();
                }
            }

            // universal movement
            Input { key: Key::Char('h'), .. } => {
                self.textarea.move_cursor(CursorMove::Back)
            }
            Input { key: Key::Char('j'), .. } => {
                self.textarea.move_cursor(CursorMove::Down)
            }
            Input { key: Key::Char('k'), .. } => {
                self.textarea.move_cursor(CursorMove::Up)
            }
            Input { key: Key::Char('l'), .. } => {
                self.textarea.move_cursor(CursorMove::Forward)
            }
            Input { key: Key::Char('w'), .. } => {
                self.textarea.move_cursor(CursorMove::WordForward)
            }
            Input { key: Key::Char('b'), .. } => {
                self.textarea.move_cursor(CursorMove::WordBack)
            }
            Input { key: Key::Char('e'), .. } => {
                self.textarea.move_cursor(CursorMove::WordEnd)
            }
            Input { key: Key::Char('0'), .. } => {
                self.textarea.move_cursor(CursorMove::Head)
            }
            Input { key: Key::Char('$'), .. } => {
                self.textarea.move_cursor(CursorMove::End);
                self.textarea.snap_cursor();
            }
            Input { key: Key::Char('g'), .. } => {
                if matches!(
                    event::read().unwrap().into(),
                    Input { key: Key::Char('g'), .. }
                ) {
                    self.textarea.move_cursor(CursorMove::Top);
                }
            }
            Input { key: Key::Char('G'), .. } => {
                self.textarea.move_cursor(CursorMove::Bottom);
            }

            Input { key: Key::Char('d'), .. } => match self.mode {
                InputMode::Normal => {
                    let e = event::read().unwrap().into();
                    match e {
                        Input { key: Key::Char('d'), .. } => {
                            self.textarea.delete_line();
                        }
                        Input { key: Key::Char('w'), .. } => {
                            self.textarea.start_selection();
                            self.textarea.move_cursor(CursorMove::WordForward);
                            self.textarea.move_cursor(CursorMove::Back);
                            self.textarea.cut();
                        }
                        Input { key: Key::Char('e'), .. } => {
                            self.textarea.start_selection();
                            self.textarea.move_cursor(CursorMove::WordEnd);
                            self.textarea.cut();
                        }
                        Input { key: Key::Char('b'), .. } => {
                            self.textarea.start_selection();
                            self.textarea.move_cursor(CursorMove::WordBack);
                            self.textarea.cut();
                        }
                        _ => {}
                    }
                }
                InputMode::Visual => {
                    self.textarea.cut();
                    self.mode = InputMode::Normal;
                }
                InputMode::Insert => {}
            },
            Input { key: Key::Char('y'), .. } => {
                if matches!(self.mode, InputMode::Visual) {
                    self.textarea.copy();
                    self.textarea.cancel_selection();
                    self.mode = InputMode::Normal;
                }
            }
            _ => {}
        };
        return AppEvent::None;
    }
}
