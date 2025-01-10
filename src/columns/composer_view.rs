use crate::{
    app::{AppEvent, EventReceiver},
    components::{
        composer::{
            embed::{Embed, EmbedState, EmbedWidget, Media},
            textarea::{CursorMove, Input, Key, TextArea, TextStyle},
        },
        post::{
            facets::{detect_facets, CharSlice, FacetFeature},
            post_widget::PostWidget,
            ReplyRef,
        },
    },
    post_manager,
};
use atrium_api::{
    app::bsky::{
        embed::record_with_media::MainMediaRefs, feed::post::RecordEmbedRefs,
    },
    types::{string::Language, Union},
};
use bsky_sdk::{rich_text::RichText, BskyAgent};
use crossterm::event::{self, Event};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, Widget},
};
use tokio::task::JoinHandle;

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
    embed: EmbedState,
    post_handle: Option<JoinHandle<AppEvent>>,
}

macro_rules! create_quote_ref {
    ($post:expr) => {
        atrium_api::app::bsky::embed::record::MainData {
            record: atrium_api::com::atproto::repo::strong_ref::MainData {
                cid: $post.cid.clone(),
                uri: $post.uri.clone(),
            }
            .into(),
        }
        .into()
    };
}

macro_rules! create_image_refs {
    ($agent:expr, $images:expr) => {{
        log::info!("Uploading image");
        let h = $images
            .into_iter()
            .map(|i| $agent.api.com.atproto.repo.upload_blob(i.data.clone()));
        let blobs = match futures::future::try_join_all(h).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("Cannot upload image: {}", e);
                return AppEvent::None;
            }
        };
        let images = blobs
            .into_iter()
            .map(|blob| {
                atrium_api::app::bsky::embed::images::ImageData {
                    alt: String::new(),
                    aspect_ratio: None,
                    image: blob.data.blob,
                }
                .into()
            })
            .collect();
        atrium_api::app::bsky::embed::images::MainData { images }.into()
    }};
}

macro_rules! create_external_ref {
    ($agent:expr, $uri:expr) => {{
        log::info!("Fetching webpage");
        let text = reqwest::get($uri.clone())
            .await
            .expect("Cannot fetch page")
            .text()
            .await
            .expect("Cannot fetch text");

        let (description, title, thumb) = {
            let dom = tl::parse(&text, tl::ParserOptions::default()).unwrap();
            let parser = dom.parser();
            let meta = dom
                .query_selector("meta")
                .unwrap()
                .filter_map(|h| h.get(parser))
                .filter_map(|t| {
                    let attributes = t.as_tag()?.attributes();
                    let property = attributes.get("property")??.as_bytes();
                    let property = std::str::from_utf8(property).ok()?;
                    if !property.starts_with("og:") {
                        return None;
                    }
                    let content = attributes.get("content")??.as_bytes();
                    let content = std::str::from_utf8(content).ok()?;
                    Some((property, content))
                })
                .collect::<Vec<_>>();
            let description = meta
                .iter()
                .find(|(p, _)| *p == "og:description")
                .map(|(_, c)| c.to_string())
                .unwrap_or(String::new());
            let title = meta
                .iter()
                .find(|(p, _)| *p == "og:title")
                .map(|(_, c)| c.to_string())
                .unwrap_or(String::new());
            let thumb = meta
                .iter()
                .find(|(p, _)| *p == "og:image")
                .map(|(_, c)| c.to_string());
            (description, title, thumb)
        };
        let thumb = if let Some(thumb) = thumb {
            log::info!("Fetching thumbnail");
            let Ok(res) = reqwest::get(thumb).await else {
                log::error!("Cannot fetch image");
                return AppEvent::None;
            };
            let Ok(blob) = res.bytes().await else {
                log::error!("Cannot fetch blob");
                return AppEvent::None;
            };

            log::info!("Uploading thumbnail");
            let r#ref = $agent.api.com.atproto.repo.upload_blob(blob.to_vec());
            let blob = match r#ref.await {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Cannot upload thumbnail: {}", e);
                    return AppEvent::None;
                }
            };
            Some(blob.blob.clone())
        } else {
            None
        };

        let external = atrium_api::app::bsky::embed::external::ExternalData {
            description,
            title,
            uri: $uri,
            thumb,
        }
        .into();
        atrium_api::app::bsky::embed::external::MainData { external }.into()
    }};
}

impl ComposerView {
    pub fn new(reply: Option<ReplyRef>, embed: Embed) -> Self {
        let textarea = TextArea::from(String::new());
        let lang = TextArea::from(String::new());

        ComposerView {
            text_field: textarea,
            langs_field: lang,
            inputmode: InputMode::Insert,
            focus: Focus::TextField,
            reply,
            embed: EmbedState::new(embed),
            post_handle: None,
        }
    }

    pub async fn post_finished(&mut self) -> bool {
        if self.post_handle.is_none() {
            return false;
        }
        if self.post_handle.as_ref().map(|h| h.is_finished()).unwrap_or(false) {
            let mut handle = None;
            std::mem::swap(&mut handle, &mut self.post_handle);
            if let AppEvent::ColumnPopLayer = handle.unwrap().await.unwrap() {
                return true;
            }
        }
        return false;
    }

    async fn post(&self, agent: BskyAgent) -> Option<JoinHandle<AppEvent>> {
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
            return None;
        }
        let langs = if langs.is_empty() { None } else { Some(langs) };

        let created_at = atrium_api::types::string::Datetime::now();

        let text = self.text_field.lines().join("\n");

        let facets = match RichText::new_with_detect_facets(&text).await {
            Ok(richtext) => richtext.facets,
            Err(e) => {
                log::error!("Cannot parse richtext: {}", e);
                return None;
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

        let embed = self.embed.embed.clone();
        return Some(tokio::spawn(async move {
            let embed = match embed {
                Embed::None => None,
                Embed::Record(post) => {
                    Some(Union::Refs(RecordEmbedRefs::AppBskyEmbedRecordMain(
                        Box::new(create_quote_ref!(post)),
                    )))
                }
                Embed::Media(Media::Images(images)) => {
                    let images = create_image_refs!(agent, images);
                    Some(Union::Refs(RecordEmbedRefs::AppBskyEmbedImagesMain(
                        Box::new(images),
                    )))
                }
                Embed::Media(Media::External(uri)) => {
                    let external = create_external_ref!(agent, uri);
                    Some(Union::Refs(
                        RecordEmbedRefs::AppBskyEmbedExternalMain(Box::new(
                            external,
                        )),
                    ))
                }
                Embed::RecordWithMedia(post, Media::Images(images)) => {
                    let quote = create_quote_ref!(post);
                    let images = create_image_refs!(agent, images);
                    let media = Union::Refs(
                        MainMediaRefs::AppBskyEmbedImagesMain(Box::new(images)),
                    );
                    Some(Union::Refs(RecordEmbedRefs::AppBskyEmbedRecordWithMediaMain(Box::new(
                        atrium_api::app::bsky::embed::record_with_media::MainData {
                            media,
                            record: quote,
                        }
                        .into(),
                    ))))
                }
                Embed::RecordWithMedia(post, Media::External(uri)) => {
                    let quote = create_quote_ref!(post);
                    let external = create_external_ref!(agent, uri);
                    let media =
                        Union::Refs(MainMediaRefs::AppBskyEmbedExternalMain(
                            Box::new(external),
                        ));
                    Some(Union::Refs(RecordEmbedRefs::AppBskyEmbedRecordWithMediaMain(Box::new(
                        atrium_api::app::bsky::embed::record_with_media::MainData {
                            media,
                            record: quote,
                        }
                        .into(),
                    ))))
                }
            };

            log::info!("Posting");
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
            log::info!("Posted");
            return AppEvent::ColumnPopLayer;
        }));
    }

    fn handle_pasting(&mut self, s: String) {
        if s.is_empty() {
            self.embed.paste_image();
            return;
        }
        match self.focus {
            Focus::TextField => {
                self.text_field.insert_string(s);
            }
            Focus::LangField => {
                self.langs_field.insert_string(s);
            }
            _ => {}
        }
    }
}

impl EventReceiver for &mut ComposerView {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
    ) -> AppEvent {
        if self.post_handle.is_some() {
            return AppEvent::None;
        }

        let key = match event.clone() {
            Event::Key(key) => key,
            Event::Paste(s) => {
                self.handle_pasting(s);
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
                            if self.post_handle.is_none() {
                                self.post_handle = self.post(agent).await;
                            }
                            return AppEvent::None;
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
                Input { key: Key::Enter, .. } => match self.focus {
                    Focus::TextField | Focus::LangField => {
                        if self.post_handle.is_none() {
                            self.post_handle = self.post(agent.clone()).await;
                        }
                    }
                    _ => {}
                },
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
                self.embed.handle_events(event, agent).await
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

        let embed = EmbedWidget::new(self.embed.clone())
            .set_focus(matches!(self.focus, Focus::AttachmentField));

        let [_, area, _] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .areas(area);
        let [_, reply_post_area, connect_area, text_area, _, lang_area, _, embed_area] =
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
                Constraint::Length(embed.line_count(area.width)),
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

        embed.render(embed_area, buf);
    }
}
