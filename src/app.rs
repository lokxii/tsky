use std::{io::Stdout, sync::Arc};

use bsky_sdk::BskyAgent;
use crossterm::event;
use ratatui::{
    layout::{Constraint, Layout},
    prelude::CrosstermBackend,
    Terminal,
};

use crate::{
    columns::{Column, ColumnStack},
    components::logger::LOGSTORE,
};

pub enum AppEvent {
    None,
    Quit,
    ColumnNewLayer(Column),
    ColumnPopLayer,
}

pub trait EventReceiver {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
    ) -> AppEvent;
}

pub struct App {
    pub column: ColumnStack,
}

impl App {
    pub fn new(column: ColumnStack) -> App {
        App { column }
    }

    pub async fn render(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) {
        let logs = Arc::clone(&LOGSTORE.logs);
        let logs = logs.lock().await;

        terminal
            .draw(|f| {
                let [main_area, log_area] = Layout::vertical([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(f.area());

                let last = self.column.pop();
                let (mut modal, mut last) =
                    if let Some(Column::FacetModal(f)) = last {
                        (Some(f), self.column.pop())
                    } else {
                        (None, last)
                    };

                match &mut last {
                    None => {}
                    Some(Column::UpdatingFeed(feed)) => {
                        f.render_widget(feed, main_area);
                    }
                    Some(Column::Thread(thread)) => {
                        f.render_widget(thread, main_area);
                    }
                    Some(Column::Composer(composer)) => {
                        f.render_widget(composer, main_area);
                    }
                    Some(Column::FacetModal(_)) => {
                        panic!("FacetModal on top of FacetModal?")
                    }
                }

                match &mut modal {
                    None => {}
                    Some(modal) => f.render_widget(modal, main_area),
                }

                if last.is_some() {
                    self.column.push(last.unwrap());
                }
                if modal.is_some() {
                    self.column.push(Column::FacetModal(modal.unwrap()));
                }

                f.render_widget(
                    format!("log: {}", logs.last().unwrap_or(&String::new())),
                    log_area,
                );
            })
            .unwrap();
    }
}

impl EventReceiver for &mut App {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
    ) -> AppEvent {
        match self.column.last_mut() {
            None => return AppEvent::None,
            Some(Column::UpdatingFeed(feed)) => {
                return feed.handle_events(event, agent).await
            }
            Some(Column::Thread(thread)) => {
                return thread.handle_events(event, agent).await
            }
            Some(Column::Composer(composer)) => {
                return composer.handle_events(event, agent).await
            }
            Some(Column::FacetModal(modal)) => {
                return modal.handle_events(event, agent).await
            }
        };
    }
}
