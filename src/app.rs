use std::{io::Stdout, sync::Arc};

use bsky_sdk::BskyAgent;
use crossterm::event;
use ratatui::{
    layout::{Constraint, Layout},
    prelude::CrosstermBackend,
    Terminal,
};

use crate::{column::Column, column::ColumnStack, logger::LOGSTORE};

pub enum AppEvent {
    None,
    Quit,
    ColumnNewLayer(Column),
    ColumnPopLayer,
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

                match self.column.last_mut() {
                    None => {}
                    Some(Column::UpdatingFeed(feed)) => {
                        let feed = Arc::clone(&feed.feed);
                        let mut feed = feed.lock().unwrap();
                        f.render_widget(&mut *feed, main_area);
                    }
                    Some(Column::Thread(thread)) => {
                        f.render_widget(thread, main_area);
                    }
                    Some(Column::Composer(composer)) => {
                        f.render_widget(composer, main_area);
                    }
                }

                f.render_widget(
                    String::from("log: ")
                        + logs.last().unwrap_or(&String::new()),
                    log_area,
                );
            })
            .unwrap();
    }

    pub async fn handle_events(&mut self, agent: BskyAgent) -> AppEvent {
        if !event::poll(std::time::Duration::from_millis(500))
            .expect("Error polling event")
        {
            return AppEvent::None;
        }

        let event = event::read().expect("Cannot read event");
        match self.column.last_mut() {
            None => return AppEvent::None,
            Some(Column::UpdatingFeed(feed)) => {
                return feed.handle_input_events(event, agent).await
            }
            Some(Column::Thread(thread)) => {
                return thread.handle_input_events(event, agent).await
            }
            Some(Column::Composer(composer)) => {
                return composer.handle_input_events(event, agent).await
            }
        };
    }
}
