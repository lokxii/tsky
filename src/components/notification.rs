use atrium_api::{
    app::bsky::notification::list_notifications::NotificationData,
    types::TryFromUnknown,
};
use chrono::{DateTime, FixedOffset, Local};
use ratatui::{
    layout::{Constraint, Layout},
    style::Color,
    text::{Line, Span},
    widgets::{Block, Widget},
};

use crate::{
    components::{actor::Actor, post::post_widget::PostWidget},
    post_manager,
};

use super::separation::Separation;

type Subject = String;
type PostUri = String;

#[derive(Clone)]
pub enum Record {
    Like(Subject),
    Repost(Subject),
    Reply(PostUri),
    Mention(PostUri),
    Quote(PostUri),
    Follow,
}

impl Record {
    fn new(
        uri: String,
        reason: String,
        record: atrium_api::types::Unknown,
    ) -> Result<Record, String> {
        use atrium_api::app::bsky::feed::{like, repost};

        match reason.as_str() {
            "like" => {
                let r = like::RecordData::try_from_unknown(record)
                    .map_err(|e| e.to_string())?;
                return Ok(Record::Like(r.subject.uri.clone()));
            }
            "repost" => {
                let r = repost::RecordData::try_from_unknown(record)
                    .map_err(|e| e.to_string())?;
                return Ok(Record::Repost(r.subject.uri.clone()));
            }
            "reply" => return Ok(Record::Reply(uri)),
            "mention" => return Ok(Record::Mention(uri)),
            "quote" => return Ok(Record::Quote(uri)),
            "follow" => return Ok(Record::Follow),
            _ => return Err("Unknown notification reason".to_string()),
        }
    }
}

#[derive(Clone)]
pub struct Notification {
    pub uri: String,
    pub author: Actor,
    pub record: Record,
    pub is_read: bool,
    pub indexed_at: DateTime<FixedOffset>,
}

impl Notification {
    pub fn new(data: NotificationData) -> Result<Self, String> {
        let NotificationData {
            uri,
            author,
            indexed_at,
            is_read,
            reason,
            record,
            ..
        } = data;

        let author = Actor::new(author.data);
        let record = Record::new(uri.clone(), reason, record)?;
        let indexed_at = {
            let indexed_at = indexed_at.as_str();
            let dt = Local::now();
            let indexed_at_utc =
                DateTime::parse_from_rfc3339(indexed_at).unwrap().naive_local();
            DateTime::from_naive_utc_and_offset(indexed_at_utc, *dt.offset())
        };

        Ok(Notification { uri, author, record, is_read, indexed_at })
    }
}

impl PartialEq for Notification {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
    }
}

impl Eq for Notification {}

#[derive(Clone)]
pub struct NotificationWidget<'a> {
    notif: &'a Notification,
    block: Option<Block<'a>>,
    focused: bool,
}

impl<'a> NotificationWidget<'a> {
    pub fn new(notif: &'a Notification) -> Self {
        Self { notif, block: None, focused: false }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn line_count(&self, width: u16) -> u16 {
        let bh = self.block.is_some() as u16 * 2;
        let width = width - bh;
        match &self.notif.record {
            Record::Like(subject) => {
                let post = post_manager!().at(&subject).unwrap();
                PostWidget::new(post).show_author(false).line_count(width)
                    + bh
                    + 2
            }
            Record::Repost(subject) => {
                let post = post_manager!().at(&subject).unwrap();
                PostWidget::new(post).show_author(false).line_count(width)
                    + bh
                    + 2
            }
            Record::Reply(post_uri) => {
                let post = post_manager!().at(&post_uri).unwrap();
                PostWidget::new(post).show_author(false).line_count(width)
                    + bh
                    + 2
            }
            Record::Mention(post_uri) => {
                let post = post_manager!().at(&post_uri).unwrap();
                PostWidget::new(post).line_count(width) + bh
            }
            Record::Quote(post_uri) => {
                let post = post_manager!().at(&post_uri).unwrap();
                PostWidget::new(post).line_count(width) + bh
            }
            Record::Follow => 1 + bh,
        }
    }
}

impl<'a> Widget for NotificationWidget<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let area = if let Some(block) = self.block {
            let block = if !self.notif.is_read {
                block.border_style(Color::LightBlue)
            } else {
                block
            };
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };
        match &self.notif.record {
            Record::Like(subject) => {
                let [reason_area, separation_area, post_area] =
                    Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Fill(1),
                    ])
                    .areas(area);

                Line::from(vec![
                    Span::styled(&self.notif.author.basic.name, Color::Cyan),
                    Span::styled(
                        if self.focused { " (A)" } else { "" },
                        Color::DarkGray,
                    ),
                    Span::styled(" ♡ liked", Color::Green),
                    Span::styled(" your post", Color::Gray),
                ])
                .render(reason_area, buf);

                Separation::default().render(separation_area, buf);

                let post = post_manager!().at(&subject).unwrap();
                PostWidget::new(post).show_author(false).render(post_area, buf);
            }
            Record::Repost(subject) => {
                let [reason_area, separation_area, post_area] =
                    Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Fill(1),
                    ])
                    .areas(area);

                Line::from(vec![
                    Span::styled(&self.notif.author.basic.name, Color::Cyan),
                    Span::styled(
                        if self.focused { " (A)" } else { "" },
                        Color::DarkGray,
                    ),
                    Span::styled(" ⭮ reposted", Color::Green),
                    Span::styled(" your post", Color::Gray),
                ])
                .render(reason_area, buf);

                Separation::default().render(separation_area, buf);

                let post = post_manager!().at(&subject).unwrap();
                PostWidget::new(post).show_author(false).render(post_area, buf);
            }
            Record::Reply(post_uri) => {
                let [reason_area, separation_area, post_area] =
                    Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Fill(1),
                    ])
                    .areas(area);

                Line::from(vec![
                    Span::styled(&self.notif.author.basic.name, Color::Cyan),
                    Span::styled(
                        if self.focused { " (A)" } else { "" },
                        Color::DarkGray,
                    ),
                    Span::styled(" 💬replied", Color::Green),
                    Span::styled(" to your post", Color::Gray),
                ])
                .render(reason_area, buf);

                Separation::default().render(separation_area, buf);

                let post = post_manager!().at(&post_uri).unwrap();
                PostWidget::new(post).show_author(false).render(post_area, buf);
            }
            Record::Mention(post_uri) => {
                let post = post_manager!().at(&post_uri).unwrap();
                PostWidget::new(post).render(area, buf);
            }
            Record::Quote(post_uri) => {
                let post = post_manager!().at(&post_uri).unwrap();
                PostWidget::new(post).render(area, buf);
            }
            Record::Follow => {
                Line::from(vec![
                    Span::styled(&self.notif.author.basic.name, Color::Cyan),
                    Span::styled(
                        if self.focused { "(A)" } else { "" },
                        Color::DarkGray,
                    ),
                    Span::styled(" followed", Color::Green),
                    Span::styled(" you", Color::Gray),
                ])
                .render(area, buf);
            }
        }
    }
}
