use std::process::Command;

use bsky_sdk::BskyAgent;
use crossterm::event::{self, Event, KeyCode};
use ratatui::prelude::StatefulWidget;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::app::AppEvent;
use crate::connected_list::{
    ConnectedList, ConnectedListContext, ConnectedListState,
};

#[derive(Clone)]
pub struct Link {
    pub text: String,
    pub url: String,
}

pub struct FacetModal {
    pub links: Vec<Link>,
    pub state: ConnectedListState,
}

impl FacetModal {
    pub async fn handle_input_events(
        &mut self,
        event: event::Event,
        _: BskyAgent,
    ) -> AppEvent {
        let Event::Key(key) = event else {
            return AppEvent::None;
        };

        match key.code {
            KeyCode::Esc => return AppEvent::ColumnPopLayer,
            KeyCode::Char('j') => {
                self.state.next();
            }
            KeyCode::Char('k') => {
                self.state.previous();
            }
            KeyCode::Enter => {
                let Some(index) = self.state.selected else {
                    return AppEvent::None;
                };
                let url = &self.links[index].url;
                if let Result::Err(e) =
                    Command::new("xdg-open").arg(url).spawn()
                {
                    log::error!("{:?}", e);
                }
            }
            _ => {}
        }
        return AppEvent::None;
    }
}

impl Widget for &mut FacetModal {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let [_, area, _] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Percentage(80),
            Constraint::Fill(1),
        ])
        .areas(area);

        let [_, area, _] = Layout::vertical([
            Constraint::Percentage(30),
            Constraint::Length(self.links.len() as u16 + 2),
            Constraint::Fill(1),
        ])
        .areas(area);
        Clear.render(area, buf);

        let block = Block::bordered().title("Links");
        let inner_area = block.inner(area);
        block.render(area, buf);

        let items = self.links.clone();
        ConnectedList::new(
            self.links.len(),
            move |context: ConnectedListContext| {
                let style = if context.is_selected {
                    Style::default().bg(Color::Rgb(45, 50, 55))
                } else {
                    Style::default()
                };
                let item = Paragraph::new(Span::styled(
                    format!(
                        "`{}` -> {}",
                        items[context.index].text, items[context.index].url
                    ),
                    style,
                ));
                let height = item.line_count(area.width - 2) as u16;
                return (item, height);
            },
        )
        .render(inner_area, buf, &mut self.state);
    }
}
