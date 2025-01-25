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
    style::Color,
    text::Line,
    widgets::{BorderType, StatefulWidget, Widget},
};

use crate::components::{
    actor::{ActorDetailed, ActorDetailedWidget},
    feed::{Feed, FeedPost, FeedPostWidget},
    list::List,
    separation::Separation,
};

pub struct ProfilePage {
    actor: Arc<Mutex<Option<ActorDetailed>>>,
    feed: Arc<Mutex<Feed>>,
    actor_selected: bool,
}

impl ProfilePage {
    pub async fn from_did(did: String, agent: BskyAgent) -> ProfilePage {
        let actor = Arc::new(Mutex::new(None));
        let feed = Arc::new(Mutex::new(Feed::default()));

        let actor_ = Arc::clone(&actor);
        let did_ = did.clone();
        let agent_ = agent.clone();
        tokio::spawn(async move {
            let out = agent_
                .api
                .app
                .bsky
                .actor
                .get_profile(
                    get_profile::ParametersData {
                        actor: AtIdentifier::Did(Did::new(did_).unwrap()),
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
            *actor_lock = Some(ActorDetailed::new(out));
        });

        let feed_ = Arc::clone(&feed);
        tokio::spawn(async move {
            let out = agent
                .api
                .app
                .bsky
                .feed
                .get_author_feed(
                    get_author_feed::ParametersData {
                        actor: AtIdentifier::Did(Did::new(did).unwrap()),
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
        });
        ProfilePage { actor, feed, actor_selected: true }
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
        }

        let items = std::iter::once(ProfilePageItemWidget::Actor(
            ActorDetailedWidget::new((*actor).as_ref().unwrap())
                .focused(self.actor_selected),
        ))
        .chain(std::iter::once(ProfilePageItemWidget::Bar(
            Separation::default()
                .text(Line::from("Posts ").style(Color::Green))
                .line(BorderType::Double)
                .padding(1),
        )))
        .chain(
            feed.posts
                .iter()
                .map(|p| ProfilePageItemWidget::Post(FeedPostWidget::new(p))),
        )
        .collect::<Vec<_>>();

        // ConnectedList::new(feed.posts.len() + 2, move |context| {
        //     let item = items[context.index].clone();
        //     let height = item.line_count(area.width);
        //     return (item, height);
        // })
        // .render(area, buf, &mut feed.state);
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
