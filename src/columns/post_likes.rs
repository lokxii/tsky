use std::sync::{Arc, Mutex};

use bsky_sdk::BskyAgent;

use crate::components::post::Author;

pub struct PostLikes {
    uri: String,
    likes: Arc<Mutex<LikeList>>,
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
                Actor { basic: Author::from(&basic), description }
            })
            .collect();
        let likes = LikeList { actors, cursor };

        Ok(PostLikes { uri, likes: Arc::new(Mutex::new(likes)) })
    }
}

struct LikeList {
    actors: Vec<Actor>,
    cursor: Option<String>,
}

struct Actor {
    basic: Author,
    description: Option<String>,
}
