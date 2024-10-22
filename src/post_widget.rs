use ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

use crate::{
    embed_widget::EmbedWidget,
    post::{Post, Reply},
};

pub struct PostWidget {
    pub post: Post,
    pub style: Style,
    pub is_selected: bool,
}

impl PostWidget {
    pub fn new(post: Post, is_selected: bool) -> PostWidget {
        PostWidget {
            post,
            style: if is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            },
            is_selected,
        }
    }

    pub fn line_count(&self, width: u16) -> u16 {
        self.post.reason.is_some() as u16
            + self.post.reply_to.is_some() as u16
            + 2 // author and date
            + self.body_paragraph().line_count(width) as u16
            + 1 // stats
            + self.post.embed.clone().map(|e| EmbedWidget::new(e, false).line_count(width) as u16).unwrap_or(0)
            + 2 // borders
    }

    pub fn body_paragraph(&self) -> Paragraph {
        Paragraph::new(
            self.post
                .text
                .split('\n')
                .map(|line| Line::from(line).style(Color::White))
                .collect::<Vec<Line>>(),
        )
        .wrap(ratatui::widgets::Wrap { trim: true })
    }
}

impl Widget for PostWidget {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let borders = Block::bordered()
            .style(self.style)
            .border_set(symbols::border::ROUNDED);
        let inner_area = borders.inner(area);
        let post = &self.post;

        borders.render(area, buf);

        let text = self.body_paragraph();
        let embed = self
            .post
            .embed
            .clone()
            .map(|e| EmbedWidget::new(e.into(), self.is_selected));

        let [top_area, author_area, datetime_area, text_area, embed_area, stats_area] =
            Layout::vertical([
                Constraint::Length(
                    self.post.reason.is_some() as u16
                        + self.post.reply_to.is_some() as u16,
                ),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(text.line_count(inner_area.width) as u16),
                Constraint::Length(
                    embed
                        .as_ref()
                        .map(|e| e.line_count(inner_area.width))
                        .unwrap_or(0),
                ),
                Constraint::Length(1),
            ])
            .areas(inner_area);

        let [repost_area, reply_area] = Layout::vertical([
            Constraint::Length(self.post.reason.is_some() as u16),
            Constraint::Length(self.post.reply_to.is_some() as u16),
        ])
        .areas(top_area);

        if let Some(repost) = &self.post.reason {
            Line::from(Span::styled(
                format!("⭮ Reposted by {}", repost.author),
                Color::Green,
            ))
            .render(repost_area, buf);
        }

        if let Some(reply_to) = &self.post.reply_to {
            let reply_to = match reply_to {
                Reply::Author(a) => &a.author,
                Reply::DeletedPost => "[deleted post]",
                Reply::BlockedUser => "[blocked user]",
            };
            Line::from(Span::styled(
                format!("⮡ Reply to {}", reply_to),
                Color::Rgb(180, 180, 180),
            ))
            .render(reply_area, buf);
        }

        Line::from(
            Span::styled(post.author.clone(), Color::Cyan)
                + Span::styled(format!(" @{}", post.handle), Color::Gray),
        )
        .render(author_area, buf);

        Line::from(post.created_at.to_string())
            .style(Color::DarkGray)
            .render(datetime_area, buf);

        self.body_paragraph().render(text_area, buf);

        let [reply_area, quote_area, repost_area, like_area, bsky_area] =
            Layout::horizontal([
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .areas(stats_area);

        let stat_color = Color::Rgb(130, 130, 130);

        Line::from(format!(
            "{} {}",
            post.reply,
            if post.reply == 1 { "reply" } else { "replies" }
        ))
        .style(stat_color)
        .alignment(Alignment::Left)
        .render(reply_area, buf);

        Line::from(format!(
            "{} {}",
            post.quote,
            if post.quote == 1 { "quote" } else { "quotes" }
        ))
        .style(stat_color)
        .alignment(Alignment::Left)
        .render(quote_area, buf);

        Line::from(format!(
            "{} {}{}",
            post.repost.count,
            if post.repost.count == 1 { "repost" } else { "reposts" },
            if self.is_selected { " (o)" } else { "" }
        ))
        .style(if post.repost.uri.is_some() {
            Color::Green
        } else {
            stat_color
        })
        .alignment(Alignment::Left)
        .render(repost_area, buf);

        Line::from(format!(
            "{} {}{}",
            post.like.count,
            if post.like.count == 1 { "like" } else { "likes" },
            if self.is_selected { " (space)" } else { "" }
        ))
        .style(if post.like.uri.is_some() { Color::Green } else { stat_color })
        .alignment(Alignment::Left)
        .render(like_area, buf);

        if self.is_selected {
            Line::from("🦋 (p)")
                .style(stat_color)
                .alignment(Alignment::Left)
                .render(bsky_area, buf);
        }

        embed.map(|e| e.render(embed_area, buf));
    }
}
