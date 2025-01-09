use std::io::Read;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, BorderType, Widget},
};
use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};

use crate::{
    components::post::{post_widget::PostWidget, PostRef},
    post_manager,
};

#[derive(Clone)]
pub enum Embed {
    None,
    Image(Vec<Image>),
    Record(PostRef),
    RecordWithImage(PostRef, Vec<Image>),
}

impl Embed {
    pub fn paste_image(&mut self) {
        let Some(image) = Image::from_clipboard() else {
            return;
        };
        match self {
            Self::None => *self = Self::Image(vec![image]),
            Self::Image(images) => {
                if images.len() == 4 {
                    log::info!("Cannot embed more than 4 images")
                } else {
                    images.push(image);
                }
            }
            Self::Record(post) => {
                *self = Self::RecordWithImage(post.clone(), vec![image])
            }
            Self::RecordWithImage(_, images) => {
                if images.len() == 4 {
                    log::info!("Cannot embed more than 4 images")
                } else {
                    images.push(image);
                }
            }
        }
    }
}

pub struct EmbedWidget {
    embed: Embed,
    focused: bool,
}

impl EmbedWidget {
    pub fn new(embed: Embed) -> Self {
        EmbedWidget { embed, focused: false }
    }

    pub fn set_focus(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn line_count(&self, width: u16) -> u16 {
        let (media_height, post_height) = match &self.embed {
            Embed::None => (1, 0),
            Embed::Image(images) => (images.len().clamp(1, 4), 0),
            Embed::Record(post) => (
                1,
                PostWidget::new(
                    post_manager!().at(&post.uri).unwrap(),
                    false,
                    true,
                )
                .line_count(width),
            ),
            Embed::RecordWithImage(post, images) => (
                images.len().clamp(1, 4),
                PostWidget::new(
                    post_manager!().at(&post.uri).unwrap(),
                    false,
                    true,
                )
                .line_count(width),
            ),
        };
        return 2 + media_height as u16 + 1 + post_height;
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
        let (media_height, post_height) = match &self.embed {
            Embed::None => (1, 0),
            Embed::Image(images) => (images.len().clamp(1, 4), 0),
            Embed::Record(post) => (
                1,
                PostWidget::new(
                    post_manager!().at(&post.uri).unwrap(),
                    false,
                    true,
                )
                .line_count(area.width),
            ),
            Embed::RecordWithImage(post, images) => (
                images.len().clamp(1, 4),
                PostWidget::new(
                    post_manager!().at(&post.uri).unwrap(),
                    false,
                    true,
                )
                .line_count(area.width),
            ),
        };
        let [media_area, _, quote_area] = Layout::vertical([
            Constraint::Length(2 + media_height as u16),
            Constraint::Length(1),
            Constraint::Length(post_height),
        ])
        .areas(area);

        let (images, quote) = match &self.embed {
            Embed::None => (None, None),
            Embed::Image(images) => {
                (if images.len() == 0 { None } else { Some(images) }, None)
            }
            Embed::Record(post) => (None, Some(post)),
            Embed::RecordWithImage(post, images) => (
                if images.len() == 0 { None } else { Some(images) },
                Some(post),
            ),
        };

        let media_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(if self.focused { "Add media" } else { "Media" });
        let media_inner = media_block.inner(media_area);
        media_block.render(media_area, buf);
        if let Some(images) = images {
            images.iter().enumerate().for_each(|(i, image)| {
                Line::from(image.name.as_str()).render(
                    Rect {
                        y: media_inner.y + i as u16,
                        height: 1,
                        ..media_inner
                    },
                    buf,
                );
            })
        } else {
            Line::from("(Open file picker)").render(media_inner, buf);
        }

        if let Some(quote) = quote {
            PostWidget::new(
                post_manager!().at(&quote.uri).unwrap(),
                false,
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
    pub fn from_clipboard() -> Option<Image> {
        let mime_types =
            paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified)
                .expect("Cannot access clipboard");
        let accepted_types =
            ["image/jpeg", "image/png", "image/webp", "image/bmp"];
        let Some(mime) =
            accepted_types.iter().find(|t| mime_types.contains(**t))
        else {
            log::error!("No supported images found in clipboard");
            return None;
        };
        let content = paste::get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific(mime),
        );
        match content {
            Ok((mut pipe, _)) => {
                let mut data = vec![];
                pipe.read_to_end(&mut data).expect("Cannot read clipboard");
                return Some(Image { name: String::from("clipboard"), data });
            }
            Err(paste::Error::NoSeats)
            | Err(paste::Error::ClipboardEmpty)
            | Err(paste::Error::NoMimeType) => return None,
            Err(e) => {
                log::error!("{}", e);
                return None;
            }
        }
    }
}
