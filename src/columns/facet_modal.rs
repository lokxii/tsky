use std::process::{Command, Stdio};

use atrium_api::types::string::Did;
use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout},
    prelude::StatefulWidget,
    style::{Color, Style},
    text::Span,
    widgets::BorderType,
    widgets::{Block, Clear, Widget},
};

use crate::app::{AppEvent, EventReceiver};
use crate::{
    columns::profile_page::ProfilePage,
    columns::Column,
    components::{
        list::{List, ListState},
        paragraph::Paragraph,
    },
};

#[derive(Clone)]
pub struct Link {
    pub text: String,
    pub url: String,
}

#[derive(Clone)]
pub struct Mention {
    pub text: String,
    pub did: Did,
}

#[derive(Clone)]
pub enum FacetModalItem {
    Link(Link),
    Mention(Mention),
}

pub struct FacetModal {
    pub links: Vec<FacetModalItem>,
    pub state: ListState,
}

impl EventReceiver for &mut FacetModal {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
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
                match &self.links[index] {
                    FacetModalItem::Link(l) => {
                        let url = &l.url;
                        if let Result::Err(e) = Command::new("xdg-open")
                            .arg(url)
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                        {
                            log::error!("{:?}", e);
                        }
                    }
                    FacetModalItem::Mention(m) => {
                        let actor = m.did.clone();
                        let me = &agent.get_session().await.unwrap().did;
                        let profile = ProfilePage::from_did(actor, me, agent);
                        return AppEvent::ColumnNewLayer(Column::ProfilePage(
                            profile,
                        ));
                    }
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
            Constraint::Length(self.links.len() as u16 + 4),
            Constraint::Fill(1),
        ])
        .areas(area);
        Clear.render(area, buf);

        let area = {
            let block = Block::bordered();
            let area = block.inner(area);
            let block = Block::bordered()
                .border_type(BorderType::Rounded)
                .title(Span::styled("Facets", Color::Gray));
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        };

        let items = self.links.clone();
        List::new(self.links.len(), move |context| {
            let style = if context.is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            };
            let item = Paragraph::new(Span::styled(
                match &items[context.index] {
                    FacetModalItem::Link(l) => {
                        format!("`{}` -> {}", l.text, l.url)
                    }
                    FacetModalItem::Mention(m) => {
                        format!("`{}` -> @{}", m.text, &*m.did)
                    }
                },
                style,
            ));
            let height = item.line_count(area.width) as u16;
            return (item, height);
        })
        .render(area, buf, &mut self.state);
    }
}
