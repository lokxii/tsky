use std::{io::Stdout, sync::Arc};

use bsky_sdk::BskyAgent;
use crossterm::event;
use ratatui::{
    layout::{Constraint, Layout},
    prelude::CrosstermBackend,
    Terminal,
};

use crate::{
    column::Column, column::ColumnStack, logger::LOGSTORE,
    thread_view::ThreadView,
};

pub enum AppEvent {
    None,
    Quit,
    ColumnNewThreadLayer(ThreadView),
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let logs = Arc::clone(&LOGSTORE.logs);
        let logs = logs.lock().await;

        match self.column.last_mut() {
            None => {}
            Some(Column::UpdatingFeed(feed)) => {
                let feed = Arc::clone(&feed.feed);
                let mut feed = feed.lock().await;

                terminal
                    .draw(|f| {
                        let [main_area, log_area] = Layout::vertical([
                            Constraint::Fill(1),
                            Constraint::Length(1),
                        ])
                        .areas(f.area());
                        f.render_widget(&mut *feed, main_area);

                        f.render_widget(
                            String::from("log: ")
                                + logs.last().unwrap_or(&String::new()),
                            log_area,
                        );
                    })
                    .unwrap();
            }
            Some(Column::Thread(thread)) => {
                terminal
                    .draw(|f| {
                        let [main_area, log_area] = Layout::vertical([
                            Constraint::Fill(1),
                            Constraint::Length(1),
                        ])
                        .areas(f.area());
                        f.render_widget(thread, main_area);

                        f.render_widget(
                            String::from("log: ")
                                + logs.last().unwrap_or(&String::new()),
                            log_area,
                        );
                    })
                    .unwrap();
            }
        }

        return Ok(());
    }

    pub async fn handle_events(&mut self, agent: BskyAgent) -> AppEvent {
        if !event::poll(std::time::Duration::from_millis(500))
            .expect("Error polling event")
        {
            return AppEvent::None;
        }

        match self.column.last_mut() {
            None => return AppEvent::None,
            Some(Column::UpdatingFeed(feed)) => {
                return feed.handle_input_events(agent).await
            }
            Some(Column::Thread(thread)) => {
                return thread.handle_input_events(agent).await
            }
        };
    }
}
