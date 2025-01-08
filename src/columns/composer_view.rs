use crate::{
    app::{AppEvent, EventReceiver},
    components::{
        post::{
            facets::{detect_facets, CharSlice, FacetFeature},
            post_widget::PostWidget,
            PostRef, ReplyRef,
        },
        textarea::{CursorMove, Input, Key, TextArea, TextStyle},
    },
    post_manager,
};
use atrium_api::types::string::Language;
use bsky_sdk::{rich_text::RichText, BskyAgent};
use crossterm::event::{self, Event};
use ratatui::{
    layout::{Constraint, Layout},
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
    AttachmentField,
}

pub struct ComposerView {
    text_field: TextArea,
    langs_field: TextArea,
    inputmode: InputMode,
    focus: Focus,
    reply: Option<ReplyRef>,
    embed: Option<PostRef>,
}

impl ComposerView {
    pub fn new(reply: Option<ReplyRef>, embed: Option<PostRef>) -> Self {
        let textarea = TextArea::from(String::new());
        let lang = TextArea::from(String::new());

        ComposerView {
            text_field: textarea,
            langs_field: lang,
            inputmode: InputMode::Insert,
            focus: Focus::TextField,
            reply,
            embed,
        }
    }

    async fn post(&self, agent: BskyAgent) -> AppEvent {
        let langs = &self.langs_field.lines()[0];
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
        let langs = if langs.is_empty() { None } else { Some(langs) };

        let created_at = atrium_api::types::string::Datetime::now();

        let text = self.text_field.lines().join("\n");

        let facets = match RichText::new_with_detect_facets(&text).await {
            Ok(richtext) => richtext.facets,
            Err(e) => {
                log::error!("Cannot parse richtext: {}", e);
                return AppEvent::None;
            }
        };

        let reply = self.reply.as_ref().map(|reply| {
            atrium_api::app::bsky::feed::post::ReplyRefData {
                root: atrium_api::com::atproto::repo::strong_ref::MainData {
                    cid: reply.root.cid.clone(),
                    uri: reply.root.uri.clone(),
                }
                .into(),
                parent: atrium_api::com::atproto::repo::strong_ref::MainData {
                    cid: reply.parent.cid.clone(),
                    uri: reply.parent.uri.clone(),
                }
                .into(),
            }
            .into()
        });

        let embed = self.embed.as_ref().map(|post| {
            atrium_api::types::Union::Refs(atrium_api::app::bsky::feed::post::RecordEmbedRefs::AppBskyEmbedRecordMain(
                Box::new(atrium_api::app::bsky::embed::record::MainData {
                    record: atrium_api::com::atproto::repo::strong_ref::MainData {
                        cid: post.cid.clone(),
                        uri: post.uri.clone(),
                    }.into()
                }.into())
            ))
        });

        let r = agent
            .create_record(atrium_api::app::bsky::feed::post::RecordData {
                created_at,
                embed,
                entities: None,
                facets,
                labels: None,
                langs,
                reply,
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
}

impl EventReceiver for &mut ComposerView {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
    ) -> AppEvent {
        let key = match event.clone() {
            Event::Key(key) => key,
            Event::Paste(_) => {
                log::info!("pasted from clipboard");
                return AppEvent::None;
            }
            _ => return AppEvent::None,
        };
        if key.kind != event::KeyEventKind::Press {
            return AppEvent::None;
        }

        match self.inputmode {
            InputMode::Insert => match event.clone().into() {
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
                        _ => {}
                    }
                    return AppEvent::None;
                }
                Input { key: Key::Tab, .. } => {
                    match self.focus {
                        Focus::TextField => self.focus = Focus::LangField,
                        Focus::LangField => self.focus = Focus::AttachmentField,
                        Focus::AttachmentField => self.focus = Focus::TextField,
                    };
                    return AppEvent::None;
                }
                input => match self.focus {
                    Focus::TextField => {
                        self.text_field.input(input, |_| true);
                        return AppEvent::None;
                    }
                    Focus::LangField => {
                        if matches!(input, Input { key: Key::Enter, .. }) {
                            return self.post(agent).await;
                        }
                        let atoz = |i| {
                            matches!(i, Input { key: Key::Char(c), .. }
                            if ('a'..='z').contains(&c) || c == ',')
                        };
                        let esc_or_backspace = |i| {
                            matches!(
                                i,
                                Input { key: Key::Esc, .. }
                                    | Input { key: Key::Backspace, .. }
                            )
                        };
                        self.langs_field
                            .input(input, |i| atoz(i) || esc_or_backspace(i));
                        return AppEvent::None;
                    }
                    Focus::AttachmentField => {}
                },
            },
            InputMode::Normal => match event.clone().into() {
                Input { key: Key::Enter, .. } => return self.post(agent).await,
                Input { key: Key::Tab, .. } => match self.focus {
                    Focus::TextField => self.focus = Focus::LangField,
                    Focus::LangField => self.focus = Focus::AttachmentField,
                    Focus::AttachmentField => self.focus = Focus::TextField,
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
            Focus::AttachmentField => {
                todo!();
            }
        };
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
        let reply_post = self.reply.as_ref().map(|reply| {
            PostWidget::new(
                post_manager!().at(&reply.parent.uri).unwrap(),
                false,
                true,
            )
        });
        let quote_post = self.embed.as_ref().map(|post| {
            PostWidget::new(post_manager!().at(&post.uri).unwrap(), false, true)
        });

        let [_, area, _] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .areas(area);
        let [_, reply_post_area, connect_area, text_area, _, lang_area, _, attachment_area, _, quote_area] =
            Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(if let Some(p) = &reply_post {
                    p.line_count(area.width)
                } else {
                    0
                }),
                Constraint::Length(1),
                Constraint::Max(10),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(if let Some(p) = &quote_post {
                    p.line_count(area.width)
                } else {
                    0
                }),
            ])
            .areas(area);

        if let Some(p) = reply_post {
            p.render(reply_post_area, buf);
            Line::from("  │").render(connect_area, buf);
        }

        let title = match (&self.focus, &self.inputmode) {
            (Focus::LangField, _) => "New Post",
            (_, InputMode::Normal) => "New Post (Normal)",
            (_, InputMode::Insert) => "New Post (Insert)",
            (_, InputMode::View) => "New Post (View)",
        };
        let text_lines = self.text_field.lines();
        let word_remaining = if text_lines.len() == 0 {
            300
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
        self.text_field.render(text_area, buf);

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
        self.langs_field.render(lang_area, buf);

        let title = match &self.focus {
            Focus::AttachmentField => "Add attachments",
            _ => "Attachments",
        };
        let block = Block::bordered().title(title);
        let attachment_inner = block.inner(attachment_area);
        block.render(attachment_area, buf);
        Line::from("(Open file picker)").render(attachment_inner, buf);

        if let Some(p) = quote_post {
            p.render(quote_area, buf);
        }
    }
}
