use std::sync::{Arc, Mutex};

use atrium_api::{
    app::bsky::{actor::get_profile, feed::get_author_feed},
    types::{
        string::{AtIdentifier, Did},
        Object,
    },
};
use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{Event, KeyCode},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

use crate::{
    app::{AppEvent, EventReceiver},
    components::{
        actor::{ActorDetailed, ActorDetailedWidget},
        feed::{Feed, FeedPost, FeedPostWidget},
        list::List,
        separation::Separation,
    },
    post_manager,
};

pub struct ProfilePage {
    actor: Arc<Mutex<Option<ActorDetailed>>>,
    feed: Arc<Mutex<Feed>>,
    actor_selected: bool,
}

impl ProfilePage {
    pub fn from_did(did: Did, me: &Did, agent: BskyAgent) -> ProfilePage {
        let actor = Arc::new(Mutex::new(None));
        let feed = Arc::new(Mutex::new(Feed::default()));

        let actor_ = Arc::clone(&actor);
        let did_ = did.clone();
        let agent_ = agent.clone();
        let is_me = did_ == *me;
        tokio::spawn(async move {
            let out = agent_
                .api
                .app
                .bsky
                .actor
                .get_profile(
                    get_profile::ParametersData {
                        actor: AtIdentifier::Did(did_),
                    }
                    .into(),
                )
                .await;
            let out = match out {
                Ok(Object { data, .. }) => data,
                Err(e) => {
                    log::error!("Cannot fetch actor profile: {}", e);
                    return;
                }
            };
            let mut actor_lock = actor_.lock().unwrap();
            *actor_lock = Some(ActorDetailed::new(out, is_me));
        });

        let feed_ = Arc::clone(&feed);
        let did_ = did.clone();
        tokio::spawn(async move {
            let out = agent
                .api
                .app
                .bsky
                .feed
                .get_author_feed(
                    get_author_feed::ParametersData {
                        actor: AtIdentifier::Did(did_),
                        cursor: None,
                        filter: Some("posts_no_replies".into()),
                        include_pins: Some(true),
                        limit: Some(100.try_into().unwrap()),
                    }
                    .into(),
                )
                .await;
            let get_author_feed::OutputData { cursor, feed } = match out {
                Ok(Object { data, .. }) => data,
                Err(e) => {
                    log::error!("Cannot fetch actor feed: {}", e);
                    return;
                }
            };
            let mut feed_lock = feed_.lock().unwrap();
            feed_lock.cursor = cursor;

            let feed = feed.iter().map(FeedPost::from).peekable();
            feed_lock.insert_new_posts(feed);
            feed_lock.state.selected = None;
        });
        ProfilePage { actor, feed, actor_selected: true }
    }
}

impl EventReceiver for &mut ProfilePage {
    async fn handle_events(
        self,
        event: ratatui::crossterm::event::Event,
        agent: BskyAgent,
    ) -> crate::app::AppEvent {
        let Event::Key(key) = event else {
            return AppEvent::None;
        };
        match key.code {
            KeyCode::Backspace => return AppEvent::ColumnPopLayer,

            KeyCode::Char('q') => {
                return AppEvent::Quit;
            }

            KeyCode::Char('j') => {
                let mut feed = self.feed.lock().unwrap();
                match (feed.state.selected, self.actor_selected) {
                    (None, true) if feed.posts.len() > 0 => {
                        self.actor_selected = false;
                        feed.state.selected = Some(0);
                        feed.state.next();
                        feed.state.next();
                    }
                    (None, _) => {}
                    (Some(_), false) => {
                        feed.state.next();
                    }
                    (Some(_), true) => panic!("How come?"),
                }
                return AppEvent::None;
            }

            KeyCode::Char('k') => {
                let mut feed = self.feed.lock().unwrap();
                match (feed.state.selected, self.actor_selected) {
                    (None, _) => {}
                    (Some(2), false) => {
                        self.actor_selected = true;
                        feed.state.previous();
                        feed.state.previous();
                        feed.state.selected = None;
                    }
                    (Some(i), false) if i > 2 => {
                        feed.state.previous();
                    }
                    (Some(_), _) => panic!("How come?"),
                }
                return AppEvent::None;
            }

            _ => {
                let feed = self.feed.lock().unwrap();
                match (feed.state.selected, self.actor_selected) {
                    (None, false) => return AppEvent::None,
                    (None, true) => {
                        let mut actor = self.actor.lock().unwrap();
                        let Some(actor) = &mut *actor else {
                            return AppEvent::None;
                        };
                        actor.handle_events(event, agent).await;
                        return AppEvent::None;
                    }
                    (Some(i), false) if i >= 2 => {
                        let post = post_manager!()
                            .at(&feed.posts[i - 2].post_uri)
                            .unwrap();
                        drop(feed);
                        return post.handle_events(event, agent).await;
                    }
                    (Some(_), _) => panic!("How come?"),
                }
            }
        }
    }
}

impl Widget for &mut ProfilePage {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let mut feed = self.feed.lock().unwrap();
        let actor = self.actor.lock().unwrap();
        if actor.is_none() {
            Line::from("Loading").render(area, buf);
            return;
        }

        let feed = &mut *feed as *mut Feed;
        unsafe {
            let actor_block = Block::bordered()
                .title(Span::styled("Profile", Color::Gray))
                .border_type(BorderType::Rounded)
                .border_style(Color::DarkGray)
                .style(if self.actor_selected {
                    Style::default().bg(Color::Rgb(45, 50, 55))
                } else {
                    Style::default()
                });
            let items =
                std::iter::once(ProfilePageItemWidget::Actor(
                    ActorDetailedWidget::new((*actor).as_ref().unwrap())
                        .focused(self.actor_selected)
                        .block(actor_block),
                ))
                .chain(std::iter::once(ProfilePageItemWidget::Bar(
                    Separation::default()
                        .text(Line::from("Posts ").style(Color::Green))
                        .line(BorderType::Double)
                        .padding(1),
                )))
                .chain((*feed).posts.iter().map(|p| {
                    ProfilePageItemWidget::Post(FeedPostWidget::new(p))
                }))
                .collect::<Vec<_>>();

            let old_selected = (*feed).state.selected;
            (*feed).state.selected = match old_selected {
                None => Some(0),
                Some(s) => Some(s),
            };

            List::new((*feed).posts.len() + 2, move |context| {
                let item =
                    items[context.index].clone().select(context.is_selected);
                let height = item.line_count(area.width);
                return (item, height);
            })
            .render(area, buf, &mut (*feed).state);

            if old_selected == None {
                (*feed).state.selected = None;
            }
        }
    }
}

#[derive(Clone)]
enum ProfilePageItemWidget<'a> {
    Post(FeedPostWidget<'a>),
    Actor(ActorDetailedWidget<'a>),
    Bar(Separation<'a>),
}

impl<'a> ProfilePageItemWidget<'a> {
    fn line_count(&self, width: u16) -> u16 {
        match self {
            Self::Post(p) => p.line_count(width),
            Self::Actor(a) => a.line_count(width),
            Self::Bar(b) => b.line_count(width),
        }
    }

    fn select(self, selected: bool) -> Self {
        match self {
            Self::Post(p) => Self::Post(p.is_selected(selected)),
            Self::Actor(a) => Self::Actor(a.focused(selected)),
            Self::Bar(b) => Self::Bar(b),
        }
    }
}

impl<'a> Widget for ProfilePageItemWidget<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        match self {
            ProfilePageItemWidget::Post(a) => a.render(area, buf),
            ProfilePageItemWidget::Actor(a) => a.render(area, buf),
            ProfilePageItemWidget::Bar(a) => a.render(area, buf),
        }
    }
}
