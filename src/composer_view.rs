use bsky_sdk::BskyAgent;
use crossterm::event::{self, Event};
use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, BorderType, Widget},
};
use tui_textarea::{CursorMove, Input, Key, TextArea};

use crate::app::AppEvent;

enum InputMode {
    Normal,
    Insert,
    View,
}

enum Focus {
    Textarea,
    Langlist,
}

pub struct ComposerView {
    textarea: TextArea<'static>,
    inputmode: InputMode,
    focus: Focus,
    lang: Vec<String>,
}

impl ComposerView {
    pub fn new() -> Self {
        ComposerView {
            textarea: TextArea::from(Vec::<String>::new()),
            inputmode: InputMode::Normal,
            focus: Focus::Textarea,
            lang: vec!["en".to_string()],
        }
    }
    pub async fn handle_input_events(&mut self, agent: BskyAgent) -> AppEvent {
        let event = event::read().expect("Cannot read event");
        let Event::Key(key) = event.clone().into() else {
            return AppEvent::None;
        };
        if key.kind != event::KeyEventKind::Press {
            return AppEvent::None;
        }
        match self.inputmode {
            InputMode::Insert => match event.into() {
                Input { key: Key::Esc, .. } => {
                    self.inputmode = InputMode::Normal;
                    return AppEvent::None;
                }
                input => {
                    self.textarea.input(input);
                    return AppEvent::None;
                }
            },
            InputMode::Normal => {
                match event.clone().into() {
                    Input { key: Key::Enter, .. } => {
                        let r = agent.create_record(
                            atrium_api::app::bsky::feed::post::RecordData {
                                created_at: atrium_api::types::string::Datetime::now(),
                                embed: None,
                                entities: None,
                                facets: None,
                                labels: None,
                                langs: None,
                                reply: None,
                                tags: None,
                                text: self.textarea.lines().join("\n")
                            },
                        ).await;
                        match r {
                            Ok(_) => {
                                return AppEvent::ColumnPopLayer;
                            }
                            Err(e) => {
                                log::error!("Cannot post: {}", e);
                            }
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        };
        return vim_keys(event, &mut self.textarea, &mut self.inputmode);
    }
}

fn vim_keys(
    event: impl Into<Input>,
    textarea: &mut TextArea,
    inputmode: &mut InputMode,
) -> AppEvent {
    match event.into() {
        // normal mode
        Input { key: Key::Backspace, .. } => {
            if matches!(inputmode, InputMode::Normal) {
                return AppEvent::ColumnPopLayer;
            }
        }
        Input { key: Key::Char('i'), .. } => {
            if matches!(inputmode, InputMode::Normal) {
                *inputmode = InputMode::Insert;
            }
        }
        Input { key: Key::Char('A'), .. } => {
            if matches!(inputmode, InputMode::Normal) {
                textarea.move_cursor(CursorMove::End);
                *inputmode = InputMode::Insert;
            }
        }
        Input { key: Key::Char('o'), .. } => {
            if matches!(inputmode, InputMode::Normal) {
                textarea.move_cursor(CursorMove::End);
                textarea.insert_newline();
                *inputmode = InputMode::Insert;
            }
        }
        Input { key: Key::Char('O'), .. } => {
            if matches!(inputmode, InputMode::Normal) {
                textarea.move_cursor(CursorMove::Head);
                textarea.insert_newline();
                textarea.move_cursor(CursorMove::Up);
                *inputmode = InputMode::Insert;
            }
        }
        Input { key: Key::Char('p'), .. } => {
            textarea.paste();
        }
        Input { key: Key::Char('u'), .. } => {
            textarea.undo();
        }
        Input { key: Key::Char('r'), ctrl: true, .. } => {
            textarea.redo();
        }
        Input { key: Key::Char('v'), .. } => {
            if matches!(*inputmode, InputMode::Normal) {
                textarea.start_selection();
                *inputmode = InputMode::View;
            }
        }
        Input { key: Key::Char('x'), .. } => {
            textarea.delete_next_char();
        }
        Input { key: Key::Char('>'), .. } => {
            if matches!(*inputmode, InputMode::Normal)
                && matches!(
                    event::read().unwrap().into(),
                    Input { key: Key::Char('>'), .. }
                )
            {
                let (y, x) = textarea.cursor();
                let mut lines = textarea.clone().into_lines();
                let mut new_line = String::from("    ");
                new_line += &lines[y];
                lines[y] = new_line;
                *textarea = TextArea::new(lines);
                textarea.move_cursor(CursorMove::Jump(y as u16, x as u16));
            }
        }
        Input { key: Key::Char('<'), .. } => {
            if matches!(*inputmode, InputMode::Normal)
                && matches!(
                    event::read().unwrap().into(),
                    Input { key: Key::Char('<'), .. }
                )
            {
                let (y, x) = textarea.cursor();
                let mut lines = textarea.clone().into_lines();
                let mut count = 0;
                lines[y] = lines[y]
                    .chars()
                    .skip_while(|c| {
                        count += 1;
                        *c == ' ' && count <= 4
                    })
                    .collect();
                *textarea = TextArea::new(lines);
                textarea.move_cursor(CursorMove::Jump(y as u16, x as u16));
            }
        }

        // universal movement
        Input { key: Key::Char('h'), .. } => {
            textarea.move_cursor(CursorMove::Back)
        }
        Input { key: Key::Char('j'), .. } => {
            textarea.move_cursor(CursorMove::Down)
        }
        Input { key: Key::Char('k'), .. } => {
            textarea.move_cursor(CursorMove::Up)
        }
        Input { key: Key::Char('l'), .. } => {
            textarea.move_cursor(CursorMove::Forward)
        }
        Input { key: Key::Char('w'), .. } => {
            textarea.move_cursor(CursorMove::WordForward)
        }
        Input { key: Key::Char('b'), .. } => {
            textarea.move_cursor(CursorMove::WordBack)
        }
        Input { key: Key::Char('e'), .. } => {
            textarea.move_cursor(CursorMove::WordEnd)
        }
        Input { key: Key::Char('^'), .. } => {
            textarea.move_cursor(CursorMove::Head)
        }
        Input { key: Key::Char('$'), .. } => {
            textarea.move_cursor(CursorMove::End)
        }
        Input { key: Key::Char('g'), .. } => {
            if matches!(
                event::read().unwrap().into(),
                Input { key: Key::Char('g'), .. }
            ) {
                textarea.move_cursor(CursorMove::Top);
            }
        }
        Input { key: Key::Char('G'), .. } => {
            textarea.move_cursor(CursorMove::Bottom);
        }

        Input { key: Key::Char('d'), .. } => match *inputmode {
            InputMode::Normal => {
                let e = event::read().unwrap().into();
                match e {
                    Input { key: Key::Char('d'), .. } => {
                        textarea.move_cursor(CursorMove::Head);
                        textarea.delete_line_by_end();
                        textarea.delete_newline();
                        textarea.move_cursor(CursorMove::Down);
                    }
                    Input { key: Key::Char('w'), .. } => {
                        textarea.start_selection();
                        textarea.move_cursor(CursorMove::WordForward);
                        textarea.cut();
                        textarea.cancel_selection();
                    }
                    Input { key: Key::Char('e'), .. } => {
                        textarea.delete_next_word();
                    }
                    Input { key: Key::Char('b'), .. } => {
                        textarea.delete_word();
                    }
                    _ => {}
                }
            }
            InputMode::View => {
                textarea.move_cursor(CursorMove::Forward);
                textarea.cut();
                *inputmode = InputMode::Normal;
            }
            InputMode::Insert => {}
        },
        Input { key: Key::Char('y'), .. } => {
            if matches!(inputmode, InputMode::View) {
                textarea.move_cursor(CursorMove::Forward);
                textarea.copy();
                textarea.cancel_selection();
                *inputmode = InputMode::Normal;
            }
        }

        Input { key: Key::Esc, .. } => {
            if matches!(inputmode, InputMode::View) {
                textarea.cancel_selection();
                *inputmode = InputMode::Normal;
            }
        }
        _ => (),
    };
    return AppEvent::None;
}

impl Widget for &mut ComposerView {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let width = if area.width > 60 { 50 } else { area.width };
        let height = if area.height > 22 { 22 } else { area.height };
        let x = if width == area.width { 0 } else { (area.width - width) / 2 };
        let y = 2;
        let upper_area = Rect { x, y, width, height };

        let title = match self.inputmode {
            InputMode::Normal => "New Note (Normal)",
            InputMode::Insert => "New Note (Insert)",
            InputMode::View => "New Note (View)",
        };
        let text_lines = self.textarea.lines();
        let word_remaining = if text_lines.len() == 0 {
            0
        } else {
            300 - text_lines
                .into_iter()
                .map(|l| l.chars().count())
                .sum::<usize>()
                - text_lines.len()
                + 1
        };
        self.textarea.set_block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title(Line::from(title).left_aligned())
                .title(Line::from(word_remaining.to_string()).right_aligned()),
        );
        self.textarea.set_cursor_line_style(Style::default());
        self.textarea.render(upper_area, buf);

        let lower_area = Rect {
            y: upper_area.y + upper_area.height + 1,
            height: 3,
            ..upper_area
        };
        let chosen_langs_box =
            Block::bordered().border_type(BorderType::Rounded).title("Langs");
        let langs_inner_area = chosen_langs_box.inner(lower_area);
        chosen_langs_box.render(lower_area, buf);
        Line::from(self.lang.join(",").as_str()).render(langs_inner_area, buf);
    }
}
