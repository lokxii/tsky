use ratatui::{
    style::{Color, Modifier, Style},
    symbols,
    text::Line,
    widgets::{Block, Widget},
};

use crate::components::{
    embed::{record_widget::RecordWidget, Embed},
    paragraph::Paragraph,
};

pub struct EmbedWidget {
    embed: Embed,
    style: Style,
    is_selected: bool,
}

impl EmbedWidget {
    pub fn new(embed: Embed, is_selected: bool) -> EmbedWidget {
        EmbedWidget {
            embed,
            style: if is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            },
            is_selected,
        }
    }

    pub fn line_count(&self, width: u16) -> u16 {
        if let Embed::Record(record) = &self.embed {
            RecordWidget::new(record.clone(), false).line_count(width) as u16
        } else {
            self.non_record_paragraph().line_count(width - 2) as u16 + 2
        }
    }

    fn non_record_paragraph(&self) -> Paragraph {
        match &self.embed {
            Embed::Images(images) => Paragraph::new(
                images
                    .iter()
                    .map(|image| {
                        Line::from(format!("[image, alt: {}]", image.alt))
                    })
                    .collect::<Vec<Line>>(),
            ),

            Embed::Video(video) => {
                Paragraph::new(format!("[video, alt: {}]", video.alt))
            }

            Embed::External(external) => Paragraph::new(vec![
                Line::from(external.title.clone())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                Line::from(external.description.clone()),
                Line::from(external.url.clone())
                    .style(Style::default().add_modifier(Modifier::UNDERLINED)),
            ]),

            Embed::Record(_) => panic!("Shouldn't happen"),
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
        if let Embed::Record(record) = self.embed {
            RecordWidget::new(record, self.is_selected).render(area, buf);
        } else {
            let borders = Block::bordered()
                .style(self.style)
                .border_set(symbols::border::ROUNDED);
            let inner_area = borders.inner(area);
            borders.render(area, buf);
            self.non_record_paragraph().render(inner_area, buf);
        }
    }
}
