use std::sync::{Arc, Mutex};

use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{Event, KeyCode},
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{StatefulWidget, Widget},
};

use crate::{
    app::{AppEvent, EventReceiver},
    components::{
        list::{List, ListContext, ListState},
        post::ActorBasic,
    },
};

pub struct PostLikes {
    uri: String,
    actors: Vec<Actor>,
    cursor: Option<String>,
    state: ListState,
}

impl PostLikes {
    pub async fn new(agent: BskyAgent, uri: String) -> Result<Self, String> {
        let res = agent
            .api
            .app
            .bsky
            .feed
            .get_likes(
                atrium_api::app::bsky::feed::get_likes::ParametersData {
                    cid: None,
                    cursor: None,
                    limit: Some(100.try_into().unwrap()),
                    uri: uri.clone(),
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
            .map(|like| {
                let atrium_api::app::bsky::actor::defs::ProfileViewData {
                    associated,
                    avatar,
                    created_at,
                    did,
                    display_name,
                    handle,
                    labels,
                    viewer,
                    description,
                    ..
                } = like.actor.data.clone();
                let basic =
                    atrium_api::app::bsky::actor::defs::ProfileViewBasicData {
                        associated,
                        avatar,
                        created_at,
                        did,
                        display_name,
                        handle,
                        labels,
                        viewer,
                    };
                Actor { basic: ActorBasic::from(&basic), description }
            })
            .collect();

        Ok(PostLikes { uri, actors, cursor, state: ListState::default() })
    }
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
                if let None = self.state.selected {
                    self.state.select(Some(0));
                } else {
                    self.state.next();
                }
                return AppEvent::None;
            }
            KeyCode::Char('k') => {
                self.state.previous();
                return AppEvent::None;
            }
            KeyCode::Backspace => return AppEvent::ColumnPopLayer,

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

        let list = List::new(self.actors.len(), |context: ListContext| {
            let name = self.actors[context.index].basic.name.clone();
            let style = if context.is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            };
            let item = Line::styled(name, style);
            return (item, 1);
        });
        list.render(list_area, buf, &mut self.state);
    }
}

struct Actor {
    basic: ActorBasic,
    description: Option<String>,
}
