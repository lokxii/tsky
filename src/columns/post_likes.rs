use std::sync::{Arc, Mutex};

use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{Event, KeyCode},
    layout::{Constraint, Layout},
    text::Line,
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

use crate::{
    app::{AppEvent, EventReceiver},
    columns::Column,
    components::{
        actor::{Actor, ActorWidget},
        list::{List, ListState},
    },
};

use super::profile_page::ProfilePage;

pub struct PostLikes {
    uri: String,
    likes: Arc<Mutex<Option<(Vec<Actor>, Option<String>)>>>,
    state: ListState,
}

impl PostLikes {
    pub fn new(agent: BskyAgent, uri: String) -> Self {
        let likes = Arc::new(Mutex::new(None));
        let likes_c = Arc::clone(&likes);
        let uri_c = uri.clone();
        tokio::spawn(async move {
            let o = match fetch_likes(agent, uri_c, None).await {
                Ok(o) => o,
                Err(e) => {
                    log::error!("Cannot fetch likes: {}", e);
                    return;
                }
            };
            let mut likes = likes_c.lock().unwrap();
            *likes = Some(o);
        });
        PostLikes { uri, likes, state: ListState::default() }
    }
}

async fn fetch_likes(
    agent: BskyAgent,
    uri: String,
    cursor: Option<String>,
) -> Result<(Vec<Actor>, Option<String>), String> {
    let res = agent
        .api
        .app
        .bsky
        .feed
        .get_likes(
            atrium_api::app::bsky::feed::get_likes::ParametersData {
                cid: None,
                cursor,
                limit: Some(100.try_into().unwrap()),
                uri,
            }
            .into(),
        )
        .await
        .map_err(|e| e.to_string())?;
    let atrium_api::app::bsky::feed::get_likes::OutputData {
        cursor,
        likes,
        ..
    } = res.data;
    let actors = likes
        .into_iter()
        .map(|like| Actor::new(like.actor.data.clone()))
        .collect();
    return Ok((actors, cursor));
}

impl EventReceiver for &mut PostLikes {
    async fn handle_events(
        self,
        event: ratatui::crossterm::event::Event,
        agent: BskyAgent,
    ) -> crate::app::AppEvent {
        let Event::Key(key) = event.into() else {
            return AppEvent::None;
        };
        match key.code {
            KeyCode::Char('j') => {
                let likes = {
                    let likes = Arc::clone(&self.likes);
                    let likes = likes.lock().unwrap();
                    likes.clone()
                };
                if likes.is_none() {
                    return AppEvent::None;
                }

                let (actors, cursor) = likes.unwrap();
                let likes = Arc::clone(&self.likes);

                if let None = self.state.selected {
                    self.state.selected = Some(0);
                    return AppEvent::None;
                }
                if self.state.selected.unwrap() == actors.len() - 1
                    && cursor.is_some()
                {
                    let uri = self.uri.clone();
                    tokio::spawn(async move {
                        let new_likes =
                            fetch_likes(agent, uri, cursor.clone()).await;
                        let mut new_likes = match new_likes {
                            Ok(o) => o,
                            Err(e) => {
                                log::error!("Cannot fetch likes: {}", e);
                                return;
                            }
                        };
                        let mut likes = likes.lock().unwrap();
                        if likes.is_none() {
                            return;
                        }
                        if likes.as_ref().unwrap().1 != cursor {
                            return;
                        }
                        likes.as_mut().unwrap().0.append(&mut new_likes.0);
                        likes.as_mut().unwrap().1 = new_likes.1;
                    });
                    return AppEvent::None;
                }
                self.state.next();
                return AppEvent::None;
            }
            KeyCode::Char('k') => {
                self.state.previous();
                return AppEvent::None;
            }
            KeyCode::Backspace => return AppEvent::ColumnPopLayer,

            KeyCode::Char('a') => {
                let likes = Arc::clone(&self.likes);
                let likes = likes.lock().unwrap();
                if likes.is_none() || self.state.selected.is_none() {
                    return AppEvent::None;
                }
                let i = self.state.selected.unwrap();
                let actor = &likes.as_ref().unwrap().0[i];
                let me = &agent.get_session().await.unwrap().did;
                let profile =
                    ProfilePage::from_did(actor.basic.did.clone(), me, agent);
                return AppEvent::ColumnNewLayer(Column::ProfilePage(profile));
            }

            _ => return AppEvent::None,
        }
    }
}

impl Widget for &mut PostLikes {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let [title_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)])
                .areas(area);
        Line::from("Post likes:").render(title_area, buf);

        let likes = Arc::clone(&self.likes);
        let likes = likes.lock().unwrap();
        if likes.is_none() {
            return;
        }

        let actors = &likes.as_ref().unwrap().0;
        let list = List::new(actors.len(), |context| {
            let item = ActorWidget::new(&actors[context.index])
                .block(Block::bordered().border_type(BorderType::Rounded))
                .focused(context.is_selected);
            let height = item.line_count(area.width) as u16;
            return (item, height);
        });
        list.render(list_area, buf, &mut self.state);
    }
}
