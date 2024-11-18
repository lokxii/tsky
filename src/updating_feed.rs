use atrium_api::{
    app::bsky::feed::{
        get_post_thread::OutputThreadRefs as GetPostThreadOutput, get_timeline,
    },
    types::Union,
};
use bsky_sdk::BskyAgent;
use crossterm::event::{self, Event, KeyCode};
use std::{
    process::Command,
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
};
use tokio::sync::{Mutex, MutexGuard};

use crate::{
    app::AppEvent,
    column::Column,
    composer_view::ComposerView,
    feed::{Feed, FeedPost},
    list::ListState,
    post_manager, post_manager_tx,
    thread_view::ThreadView,
};

pub enum RequestMsg {
    OldPost,
    Close,
}

pub struct UpdatingFeed {
    pub feed: Arc<Mutex<Feed>>,
    cursor: Arc<Mutex<Option<String>>>,
    pub request_worker_tx: Sender<RequestMsg>,
}

impl UpdatingFeed {
    pub fn new(tx: Sender<RequestMsg>) -> UpdatingFeed {
        UpdatingFeed {
            feed: Arc::new(Mutex::new(Feed {
                posts: Vec::new(),
                state: ListState::default(),
            })),
            cursor: Arc::new(Mutex::new(None)),
            request_worker_tx: tx,
        }
    }

    pub async fn handle_input_events(&self, agent: BskyAgent) -> AppEvent {
        let Event::Key(key) = event::read().expect("Cannot read event") else {
            return AppEvent::None;
        };
        if key.kind != event::KeyEventKind::Press {
            return AppEvent::None;
        }

        let feed = Arc::clone(&self.feed);
        let mut feed = feed.lock().await;

        match key.code {
            KeyCode::Char('q') => {
                return AppEvent::Quit;
            }

            // Cursor move down
            KeyCode::Char('j') => {
                if feed.posts.len() > 0
                    && feed.state.selected == Some(feed.posts.len() - 1)
                {
                    let cursor = Arc::clone(&self.cursor);
                    if let Result::Err(_) = cursor.try_lock() {
                        feed.state.next();
                        return AppEvent::None;
                    };
                    self.request_worker_tx.send(RequestMsg::OldPost)
                        .unwrap_or_else(|_| {
                            log::error!("Cannot send message to worker fetching old post");
                        });
                } else {
                    feed.state.next();
                }
                return AppEvent::None;
            }

            // Cursor move up
            KeyCode::Char('k') => {
                feed.state.previous();
                return AppEvent::None;
            }

            // Like
            KeyCode::Char(' ') => {
                if feed.state.selected.is_none() {
                    return AppEvent::None;
                }
                let post = &feed.posts[feed.state.selected.unwrap()];
                let post = post_manager!().at(&post.post_uri).unwrap();
                if post.like.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnlikePost(
                            post_manager::DeleteRecordData {
                                post_uri: post.uri.clone(),
                                record_uri: post.like.uri.clone().unwrap(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                } else {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::LikePost(
                            post_manager::CreateRecordData {
                                post_uri: post.uri.clone(),
                                post_cid: post.cid.clone(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                }
                return AppEvent::None;
            }

            // Repost
            KeyCode::Char('o') => {
                if feed.state.selected.is_none() {
                    return AppEvent::None;
                }
                let post = &feed.posts[feed.state.selected.unwrap()];
                let post = post_manager!().at(&post.post_uri).unwrap();
                if post.repost.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnrepostPost(
                            post_manager::DeleteRecordData {
                                post_uri: post.uri.clone(),
                                record_uri: post.repost.uri.clone().unwrap(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker repost post"
                            );
                        });
                } else {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::RepostPost(
                            post_manager::CreateRecordData {
                                post_uri: post.uri.clone(),
                                post_cid: post.cid.clone(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unrepost post"
                            );
                        });
                }
                return AppEvent::None;
            }

            KeyCode::Char('p') => {
                if feed.state.selected.is_none() {
                    return AppEvent::None;
                }
                let post_uri = feed.posts[feed.state.selected.unwrap()]
                    .post_uri
                    .split('/')
                    .collect::<Vec<_>>();
                let author = post_uri[2];
                let post_id = post_uri[4];
                let url = format!(
                    "https://bsky.app/profile/{}/post/{}",
                    author, post_id
                );
                if let Result::Err(e) =
                    Command::new("xdg-open").arg(url).spawn()
                {
                    log::error!("{:?}", e);
                }
                return AppEvent::None;
            }

            KeyCode::Char('m') => {
                if feed.state.selected.is_none() {
                    return AppEvent::None;
                }

                let uri =
                    feed.posts[feed.state.selected.unwrap()].post_uri.clone();
                let Ok(_) = post_manager_tx!()
                    .send(post_manager::RequestMsg::OpenMedia(uri))
                else {
                    return AppEvent::None;
                };
                return AppEvent::None;
            }

            KeyCode::Enter => {
                if feed.state.selected.is_none() {
                    return AppEvent::None;
                }

                let Ok(out) = agent.api.app.bsky.feed.get_post_thread(
                    atrium_api::app::bsky::feed::get_post_thread::ParametersData {
                        depth: Some(1.try_into().unwrap()),
                        parent_height: None,
                        uri: feed.posts[feed.state.selected.unwrap()].post_uri.clone(),
                    }.into()).await else {
                    return AppEvent::None;
                };
                let Union::Refs(thread) = out.data.thread else {
                    log::error!("Unknown thread response");
                    return AppEvent::None;
                };

                match thread {
                    GetPostThreadOutput::AppBskyFeedDefsThreadViewPost(
                        thread,
                    ) => {
                        return AppEvent::ColumnNewLayer(Column::Thread(
                            ThreadView::from(thread.data),
                        ));
                    }
                    GetPostThreadOutput::AppBskyFeedDefsBlockedPost(_) => {
                        log::error!("Blocked thread");
                        return AppEvent::None;
                    }
                    GetPostThreadOutput::AppBskyFeedDefsNotFoundPost(_) => {
                        log::error!("Thread not found");
                        return AppEvent::None;
                    }
                }
            }

            KeyCode::Char('n') => {
                return AppEvent::ColumnNewLayer(Column::Composer(
                    ComposerView::new(),
                ))
            }

            _ => {
                return AppEvent::None;
            }
        };
    }

    pub fn spawn_feed_autoupdate(&self, agent: BskyAgent) {
        let feed = Arc::clone(&self.feed);
        let cursor = Arc::clone(&self.cursor);
        tokio::spawn(async move {
            loop {
                let new_posts = agent
                    .api
                    .app
                    .bsky
                    .feed
                    .get_timeline(
                        get_timeline::ParametersData {
                            algorithm: None,
                            cursor: None,
                            limit: None,
                        }
                        .into(),
                    )
                    .await;
                let Result::Ok(new_posts) = new_posts else {
                    log::error!("Cannot fetch new posts");
                    tokio::time::sleep(tokio::time::Duration::from_secs(1))
                        .await;
                    continue;
                };

                let get_timeline::OutputData {
                    feed: posts,
                    cursor: new_cursor,
                } = new_posts.data;
                let new_posts = posts.iter().map(FeedPost::from);

                {
                    let mut feed = feed.lock().await;
                    if feed.insert_new_posts(new_posts).await {
                        let mut cursor = cursor.lock().await;
                        *cursor = new_cursor;
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        });
    }

    pub fn spawn_request_worker(
        &self,
        agent: BskyAgent,
        rx: Receiver<RequestMsg>,
    ) {
        let feed = Arc::clone(&self.feed);
        let cursor = Arc::clone(&self.cursor);
        tokio::spawn(async move {
            loop {
                let Ok(msg) = rx.recv() else {
                    log::error!("Error receiving request message in worker");
                    continue;
                };

                match msg {
                    RequestMsg::Close => return,

                    RequestMsg::OldPost => {
                        get_old_posts(
                            &agent,
                            Arc::clone(&feed),
                            cursor.lock().await,
                        )
                        .await;
                    }
                }
            }
        });
    }
}

async fn get_old_posts(
    agent: &BskyAgent,
    feed: Arc<Mutex<Feed>>,
    mut cursor: MutexGuard<'_, Option<String>>,
) {
    let new_posts = agent
        .api
        .app
        .bsky
        .feed
        .get_timeline(
            get_timeline::ParametersData {
                algorithm: None,
                cursor: cursor.clone(),
                limit: None,
            }
            .into(),
        )
        .await;
    let Result::Ok(new_posts) = new_posts else {
        log::error!("Cannot fetch old posts");
        return;
    };

    let get_timeline::OutputData { feed: posts, cursor: new_cursor } =
        new_posts.data;
    *cursor = new_cursor;

    let mut feed = feed.lock().await;
    feed.append_old_posts(posts.iter().map(FeedPost::from));
}
