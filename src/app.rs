use std::sync::Arc;

use bsky_sdk::BskyAgent;
use crossterm::event;
use ratatui::{
    layout::{Constraint, Layout},
    text::Line,
    widgets::Widget,
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

    pub async fn active(&mut self) {
        let last = self.column.pop();
        if last.is_none() {
            return;
        }
        let Some(Column::Composer(mut composer)) = last else {
            self.column.push(last.unwrap());
            return;
        };
        if !composer.post_finished().await {
            self.column.push(Column::Composer(composer));
            return;
        }
    }
}

impl Widget for &mut App {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let logs = Arc::clone(&LOGSTORE.logs);
        let logs = logs.lock().unwrap();

        let [main_area, log_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                .areas(area);

        let last = self.column.pop();
        let (mut modal, mut last) = if let Some(Column::FacetModal(f)) = last {
            (Some(f), self.column.pop())
        } else {
            (None, last)
        };

        match &mut last {
            None => {}
            Some(Column::UpdatingFeed(feed)) => {
                feed.render(main_area, buf);
            }
            Some(Column::Thread(thread)) => {
                thread.render(main_area, buf);
            }
            Some(Column::Composer(composer)) => {
                composer.render(main_area, buf);
            }
            Some(Column::FacetModal(_)) => {
                panic!("FacetModal on top of FacetModal?")
            }
        }

        match &mut modal {
            None => {}
            Some(modal) => modal.render(main_area, buf),
        }

        if last.is_some() {
            self.column.push(last.unwrap());
        }
        if modal.is_some() {
            self.column.push(Column::FacetModal(modal.unwrap()));
        }

        Line::from(format!("log: {}", logs.last().unwrap_or(&String::new())))
            .render(log_area, buf);
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
