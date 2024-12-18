use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

use crate::components::embed::{embed_widget::EmbedWidget, Record};

pub struct RecordWidget {
    record: Record,
    style: Style,
    is_selected: bool,
}

impl RecordWidget {
    pub fn new(record: Record, is_selected: bool) -> RecordWidget {
        RecordWidget {
            record,
            style: if is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            },
            is_selected,
        }
    }

    pub fn line_count(&self, width: u16) -> u16 {
        match &self.record {
            Record::Post(post) => {
                let text_lines = Paragraph::new(
                    post.text
                        .split('\n')
                        .map(|line| Line::from(line).style(Color::White))
                        .collect::<Vec<Line>>(),
                )
                .wrap(ratatui::widgets::Wrap { trim: false })
                .line_count(width - 2) as u16;

                let media_lines = post
                    .media
                    .clone()
                    .map(|e| {
                        EmbedWidget::new(e.into(), false).line_count(width - 2)
                    })
                    .unwrap_or(0);

                media_lines + (1 + text_lines) + post.has_embed as u16 + 2
            }
            _ => 1 + 2,
        }
    }
}

impl Widget for RecordWidget {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        match self.record {
            Record::Post(post) => {
                let text = Paragraph::new(
                    post.text
                        .split('\n')
                        .map(|line| Line::from(line).style(Color::White))
                        .collect::<Vec<Line>>(),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });

                let media = post
                    .media
                    .map(|e| EmbedWidget::new(e.into(), self.is_selected));

                let [media_area, quote_area] = Layout::vertical([
                    Constraint::Length(
                        media
                            .as_ref()
                            .map(|m| m.line_count(area.width - 2))
                            .unwrap_or(0),
                    ),
                    Constraint::Length(
                        text.line_count(area.width - 2) as u16
                            + 1
                            + post.has_embed as u16
                            + 2,
                    ),
                ])
                .areas(area);

                media.map(|e| e.render(media_area, buf));

                let quote_border = Block::bordered()
                    .style(self.style)
                    .border_set(symbols::border::ROUNDED);
                let quote_inner_area = quote_border.inner(quote_area);
                quote_border.render(quote_area, buf);

                let [author_area, text_area, quote_embed_area] =
                    Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Length(
                            text.line_count(area.width - 2) as u16
                        ),
                        Constraint::Length(post.has_embed as u16),
                    ])
                    .areas(quote_inner_area);

                let author_labels =
                    post.author.labels.iter().fold(String::new(), |acc, e| {
                        format!("{} [{}]", acc, e)
                    });
                (Span::styled(post.author.name.clone(), Color::Cyan)
                    + Span::styled(
                        format!(" @{}", post.author.handle),
                        Color::Gray,
                    )
                    + Span::styled(author_labels, Color::LightRed))
                .render(author_area, buf);

                text.render(text_area, buf);
                if post.has_embed {
                    Line::from("[embed]")
                        .style(Color::DarkGray)
                        .render(quote_embed_area, buf);
                }
            }

            Record::Blocked => {
                Line::from("[blocked]").render(area, buf);
            }
            Record::NotFound => {
                Line::from("[Not found]").render(area, buf);
            }
            Record::Detached => {
                Line::from("[Detached]").render(area, buf);
            }
            Record::NotImplemented => {
                Line::from("[Not implemented]").render(area, buf);
            }
        }
    }
}
