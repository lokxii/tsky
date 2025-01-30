use std::sync::OnceLock;

use crate::{
    app::{AppEvent, EventReceiver},
    components::{
        composer::{
            embed::{Embed, EmbedState, EmbedWidget, Media},
            textarea::{Input, Key, TextStyle},
            vim::{InputMode, Vim},
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
use ratatui::{
    crossterm::event::{self, Event},
    layout::{Constraint, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Widget},
};
use regex::Regex;
use tokio::task::JoinHandle;

enum Focus {
    TextField,
    LangField,
    AttachmentField,
}

static RE_URL: OnceLock<Regex> = OnceLock::new();
static RE_ENDING_PUNCTUATION: OnceLock<Regex> = OnceLock::new();

pub struct ComposerView {
    text_field: Vim,
    lang_field: Vim,
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
        let ar = $images
            .iter()
            .map(|i| imagesize::blob_size(&i.data))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("{}", e));
        let ar = match ar {
            Ok(o) => o,
            Err(e) => {
                log::error!("Cannot get aspect ratio of image {}", e);
                return AppEvent::None;
            }
        };
        let ar = ar.into_iter().map(|ar| {
            Some(
                atrium_api::app::bsky::embed::defs::AspectRatioData {
                    height: (ar.height as u64).try_into().unwrap(),
                    width: (ar.width as u64).try_into().unwrap(),
                }
                .into(),
            )
        });

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
            .zip(ar)
            .map(|(blob, ar)| {
                atrium_api::app::bsky::embed::images::ImageData {
                    alt: String::new(),
                    aspect_ratio: ar,
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
        let text_field = Vim::new(|_| true);
        let langs_field = Vim::new(|i| {
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
            atoz(i) || esc_or_backspace(i)
        });

        ComposerView {
            text_field,
            lang_field: langs_field,
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
        let text = self.text_field.textarea.lines().join("\n");
        if text.is_empty() && matches!(self.embed.embed, Embed::None) {
            return None;
        }

        let langs = &self.lang_field.textarea.lines()[0];
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
                    return AppEvent::None;
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
                self.text_field.textarea.insert_string(s);
                self.embed_external();
            }
            Focus::LangField => {
                self.lang_field.textarea.insert_string(s);
            }
            _ => {}
        }
    }

    fn embed_external(&mut self) {
        let text = self.text_field.textarea.lines().join("\n");
        let re_url = RE_URL.get_or_init(|| {
            Regex::new(
                r"(?:^|\s|\()((?:https?:\/\/[\S]+)|(?:(?<domain>[a-z][a-z0-9]*(?:\.[a-z0-9]+)+)[\S]*))",
            )
            .expect("invalid regex")
        });
        let Some(capture) = re_url.captures(&text) else {
            return;
        };

        let m = capture.get(1).expect("invalid capture");
        let mut uri = if let Some(domain) = capture.name("domain") {
            if !psl::suffix(domain.as_str().as_bytes())
                .map_or(false, |suffix| suffix.is_known())
            {
                return;
            }
            format!("https://{}", m.as_str())
        } else {
            m.as_str().into()
        };

        let re_ep = RE_ENDING_PUNCTUATION
            .get_or_init(|| Regex::new(r"[.,;:!?]$").expect("invalid regex"));
        if re_ep.is_match(&uri) || (uri.ends_with(')') && !uri.contains('(')) {
            uri.pop();
        }

        self.embed.add_external(uri);
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

        match self.focus {
            Focus::TextField => match event.clone().into() {
                Input { key: Key::Tab, .. } => {
                    self.focus = Focus::LangField;
                    return AppEvent::None;
                }
                Input { key: Key::Enter, .. }
                    if matches!(self.text_field.mode, InputMode::Normal) =>
                {
                    if self.post_handle.is_none() {
                        self.post_handle = self.post(agent).await;
                    }
                    return AppEvent::None;
                }
                _ => return self.text_field.handle_events(event, agent).await,
            },
            Focus::LangField => match event.clone().into() {
                Input { key: Key::Tab, .. } => {
                    self.focus = Focus::AttachmentField;
                    return AppEvent::None;
                }
                Input { key: Key::Enter, .. }
                    if matches!(
                        self.lang_field.mode,
                        InputMode::Normal | InputMode::Visual
                    ) =>
                {
                    if self.post_handle.is_none() {
                        self.post_handle = self.post(agent).await;
                    }
                    return AppEvent::None;
                }
                _ => return self.lang_field.handle_events(event, agent).await,
            },
            Focus::AttachmentField => match event.clone().into() {
                Input { key: Key::Tab, .. } => {
                    self.focus = Focus::TextField;
                    return AppEvent::None;
                }
                _ => return self.embed.handle_events(event, agent).await,
            },
        }
    }
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
            PostWidget::new(post_manager!().at(&reply.parent.uri).unwrap())
                .has_border(true)
        });

        let embed = EmbedWidget::new(&self.embed)
            .focused(matches!(self.focus, Focus::AttachmentField));

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

        let title = match (&self.focus, &self.text_field.mode) {
            (Focus::LangField, _) => "New Post",
            (_, InputMode::Normal) => "New Post (Normal)",
            (_, InputMode::Insert) => "New Post (Insert)",
            (_, InputMode::Visual) => "New Post (View)",
        };
        let text_lines = self.text_field.textarea.lines();
        let word_remaining = if text_lines.len() == 0 {
            300
        } else {
            300 - text_lines
                .into_iter()
                .map(|l| l.chars().count())
                .sum::<usize>() as i64
                - text_lines.len() as i64
                + 1
        };
        self.text_field.textarea.block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Color::DarkGray)
                .title(Line::styled(title, Color::Gray).left_aligned())
                .title(
                    Line::styled(word_remaining.to_string(), Color::Gray)
                        .right_aligned(),
                ),
        );
        self.text_field
            .textarea
            .focused(matches!(self.focus, Focus::TextField));
        let text_styles = parse_text_styles(self.text_field.textarea.lines());
        self.text_field.textarea.text_styles(text_styles);
        self.text_field.textarea.render(text_area, buf);

        let title = match (&self.focus, &self.lang_field.mode) {
            (Focus::TextField, _) => "Langs",
            (_, InputMode::Normal) => "Langs (Normal)",
            (_, InputMode::Insert) => "Langs (Insert)",
            (_, InputMode::Visual) => "Langs (View)",
        };
        self.lang_field.textarea.block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Color::DarkGray)
                .title(Span::styled(title, Color::Gray)),
        );
        self.lang_field
            .textarea
            .focused(matches!(self.focus, Focus::LangField));
        self.lang_field.textarea.render(lang_area, buf);

        embed.render(embed_area, buf);
    }
}
