use atrium_api::app::bsky::feed::get_timeline;
use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    widgets::Widget,
};
use std::sync::{
    mpsc::{Receiver, Sender},
    Arc, Mutex,
};

use crate::{
    app::{AppEvent, EventReceiver},
    columns::{Column, ComposerView, ThreadView},
    components::{
        composer,
        feed::{Feed, FeedPost, Reply},
        list::ListState,
    },
    post_manager,
};

pub enum RequestMsg {
    OldPost,
    Close,
}

pub struct UpdatingFeed {
    pub feed: Arc<Mutex<Feed>>,
    pub request_worker_tx: Sender<RequestMsg>,
}

impl UpdatingFeed {
    pub fn new(tx: Sender<RequestMsg>) -> UpdatingFeed {
        UpdatingFeed {
            feed: Arc::new(Mutex::new(Feed::default())),
            request_worker_tx: tx,
        }
    }

    pub fn spawn_feed_autoupdate(&self, agent: BskyAgent) {
        let feed = Arc::clone(&self.feed);
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
                let new_posts = posts.iter().map(FeedPost::from).filter(|p| {
                    p.reply_to
                        .as_ref()
                        .map(|r| match r {
                            Reply::Reply(r) => r.following,
                            _ => false,
                        })
                        .unwrap_or(true)
                });

                {
                    let mut feed = feed.lock().unwrap();
                    if feed.insert_new_posts(new_posts) {
                        feed.cursor = new_cursor;
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
        tokio::spawn(async move {
            loop {
                let Ok(msg) = rx.recv() else {
                    log::error!("Error receiving request message in worker");
                    continue;
                };

                match msg {
                    RequestMsg::Close => return,

                    RequestMsg::OldPost => {
                        get_old_posts(&agent, Arc::clone(&feed)).await;
                    }
                }
            }
        });
    }
}

impl EventReceiver for &mut UpdatingFeed {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
    ) -> AppEvent {
        let Event::Key(key) = event.clone() else {
            return AppEvent::None;
        };

        let feed = Arc::clone(&self.feed);
        let mut feed = feed.lock().unwrap();

        match key.code {
            KeyCode::Char('q') => {
                return AppEvent::Quit;
            }

            // Cursor move down
            KeyCode::Char('j') => {
                if feed.posts.len() > 0
                    && feed.state.selected == Some(feed.posts.len() - 1)
                {
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

            KeyCode::Char('g') => {
                let Event::Key(event::KeyEvent {
                    code: KeyCode::Char('g'),
                    kind: event::KeyEventKind::Press,
                    ..
                }) = event::read().expect("Cannot read event")
                else {
                    return AppEvent::None;
                };

                feed.state = ListState::default();
                feed.state.select(Some(0));
                return AppEvent::None;
            }

            KeyCode::Char('G') => {
                if feed.posts.len() > 0 {
                    feed.state = ListState::default();
                    feed.state.selected = Some(feed.posts.len() - 1);
                    self.request_worker_tx.send(RequestMsg::OldPost)
                        .unwrap_or_else(|_| {
                            log::error!("Cannot send message to worker fetching old post");
                        });
                }
                return AppEvent::None;
            }

            KeyCode::Enter => {
                let Some(selected) = feed.state.selected else {
                    return AppEvent::None;
                };

                let uri = feed.posts[selected].post_uri.clone();
                drop(feed);

                let view = match ThreadView::from_uri(uri, agent).await {
                    Ok(view) => view,
                    Err(e) => {
                        log::error!("{}", e);
                        return AppEvent::None;
                    }
                };
                return AppEvent::ColumnNewLayer(Column::Thread(view));
            }

            KeyCode::Char('n') => {
                return AppEvent::ColumnNewLayer(Column::Composer(
                    ComposerView::new(None, composer::embed::Embed::None),
                ));
            }

            _ => {
                let Some(selected) = feed.state.selected else {
                    return AppEvent::None;
                };
                let uri = &feed.posts[selected].post_uri;
                let post = post_manager!().at(uri).unwrap();
                return post.handle_events(event, agent).await;
            }
        };
    }
}

impl Widget for &mut UpdatingFeed {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let feed = Arc::clone(&self.feed);
        let mut feed = feed.lock().unwrap();
        feed.render(area, buf);
    }
}

async fn get_old_posts(agent: &BskyAgent, feed: Arc<Mutex<Feed>>) {
    let cursor = {
        let feed = Arc::clone(&feed);
        let feed = feed.lock().unwrap();
        feed.cursor.clone()
    };
    let new_posts = agent
        .api
        .app
        .bsky
        .feed
        .get_timeline(
            get_timeline::ParametersData {
                algorithm: None,
                cursor,
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

    let mut feed = feed.lock().unwrap();
    let posts = posts.iter().map(FeedPost::from).filter(|p| {
        p.reply_to
            .as_ref()
            .map(|r| match r {
                Reply::Reply(r) => r.following,
                _ => false,
            })
            .unwrap_or(true)
    });
    feed.append_old_posts(posts);
    feed.cursor = new_cursor;
}
