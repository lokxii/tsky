use std::{io::Stdout, sync::Arc};

use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event,
    layout::{Constraint, Layout},
    prelude::CrosstermBackend,
    style::{Color, Style, Stylize},
    text::{Line, Span},
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
                let last_log = logs
                    .last()
                    .map(|(time, log)| {
                        let delta_time = chrono::Local::now() - time;
                        if delta_time.num_seconds() < 5 {
                            Some(log)
                        } else {
                            None
                        }
                    })
                    .flatten();

                let [_, area, _] = Layout::horizontal([
                    Constraint::Fill(1),
                    Constraint::Max(80),
                    Constraint::Fill(1),
                ])
                .areas(f.area());

                let [top_area, main_area, log_area] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Fill(1),
                    Constraint::Length(last_log.is_some() as u16),
                ])
                .areas(area);

                let mut top_items = self
                    .column
                    .stack
                    .iter()
                    .map(Column::name)
                    .map(|c| {
                        Span::styled(
                            c,
                            Style::default().bg(Color::Rgb(45, 50, 55)),
                        )
                    })
                    .peekable();
                let mut top = Line::from(top_items.next().unwrap_or_default());
                while top_items.peek().is_some() {
                    top += Span::styled(" > ", Color::DarkGray);
                    top += top_items.next().unwrap();
                }
                top += top_items.next().unwrap_or_default();
                f.render_widget(top, top_area);

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
                    Some(Column::Notifications(notifications)) => {
                        f.render_widget(notifications, main_area);
                    }
                    Some(Column::PostLikes(post_likes)) => {
                        f.render_widget(post_likes, main_area);
                    }
                    Some(Column::ProfilePage(profile)) => {
                        f.render_widget(profile, main_area);
                    }
                    Some(Column::SearchView(search)) => {
                        f.render_widget(search, main_area);
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

                last_log.map(|log| {
                    f.render_widget(
                        Span::styled(log, Style::default().reversed()),
                        log_area,
                    );
                });
            })
            .unwrap();
    }

    pub async fn refresh(&mut self) {
        let last = self.column.pop();
        if last.is_none() {
            return;
        }
        match last {
            Some(Column::Composer(mut composer)) => {
                if !composer.post_finished().await {
                    self.column.push(Column::Composer(composer));
                }
            }
            Some(Column::SearchView(mut search)) => {
                search.refresh();
                self.column.push(Column::SearchView(search));
            }
            _ => {
                self.column.push(last.unwrap());
            }
        }
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
            Some(Column::Notifications(notifications)) => {
                return notifications.handle_events(event, agent).await
            }
            Some(Column::PostLikes(post_likes)) => {
                return post_likes.handle_events(event, agent).await
            }
            Some(Column::ProfilePage(profile)) => {
                return profile.handle_events(event, agent).await
            }
            Some(Column::SearchView(search)) => {
                return search.handle_events(event, agent).await
            }
        };
    }
}
