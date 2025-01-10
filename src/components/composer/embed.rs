use std::{io::Read, process::Stdio};

use bsky_sdk::BskyAgent;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Widget},
};
use tokio::{fs::File, io::AsyncReadExt, process::Command};
use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};

use crate::{
    app::{AppEvent, EventReceiver},
    columns::{thread_view::ThreadView, Column},
    components::{
        composer::textarea::{Input, Key},
        post::{post_widget::PostWidget, PostRef},
    },
    post_manager,
};

#[derive(Clone)]
pub enum Embed {
    None,
    Media(Media),
    Record(PostRef),
    RecordWithMedia(PostRef, Media),
}

#[derive(Clone)]
pub enum Media {
    Images(Vec<Image>),
    External(String),
}

struct MediaWidget<'a> {
    media: &'a Media,
    block: Option<Block<'a>>,
    focused: Option<usize>,
}

impl<'a> MediaWidget<'a> {
    fn new(media: &'a Media) -> Self {
        MediaWidget { media, block: None, focused: None }
    }

    fn set_block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    fn set_focus(mut self, focus: usize) -> Self {
        self.focused = Some(focus);
        self
    }
}

impl<'a> Widget for MediaWidget<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let area = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        match self.media {
            Media::Images(images) => {
                images.iter().enumerate().for_each(|(i, image)| {
                    #[rustfmt::skip]
                    let style = if self.focused.map(|f| i == f).unwrap_or(false) {
                        Style::default().bg(Color::Rgb(45, 50, 55))
                    } else {
                        Style::default()
                    };

                    Line::styled(format!("Image: {}", image.name.as_str()), style).render(
                        Rect { y: area.y + i as u16, height: 1, ..area },
                        buf,
                    );
                })
            }
            Media::External(uri) => {
                let style = if self.focused.is_some() {
                    Style::default().bg(Color::Rgb(45, 50, 55))
                } else {
                    Style::default()
                };
                Line::styled(format!("Link: {}", uri), style).render(area, buf);
            }
        }
    }
}

#[derive(Clone)]
pub struct EmbedState {
    pub embed: Embed,
    pub state: usize,
}

impl EmbedState {
    pub fn new(embed: Embed) -> Self {
        EmbedState { embed, state: 0 }
    }

    pub fn paste_image(&mut self) {
        let image = match Image::from_clipboard() {
            Ok(image) => image,
            Err(e) => {
                log::error!("{}", e);
                return;
            }
        };
        self.add_image(image);
    }

    fn add_image(&mut self, image: Image) {
        match &mut self.embed {
            Embed::None => {
                self.embed = Embed::Media(Media::Images(vec![image]));
                self.state = 0;
            }
            Embed::Media(Media::Images(images)) => {
                if images.len() == 4 {
                    log::info!("Cannot embed more than 4 images")
                } else {
                    images.push(image);
                }
            }
            Embed::Media(Media::External(_)) => {
                log::info!("Media already exists");
            }
            Embed::Record(post) => {
                self.embed = Embed::RecordWithMedia(
                    post.clone(),
                    Media::Images(vec![image]),
                );
                self.state = 0;
            }
            Embed::RecordWithMedia(_, Media::Images(images)) => {
                if images.len() == 4 {
                    log::info!("Cannot embed more than 4 images")
                } else {
                    images.push(image);
                }
            }
            Embed::RecordWithMedia(_, Media::External(_)) => {
                log::info!("Media already exists");
            }
        }
    }

    pub fn add_external(&mut self, external: String) {
        match &mut self.embed {
            Embed::None => {
                self.embed = Embed::Media(Media::External(external));
                self.state = 0;
            }
            Embed::Media(_) | Embed::RecordWithMedia(_, _) => {
                log::info!("Media already exists");
            }
            Embed::Record(post) => {
                self.embed = Embed::RecordWithMedia(
                    post.clone(),
                    Media::External(external),
                );
                self.state = 0;
            }
        }
    }
}

impl EventReceiver for &mut EmbedState {
    async fn handle_events(self, event: Event, agent: BskyAgent) -> AppEvent {
        let Event::Key(key) = event else {
            return AppEvent::None;
        };
        match key.code {
            KeyCode::Backspace => {
                return AppEvent::ColumnPopLayer;
            }
            KeyCode::Char('j') => {
                self.state += 1;
                self.state = match &self.embed {
                    Embed::None => 0,
                    Embed::Media(Media::Images(images)) => {
                        self.state.clamp(0, images.len() - 1)
                    }
                    Embed::Media(Media::External(_)) => 0,
                    Embed::Record(_) => self.state.clamp(0, 1),
                    Embed::RecordWithMedia(_, Media::Images(images)) => {
                        self.state.clamp(0, images.len())
                    }
                    Embed::RecordWithMedia(_, Media::External(_)) => 1,
                };
            }
            KeyCode::Char('k') => {
                self.state = self.state.saturating_sub(1);
            }

            KeyCode::Enter => {
                let post = match &self.embed {
                    Embed::Record(post)
                    | Embed::RecordWithMedia(post, Media::External(_))
                        if self.state == 1 =>
                    {
                        Some(post)
                    }
                    Embed::RecordWithMedia(post, Media::Images(images))
                        if self.state == images.len() =>
                    {
                        Some(post)
                    }
                    _ => None,
                };
                if let Some(post) = post {
                    let uri = post.uri.clone();
                    let view = match ThreadView::from_uri(uri, agent).await {
                        Ok(thread_view) => thread_view,
                        Err(e) => {
                            log::error!("{}", e);
                            return AppEvent::None;
                        }
                    };
                    return AppEvent::ColumnNewLayer(Column::Thread(view));
                }

                #[rustfmt::skip]
                let should_fetch_image =
                    matches!(&self.embed, Embed::None | Embed::Record(_)) ||
                    matches!(&self.embed, Embed::Media(Media::Images(images)) | Embed::RecordWithMedia(_, Media::Images(images)) if images.len() < 4);
                if !should_fetch_image {
                    return AppEvent::None;
                }

                let path = match file_picker().await {
                    Ok(Some(path)) => path,
                    Ok(None) => {
                        return AppEvent::None;
                    }
                    Err(e) => {
                        log::error!("{}", e);
                        return AppEvent::None;
                    }
                };
                let image = match Image::from_path(path).await {
                    Ok(image) => image,
                    Err(e) => {
                        log::error!("{}", e);
                        return AppEvent::None;
                    }
                };
                self.add_image(image);
                return AppEvent::None;
            }

            KeyCode::Char('d') => {
                let e = event::read().expect("Cannot read event").into();
                let Input { key: Key::Char('d'), .. } = e else {
                    return AppEvent::None;
                };
                let mut embed = Embed::None;
                std::mem::swap(&mut embed, &mut self.embed);
                self.embed = match embed {
                    Embed::None => Embed::None,
                    Embed::Media(Media::Images(mut images)) => {
                        self.state = self.state.saturating_sub(1);
                        images.remove(self.state);
                        if images.is_empty() {
                            Embed::None
                        } else {
                            Embed::Media(Media::Images(images))
                        }
                    }
                    Embed::Media(Media::External(_)) => Embed::None,
                    Embed::Record(post) => {
                        if self.state == 0 {
                            Embed::Record(post)
                        } else {
                            self.state = 0;
                            Embed::None
                        }
                    }
                    Embed::RecordWithMedia(post, Media::Images(mut images)) => {
                        if self.state == images.len() {
                            self.state = self.state.saturating_sub(1);
                            Embed::Media(Media::Images(images))
                        } else {
                            images.remove(self.state);
                            self.state = self.state.saturating_sub(1);
                            if images.is_empty() {
                                Embed::Record(post)
                            } else {
                                Embed::RecordWithMedia(
                                    post,
                                    Media::Images(images),
                                )
                            }
                        }
                    }
                    Embed::RecordWithMedia(post, Media::External(uri)) => {
                        if self.state == 1 {
                            self.state = 0;
                            Embed::Media(Media::External(uri))
                        } else {
                            self.state = 0;
                            Embed::Record(post)
                        }
                    }
                };
            }

            _ => {
                let post = match &self.embed {
                    Embed::Record(post) if self.state == 1 => Some(post),
                    Embed::RecordWithMedia(post, _) if self.state == 1 => {
                        Some(post)
                    }
                    _ => None,
                };
                if let Some(post) = post {
                    return post_manager!()
                        .at(&post.uri)
                        .unwrap()
                        .handle_events(event, agent)
                        .await;
                }
            }
        }

        return AppEvent::None;
    }
}

async fn file_picker() -> Result<Option<std::path::PathBuf>, String> {
    let child = Command::new("zenity")
        .arg("--file-selection")
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(e) => {
            return Err(format!("Cannot spawn zenity (file selector): {}", e));
        }
    };
    let out = match child.wait_with_output().await {
        Ok(out) => out.stdout,
        Err(e) => return Err(format!("Cannot read output from zenity: {}", e)),
    };
    if out.is_empty() {
        return Ok(None);
    }
    let path = match std::str::from_utf8(&out) {
        Ok(path) => path.strip_suffix('\n').unwrap(),
        Err(e) => {
            return Err(format!("Malformed utf8 path: {}", e));
        }
    };
    Ok(Some(std::path::PathBuf::from(path)))
}

pub struct EmbedWidget {
    embed: EmbedState,
    focused: bool,
}

impl EmbedWidget {
    pub fn new(embed: EmbedState) -> Self {
        EmbedWidget { embed, focused: false }
    }

    pub fn set_focus(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn line_count(&self, width: u16) -> u16 {
        let (media_height, post_height) = self.media_record_height(width);
        return 2 + media_height + 1 + post_height;
    }

    fn media_record_height(&self, width: u16) -> (u16, u16) {
        match &self.embed.embed {
            Embed::None => (1, 0),
            Embed::Media(Media::Images(images)) => {
                (images.len().clamp(1, 4) as u16, 0)
            }
            Embed::Media(Media::External(_)) => (1, 0),
            Embed::Record(post) => (
                1,
                PostWidget::new(
                    post_manager!().at(&post.uri).unwrap(),
                    false,
                    true,
                )
                .line_count(width),
            ),
            Embed::RecordWithMedia(post, Media::Images(images)) => (
                images.len().clamp(1, 4) as u16,
                PostWidget::new(
                    post_manager!().at(&post.uri).unwrap(),
                    false,
                    true,
                )
                .line_count(width),
            ),
            Embed::RecordWithMedia(post, Media::External(_)) => (
                1,
                PostWidget::new(
                    post_manager!().at(&post.uri).unwrap(),
                    false,
                    true,
                )
                .line_count(width),
            ),
        }
    }
}

impl Widget for EmbedWidget {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let (media_height, post_height) = self.media_record_height(area.width);
        let [media_area, _, quote_area] = Layout::vertical([
            Constraint::Length(2 + media_height as u16),
            Constraint::Length(1),
            Constraint::Length(post_height),
        ])
        .areas(area);

        let (media, quote) = match &self.embed.embed {
            Embed::None => (None, None),
            Embed::Media(media) => (Some(media), None),
            Embed::Record(post) => (None, Some(post)),
            Embed::RecordWithMedia(post, media) => (Some(media), Some(post)),
        };

        let media_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(if self.focused { "Add media" } else { "Media" });

        if let Some(media) = media {
            MediaWidget::new(&media)
                .set_block(media_block)
                .render(media_area, buf);
        } else {
            let media_inner = media_block.inner(media_area);
            media_block.render(media_area, buf);
            let style = if self.embed.state == 0 && self.focused {
                Style::default().bg(Color::Rgb(45, 40, 44))
            } else {
                Style::default()
            };
            Line::styled("(Open file picker)", style).render(media_inner, buf);
        }

        if let Some(quote) = quote {
            let is_selected = match &self.embed.embed {
                Embed::None
                | Embed::Record(_)
                | Embed::Media(Media::External(_))
                | Embed::RecordWithMedia(_, Media::External(_)) => {
                    self.embed.state == 1
                }
                Embed::Media(Media::Images(images))
                | Embed::RecordWithMedia(_, Media::Images(images)) => {
                    self.embed.state == images.len()
                }
            };
            PostWidget::new(
                post_manager!().at(&quote.uri).unwrap(),
                is_selected,
                true,
            )
            .render(quote_area, buf);
        }
    }
}

#[derive(Clone)]
pub struct Image {
    pub name: String,
    pub data: Vec<u8>,
}

impl Image {
    pub fn from_clipboard() -> Result<Image, String> {
        let mime_types = match paste::get_mime_types(
            ClipboardType::Regular,
            Seat::Unspecified,
        ) {
            Ok(m) => m,
            Err(e) => {
                return Err(format!("Cannot get clipboard mime type: {}", e))
            }
        };
        let accepted_types =
            ["image/jpeg", "image/png", "image/webp", "image/bmp"];
        let mime = accepted_types.iter().find(|t| mime_types.contains(**t));
        let Some(mime) = mime else {
            return Err("No supported images found in clipboard".to_string());
        };
        let content = paste::get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific(mime),
        );
        match content {
            Ok((mut pipe, _)) => {
                let mut data = vec![];
                if let Some(e) = pipe.read_to_end(&mut data).err() {
                    return Err(format!("Cannot read from clipboard: {}", e));
                }
                return Ok(Image { name: String::from("clipboard"), data });
            }
            Err(paste::Error::NoSeats)
            | Err(paste::Error::ClipboardEmpty)
            | Err(paste::Error::NoMimeType) => {
                return Err("Empty clipboard".to_string())
            }
            Err(e) => {
                return Err(format!("Cannot paste from clipboard: {}", e));
            }
        }
    }

    pub async fn from_path(path: std::path::PathBuf) -> Result<Image, String> {
        let accepted_types =
            ["image/jpeg", "image/png", "image/webp", "image/bmp"];

        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let mut file = match File::open(path).await {
            Ok(file) => file,
            Err(e) => return Err(format!("Cannot open file: {}", e)),
        };
        let mut data = vec![];
        if let Some(e) = file.read_to_end(&mut data).await.err() {
            return Err(format!("Cannot read from file: {}", e));
        }

        let mime = tree_magic::from_u8(&data);
        if !accepted_types.contains(&mime.as_str()) {
            return Err("Filetype not supported".to_string());
        };

        return Ok(Image { data, name });
    }
}
