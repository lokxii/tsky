use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Block, Widget},
};

use crate::components::{
    actor::ActorBasicWidget,
    embed::embed_widget::EmbedWidget,
    paragraph::Paragraph,
    post::{FacetType, Post},
};

pub struct PostWidget {
    post: Post,
    style: Style,
    is_selected: bool,
    has_border: bool,
    show_author: bool,
}

impl PostWidget {
    pub fn new(post: Post) -> Self {
        PostWidget {
            post,
            style: Style::default(),
            is_selected: false,
            has_border: false,
            show_author: true,
        }
    }

    pub fn is_selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self.style = if self.is_selected {
            Style::default().bg(Color::Rgb(45, 50, 55))
        } else {
            Style::default()
        };
        self
    }

    pub fn show_author(mut self, show_author: bool) -> Self {
        self.show_author = show_author;
        self
    }

    pub fn has_border(mut self, has_border: bool) -> Self {
        self.has_border = has_border;
        self
    }

    pub fn line_count(&self, width: u16) -> u16 {
        let width = width - self.has_border as u16 * 2;
        self.show_author as u16
            + 1 // date
            + self.body_paragraph().line_count(width) as u16
            + self.post.labels.len() as u16
            + 1 // stats
            + self.post.embed.as_ref().map(|e| EmbedWidget::new(e.clone(), false).line_count(width) as u16).unwrap_or(0)
            + self.has_border as u16 * 2
    }

    fn body_paragraph(&self) -> Paragraph {
        let mut last_segment = self.post.text.as_str();
        let mut last_offset = 0;
        let mut lines = vec![Line::from("")];
        for facet in &self.post.facets {
            let (left, middle) =
                last_segment.split_at(facet.range.start - last_offset);
            let (middle, right) =
                middle.split_at(facet.range.end - facet.range.start);

            let mut left_lines = left.split('\n');
            lines.last_mut().unwrap().push_span(Span::styled(
                left_lines.next().unwrap(),
                Style::default(),
            ));
            left_lines.for_each(|line| {
                lines.push(Line::from(line));
            });

            let facet_style = match facet.r#type {
                FacetType::Mention(_) => Style::default().italic(),
                FacetType::Link(_) => Style::default().underlined(),
                FacetType::Tag => Style::default().bold(),
            };
            lines
                .last_mut()
                .unwrap()
                .push_span(Span::styled(middle, facet_style));
            last_segment = right;
            last_offset = facet.range.end;
        }

        let mut last_segment_lines = last_segment.split('\n');
        lines.last_mut().unwrap().push_span(Span::styled(
            last_segment_lines.next().unwrap(),
            Style::default(),
        ));
        last_segment_lines.for_each(|line| {
            lines.push(Line::from(line));
        });

        Paragraph::new(lines).wrap(true)
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
        let area = if self.has_border {
            let borders = Block::bordered()
                .style(self.style)
                .border_set(symbols::border::ROUNDED)
                .border_style(Color::DarkGray);
            let inner_area = borders.inner(area);
            borders.render(area, buf);
            inner_area
        } else {
            area
        };
        let post = &self.post;

        let text = self.body_paragraph();
        let embed = self
            .post
            .embed
            .clone()
            .map(|e| EmbedWidget::new(e.into(), self.is_selected));
        let labels = &self.post.labels;

        let [author_area, datetime_area, text_area, labels_area, embed_area, stats_area] =
            Layout::vertical([
                Constraint::Length(self.show_author as u16),
                Constraint::Length(1),
                Constraint::Length(text.line_count(area.width) as u16),
                Constraint::Length(labels.len() as u16),
                Constraint::Length(
                    embed
                        .as_ref()
                        .map(|e| e.line_count(area.width))
                        .unwrap_or(0),
                ),
                Constraint::Length(1),
            ])
            .areas(area);

        if self.show_author {
            ActorBasicWidget::new(&post.author).render(author_area, buf);
        }

        let delta_time = chrono::Local::now() - post.created_at;
        let weeks = delta_time.num_weeks();
        let days = delta_time.num_days();
        let hours = delta_time.num_hours();
        let mins = delta_time.num_minutes();
        if weeks > 0 {
            Line::from(format!("{}wk", weeks,))
                .style(Color::DarkGray)
                .render(datetime_area, buf);
        } else if days > 0 {
            Line::from(format!("{}d", days,))
                .style(Color::DarkGray)
                .render(datetime_area, buf);
        } else if hours > 0 {
            Line::from(format!("{}h", hours))
                .style(Color::DarkGray)
                .render(datetime_area, buf);
        } else if mins > 0 {
            Line::from(format!("{}m", mins))
                .style(Color::DarkGray)
                .render(datetime_area, buf);
        } else {
            Line::from("now").style(Color::DarkGray).render(datetime_area, buf);
        }

        self.body_paragraph().render(text_area, buf);

        let labels_subareas = (0..labels.len() as u16).map(|i| Rect {
            y: labels_area.y + i,
            height: 1,
            ..labels_area
        });
        labels.iter().zip(labels_subareas).for_each(|t| {
            Line::from(format!("[{}]", t.0))
                .style(Color::LightRed)
                .render(t.1, buf)
        });

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
            "💬{}{}",
            post.reply,
            if self.is_selected { " (u)" } else { "" }
        ))
        .style(stat_color)
        .alignment(Alignment::Left)
        .render(reply_area, buf);

        Line::from(format!(
            "❝ {}{}",
            post.quote,
            if self.is_selected { " (i)" } else { "" }
        ))
        .style(stat_color)
        .alignment(Alignment::Left)
        .render(quote_area, buf);

        Line::from(format!(
            "⭮ {}{}",
            post.repost_view.count,
            if self.is_selected { " (o)" } else { "" }
        ))
        .style(if post.repost_view.uri.is_some() {
            Color::Green
        } else {
            stat_color
        })
        .alignment(Alignment::Left)
        .render(repost_area, buf);

        Line::from(format!(
            "♡ {}{}",
            post.like_view.count,
            if self.is_selected { " (⎵)" } else { "" }
        ))
        .style(if post.like_view.uri.is_some() {
            Color::Green
        } else {
            stat_color
        })
        .alignment(Alignment::Left)
        .render(like_area, buf);

        if self.is_selected {
            Line::from("🦋(p)")
                .style(stat_color)
                .alignment(Alignment::Left)
                .render(bsky_area, buf);
        }

        embed.map(|e| e.render(embed_area, buf));
    }
}
