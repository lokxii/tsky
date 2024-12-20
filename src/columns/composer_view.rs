use crate::{
    app::{AppEvent, EventReceiver},
    components::{
        post::facets::{detect_facets, CharSlice, FacetFeature},
        textarea::{CursorMove, Input, Key, TextArea, TextStyle},
    },
};
use atrium_api::types::string::Language;
use bsky_sdk::{rich_text::RichText, BskyAgent};
use crossterm::event::{self, Event};
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, Widget},
};

enum InputMode {
    Normal,
    Insert,
    View,
}

enum Focus {
    TextField,
    LangField,
}

pub struct ComposerView {
    text_field: TextArea,
    inputmode: InputMode,
    focus: Focus,
    langs_field: TextArea,
}

impl ComposerView {
    pub fn new() -> Self {
        let textarea = TextArea::from(String::new());
        let lang = TextArea::from(String::new());

        ComposerView {
            text_field: textarea,
            inputmode: InputMode::Insert,
            focus: Focus::TextField,
            langs_field: lang,
        }
    }
}

impl EventReceiver for &mut ComposerView {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
    ) -> AppEvent {
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
                    match self.focus {
                        Focus::TextField => {
                            self.text_field.push_history();
                            self.text_field.snap_cursor();
                        }
                        Focus::LangField => {
                            self.text_field.push_history();
                            self.langs_field.snap_cursor();
                        }
                    }
                    return AppEvent::None;
                }
                Input { key: Key::Tab, .. } => {
                    match self.focus {
                        Focus::TextField => self.focus = Focus::LangField,
                        Focus::LangField => self.focus = Focus::TextField,
                    };
                    return AppEvent::None;
                }
                input => {
                    match self.focus {
                        Focus::TextField => {
                            self.text_field.input(input);
                        }
                        Focus::LangField => {
                            if matches!(input, Input { key: Key::Enter, .. }) {
                                return post(
                                    agent,
                                    self.text_field.lines().join("\n"),
                                    &self.langs_field.lines()[0],
                                )
                                .await;
                            }
                            if matches!(input, Input { key: Key::Char(c), .. } if ('a'..='z').contains(&c) || c == ',')
                                || matches!(input, Input { key: Key::Esc, .. })
                                || matches!(
                                    input,
                                    Input { key: Key::Backspace, .. }
                                )
                            {
                                self.langs_field.input(input);
                            }
                        }
                    };
                    return AppEvent::None;
                }
            },
            InputMode::Normal => match event.clone().into() {
                Input { key: Key::Enter, .. } => {
                    return post(
                        agent,
                        self.text_field.lines().join("\n"),
                        &self.langs_field.lines()[0],
                    )
                    .await
                }
                Input { key: Key::Tab, .. } => match self.focus {
                    Focus::TextField => self.focus = Focus::LangField,
                    Focus::LangField => self.focus = Focus::TextField,
                },
                _ => (),
            },
            _ => (),
        };
        return match self.focus {
            Focus::TextField => {
                vim_keys(event, &mut self.text_field, &mut self.inputmode)
            }
            Focus::LangField => {
                vim_keys(event, &mut self.langs_field, &mut self.inputmode)
            }
        };
    }
}

async fn post(agent: BskyAgent, text: String, langs: &String) -> AppEvent {
    let mut invalid_langs = vec![];
    let langs = langs
        .split(',')
        .filter_map(|lang| {
            if lang.is_empty() {
                return None;
            }
            if let Ok(lang) = Language::new(lang.to_string()) {
                return Some(lang);
            } else {
                invalid_langs.push(lang);
                return None;
            }
        })
        .collect::<Vec<_>>();
    if invalid_langs.len() != 0 {
        log::error!("Langs {:?} are invalid", invalid_langs);
        return AppEvent::None;
    }

    let created_at = atrium_api::types::string::Datetime::now();
    let facets = match RichText::new_with_detect_facets(&text).await {
        Ok(richtext) => richtext.facets,
        Err(e) => {
            log::error!("Cannot parse richtext: {}", e);
            return AppEvent::None;
        }
    };
    let langs = if langs.is_empty() { None } else { Some(langs) };
    let r = agent
        .create_record(atrium_api::app::bsky::feed::post::RecordData {
            created_at,
            embed: None,
            entities: None,
            facets,
            labels: None,
            langs,
            reply: None,
            tags: None,
            text,
        })
        .await;
    match r {
        Ok(_) => {}
        Err(e) => {
            log::error!("Cannot post: {}", e);
        }
    }
    return AppEvent::ColumnPopLayer;
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
                textarea.insert_newline_after();
                textarea.move_cursor(CursorMove::Down);
                textarea.snap_cursor();
                *inputmode = InputMode::Insert;
            }
        }
        Input { key: Key::Char('O'), .. } => {
            if matches!(inputmode, InputMode::Normal) {
                textarea.insert_newline_before();
                textarea.snap_cursor();
                *inputmode = InputMode::Insert;
            }
        }
        Input { key: Key::Char('p'), .. } => {
            textarea.paste_after();
        }
        Input { key: Key::Char('P'), .. } => {
            textarea.paste_before();
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
            textarea.delete_char();
        }
        Input { key: Key::Char('>'), .. } => {
            if matches!(*inputmode, InputMode::Normal)
                && matches!(
                    event::read().unwrap().into(),
                    Input { key: Key::Char('>'), .. }
                )
            {
                textarea.indent_right();
            }
        }
        Input { key: Key::Char('<'), .. } => {
            if matches!(*inputmode, InputMode::Normal)
                && matches!(
                    event::read().unwrap().into(),
                    Input { key: Key::Char('<'), .. }
                )
            {
                textarea.indent_left();
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
        Input { key: Key::Char('0'), .. } => {
            textarea.move_cursor(CursorMove::Head)
        }
        Input { key: Key::Char('$'), .. } => {
            textarea.move_cursor(CursorMove::End);
            textarea.snap_cursor();
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
                        textarea.delete_line();
                    }
                    Input { key: Key::Char('w'), .. } => {
                        textarea.start_selection();
                        textarea.move_cursor(CursorMove::WordForward);
                        textarea.move_cursor(CursorMove::Back);
                        textarea.cut();
                    }
                    Input { key: Key::Char('e'), .. } => {
                        textarea.start_selection();
                        textarea.move_cursor(CursorMove::WordEnd);
                        textarea.cut();
                    }
                    Input { key: Key::Char('b'), .. } => {
                        textarea.start_selection();
                        textarea.move_cursor(CursorMove::WordBack);
                        textarea.cut();
                    }
                    _ => {}
                }
            }
            InputMode::View => {
                textarea.cut();
                *inputmode = InputMode::Normal;
            }
            InputMode::Insert => {}
        },
        Input { key: Key::Char('y'), .. } => {
            if matches!(inputmode, InputMode::View) {
                textarea.copy();
                textarea.cancel_selection();
                *inputmode = InputMode::Normal;
            }
        }

        Input { key: Key::Esc, .. } => {
            if matches!(inputmode, InputMode::View) {
                let cursor = textarea.cursor();
                textarea.cancel_selection();
                textarea.move_cursor(CursorMove::Jump(cursor));
                *inputmode = InputMode::Normal;
            }
        }
        _ => (),
    };
    return AppEvent::None;
}

fn parse_text_styles(lines: &[String]) -> Vec<TextStyle> {
    return lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            detect_facets(&line).into_iter().map(move |f| {
                let slice = CharSlice::from(line, f.index);
                let style = match f.feature {
                    FacetFeature::Mention => Style::default().italic(),
                    FacetFeature::Link => Style::default().underlined(),
                    FacetFeature::Tag => Style::default().bold(),
                };
                TextStyle {
                    start: (i, slice.char_start),
                    end: (i, slice.char_end - 1),
                    style,
                }
            })
        })
        .flatten()
        .collect();
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
        let lower_area = Rect {
            y: upper_area.y + upper_area.height + 1,
            height: 3,
            ..upper_area
        };

        let title = match (&self.focus, &self.inputmode) {
            (Focus::LangField, _) => "New Note",
            (_, InputMode::Normal) => "New Note (Normal)",
            (_, InputMode::Insert) => "New Note (Insert)",
            (_, InputMode::View) => "New Note (View)",
        };
        let text_lines = self.text_field.lines();
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
        self.text_field.set_block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title(Line::from(title).left_aligned())
                .title(Line::from(word_remaining.to_string()).right_aligned()),
        );
        self.text_field.set_focus(matches!(self.focus, Focus::TextField));
        let text_styles = parse_text_styles(self.text_field.lines());
        self.text_field.set_text_styles(text_styles);
        self.text_field.render(upper_area, buf);

        let title = match (&self.focus, &self.inputmode) {
            (Focus::TextField, _) => "Langs",
            (_, InputMode::Normal) => "Langs (Normal)",
            (_, InputMode::Insert) => "Langs (Insert)",
            (_, InputMode::View) => "Langs (View)",
        };
        self.langs_field.set_block(
            Block::bordered().border_type(BorderType::Rounded).title(title),
        );
        self.langs_field.set_focus(matches!(self.focus, Focus::LangField));
        self.langs_field.render(lower_area, buf);
    }
}
