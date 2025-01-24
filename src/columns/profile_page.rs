use std::sync::{Arc, Mutex};

use atrium_api::{
    app::bsky::{actor::get_profile, feed::get_author_feed},
    types::{
        string::{AtIdentifier, Did},
        Object,
    },
};
use bsky_sdk::BskyAgent;

use crate::components::{
    actor::{Actor, ActorDetailed},
    feed::{Feed, FeedPost, Reason},
};

pub struct ProfilePage {
    actor: Arc<Mutex<Option<ActorDetailed>>>,
    feed: Arc<Mutex<Feed>>,
    pin_uri: Arc<Mutex<Option<String>>>,
}

impl ProfilePage {
    pub async fn from_did(did: String, agent: BskyAgent) -> ProfilePage {
        let actor = Arc::new(Mutex::new(None));
        let feed = Arc::new(Mutex::new(Feed::default()));
        let pin_uri = Arc::new(Mutex::new(None));

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
        let pin_uri_ = Arc::clone(&pin_uri);
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

            let mut feed = feed.iter().map(FeedPost::from).peekable();
            match feed.peek() {
                Some(f) if f.reason == Some(Reason::Pin) => {
                    let post = feed.next().unwrap();
                    let mut pin_uri_lock = pin_uri_.lock().unwrap();
                    *pin_uri_lock = Some(post.post_uri);
                }
                _ => {}
            }
        });
        ProfilePage { actor, feed, pin_uri }
    }
}
