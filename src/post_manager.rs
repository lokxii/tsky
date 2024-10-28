use atrium_api::types::string::Cid;
use bsky_sdk::BskyAgent;
use std::{
    collections::HashMap,
    process::Command,
    sync::{
        mpsc::{self, Sender},
        Arc,
    },
};

use crate::{embed::Embed, post::Post};

pub struct DeleteRecordData {
    pub post_uri: String,
    pub record_uri: String,
}

pub struct CreateRecordData {
    pub post_uri: String,
    pub post_cid: Cid,
}

pub enum RequestMsg {
    LikePost(CreateRecordData),
    UnlikePost(DeleteRecordData),
    RepostPost(CreateRecordData),
    UnrepostPost(DeleteRecordData),
    OpenMedia(String),
    Close,
}

pub struct PostManager {
    posts: Arc<std::sync::Mutex<HashMap<String, Post>>>,
    pub tx: Option<Sender<RequestMsg>>,
}

macro_rules! request_retry {
    ($retry:expr, $request:expr) => {{
        let mut count = 0;
        loop {
            let r = $request;
            match r {
                Ok(output) => break Some(output),
                Err(_) => {
                    count += 1;
                    if count == $retry {
                        break None;
                    }
                }
            }
        }
    }};
}

impl PostManager {
    pub fn new() -> PostManager {
        PostManager {
            posts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            tx: None,
        }
    }

    pub fn insert(&self, post: Post) {
        let posts = Arc::clone(&self.posts);
        let mut posts = posts.lock().unwrap();
        posts.insert(post.uri.clone(), post);
    }

    pub fn append(&self, new_posts: Vec<Post>) {
        let posts = Arc::clone(&self.posts);
        let mut posts = posts.lock().unwrap();
        posts.extend(new_posts.into_iter().map(|p| (p.uri.clone(), p)));
    }

    pub fn at(&self, key: &String) -> Option<Post> {
        let posts = Arc::clone(&self.posts);
        let posts = posts.lock().unwrap();
        return posts.get(key).map(|p| p.to_owned());
    }

    pub fn spawn_worker(&mut self, agent: BskyAgent) {
        let posts = Arc::clone(&self.posts);
        let (tx, rx) = mpsc::channel();
        self.tx = Some(tx);
        tokio::spawn(async move {
            loop {
                let Ok(msg) = rx.recv() else {
                    continue;
                };
                match msg {
                    RequestMsg::Close => return,

                    RequestMsg::LikePost(data) => {
                        let Some(output) = request_retry!(3, {
                            agent.create_record(
                                atrium_api::app::bsky::feed::like::RecordData {
                                    created_at: atrium_api::types::string::Datetime::now(),
                                    subject: atrium_api::com::atproto::repo::strong_ref::MainData {
                                        cid: data.post_cid.clone(),
                                        uri: data.post_uri.clone(),
                                    }.into()
                                },
                            ).await
                        }) else {
                            log::error!(
                                "Could not post create record liking post"
                            );
                            continue;
                        };

                        let mut posts = posts.lock().unwrap();
                        let Some(post) = posts.get_mut(&data.post_uri) else {
                            log::error!("Could not find post in post manager");
                            continue;
                        };
                        post.like.uri = Some(output.uri.clone());
                        post.like.count += 1;
                        tokio::spawn(async {}); // black magic, removing this causes feed autoupdating to stop
                    }

                    RequestMsg::UnlikePost(data) => {
                        let Some(_) = request_retry!(3, {
                            agent.delete_record(data.record_uri.clone()).await
                        }) else {
                            log::error!(
                                "Could not post delete record unliking post"
                            );
                            continue;
                        };

                        let mut posts = posts.lock().unwrap();
                        let Some(post) = posts.get_mut(&data.post_uri) else {
                            log::error!("Could not find post in post manager");
                            continue;
                        };
                        post.like.uri = None;
                        post.like.count -= 1;
                        tokio::spawn(async {});
                    }

                    RequestMsg::RepostPost(data) => {
                        let Some(output) = request_retry!(3, {
                            agent.create_record(
                                atrium_api::app::bsky::feed::repost::RecordData {
                                    created_at: atrium_api::types::string::Datetime::now(),
                                    subject: atrium_api::com::atproto::repo::strong_ref::MainData {
                                        cid: data.post_cid.clone(),
                                        uri: data.post_uri.clone(),
                                    }.into()
                                }
                            ).await
                        }) else {
                            log::error!(
                                "Could not post create record reposting post"
                            );
                            continue;
                        };

                        let mut posts = posts.lock().unwrap();
                        let Some(post) = posts.get_mut(&data.post_uri) else {
                            log::error!("Could not find post in post manager");
                            continue;
                        };
                        post.repost.uri = Some(output.uri.clone());
                        post.repost.count += 1;
                        tokio::spawn(async {});
                    }

                    RequestMsg::UnrepostPost(data) => {
                        let Some(_) = request_retry!(3, {
                            agent.delete_record(data.record_uri.clone()).await
                        }) else {
                            log::error!(
                                "Could not post delete record unreposting post"
                            );
                            continue;
                        };

                        let mut posts = posts.lock().unwrap();
                        let Some(post) = posts.get_mut(&data.post_uri) else {
                            log::error!("Could not find post in post manager");
                            continue;
                        };
                        post.repost.uri = None;
                        post.repost.count -= 1;
                        tokio::spawn(async {});
                    }

                    RequestMsg::OpenMedia(uri) => {
                        let posts = posts.lock().unwrap();
                        let Some(post) = posts.get(&uri) else {
                            log::error!("Could not find post in post manager");
                            continue;
                        };

                        if post.embed.is_none() {
                            continue;
                        }
                        match post.embed.as_ref().unwrap() {
                            Embed::Record(_) => continue,

                            Embed::Images(images) => {
                                images.iter().for_each(|image| {
                                    if let Result::Err(e) =
                                        Command::new("xdg-open")
                                            .arg(image.url.clone())
                                            .spawn()
                                    {
                                        log::error!("{:?}", e);
                                    }
                                });
                            }

                            Embed::Video(video) => {
                                if let Result::Err(e) = Command::new("vlc")
                                    .arg(video.m3u8.clone())
                                    .spawn()
                                {
                                    log::error!("{:?}", e);
                                }
                            }

                            Embed::External(external) => {
                                if let Result::Err(e) = Command::new("xdg-open")
                                    .arg(external.url.clone())
                                    .spawn()
                                {
                                    log::error!("{:?}", e);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

#[macro_export]
macro_rules! post_manager {
    () => {
        crate::POST_MANAGER.read().unwrap()
    };
}

#[macro_export]
macro_rules! post_manager_tx {
    () => {
        crate::POST_MANAGER.read().unwrap().tx.as_ref().unwrap()
    };
}
