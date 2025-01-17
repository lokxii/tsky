use std::process::{Command, Stdio};

use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout},
    prelude::StatefulWidget,
    style::{Color, Style},
    text::Span,
    widgets::{Block, Clear, Widget},
    widgets::{BorderType, Padding},
};

use crate::app::{AppEvent, EventReceiver};
use crate::components::connected_list::{
    ConnectedList, ConnectedListContext, ConnectedListState,
};
use crate::components::paragraph::Paragraph;

#[derive(Clone)]
pub struct Link {
    pub text: String,
    pub url: String,
}

pub struct FacetModal {
    pub links: Vec<Link>,
    pub state: ConnectedListState,
}

impl EventReceiver for &mut FacetModal {
    async fn handle_events(
        self,
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
                if let Result::Err(e) = Command::new("xdg-open")
                    .arg(url)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
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
            Constraint::Length(self.links.len() as u16 + 6),
            Constraint::Fill(1),
        ])
        .areas(area);
        Clear.render(area, buf);

        let block = Block::bordered()
            .padding(Padding::uniform(2))
            .border_type(BorderType::QuadrantInside);
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
                let height = item.line_count(area.width) as u16;
                return (item, height);
            },
        )
        .render(inner_area, buf, &mut self.state);
    }
}
