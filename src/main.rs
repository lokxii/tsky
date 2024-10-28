mod app;
mod embed;
mod embed_widget;
mod feed;
mod list;
mod logger;
mod post;
mod post_manager;
mod post_widget;
mod record_widget;
mod thread_view;

use app::{App, AppEvent};
use atrium_api::{
    self,
    app::bsky::feed::{
        defs::ThreadViewPostRepliesItem,
        get_post_thread::OutputThreadRefs as GetPostThreadOutput, get_timeline,
    },
    types::Union,
};
use bsky_sdk::{
    agent::config::{Config, FileStore},
    BskyAgent,
};
use crossterm::event::{self, Event, KeyCode};
use feed::{Feed, FeedPost};
use lazy_static::lazy_static;
use list::ListState;
use logger::LOGGER;
use post::Post;
use post_manager::PostManager;
use std::{
    env,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, RwLock,
    },
};
use thread_view::ThreadView;
use tokio::{
    process::Command,
    sync::{Mutex, MutexGuard},
};

lazy_static! {
    static ref POST_MANAGER: RwLock<PostManager> =
        RwLock::new(PostManager::new());
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    let mut terminal = ratatui::init();

    terminal.draw(|f| f.render_widget("Logging in", f.area()))?;
    let agent = login().await.unwrap();

    terminal.draw(|f| {
        f.render_widget("Creating column (starting workers)", f.area())
    })?;
    let (tx, rx) = mpsc::channel();
    let feed = UpdatingFeed::new(tx);
    feed.spawn_feed_autoupdate(agent.clone());
    feed.spawn_request_worker(agent.clone(), rx);

    terminal
        .draw(|f| f.render_widget("Starting post manager worker", f.area()))?;
    {
        POST_MANAGER.write().unwrap().spawn_worker(agent.clone());
    }

    let mut app = App::new(ColumnStack::from(vec![Column::UpdatingFeed(feed)]));

    loop {
        app.render(&mut terminal).await?;

        match app.handle_events(agent.clone()).await? {
            AppEvent::None => {}

            AppEvent::Quit => {
                for col in &app.column.stack {
                    match col {
                        Column::UpdatingFeed(feed) => {
                            feed.request_worker_tx.send(RequestMsg::Close)?;
                        }
                        _ => {}
                    }
                }
                break;
            }

            AppEvent::ColumnNewThreadLayer(thread) => {
                app.column.push(Column::Thread(thread));
            }

            AppEvent::ColumnPopLayer => {
                app.column.pop();
            }
        };
    }

    post_manager_tx!().send(post_manager::RequestMsg::Close)?;
    ratatui::restore();
    agent.to_config().await.save(&FileStore::new("session.json")).await?;
    return Ok(());
}

async fn login() -> Result<BskyAgent, Box<dyn std::error::Error>> {
    dotenvy::dotenv()?;

    let handle = env::var("handle")?;
    let password = env::var("password")?;

    match Config::load(&FileStore::new("session.json")).await {
        Ok(config) => {
            let agent = BskyAgent::builder().config(config).build().await?;
            return Ok(agent);
        }
        Err(e) => {
            eprintln!("{}", e);
            eprintln!("Using env var to login");
            let agent = BskyAgent::builder().build().await?;
            agent.login(handle, password).await?;
            agent
                .to_config()
                .await
                .save(&FileStore::new("session.json"))
                .await?;
            return Ok(agent);
        }
    };
}

enum Column {
    UpdatingFeed(UpdatingFeed),
    Thread(ThreadView),
}

struct ColumnStack {
    stack: Vec<Column>,
}

impl ColumnStack {
    fn from(stack: Vec<Column>) -> ColumnStack {
        ColumnStack { stack }
    }

    fn push(&mut self, column: Column) {
        self.stack.push(column);
    }

    fn pop(&mut self) {
        self.stack.pop();
    }

    fn last(&self) -> Option<&Column> {
        self.stack.last()
    }

    fn last_mut(&mut self) -> Option<&mut Column> {
        self.stack.last_mut()
    }
}

enum RequestMsg {
    OldPost,
    Close,
}

struct UpdatingFeed {
    feed: Arc<Mutex<Feed>>,
    cursor: Arc<Mutex<Option<String>>>,
    request_worker_tx: Sender<RequestMsg>,
}

impl UpdatingFeed {
    fn new(tx: Sender<RequestMsg>) -> UpdatingFeed {
        UpdatingFeed {
            feed: Arc::new(Mutex::new(Feed {
                posts: Vec::new(),
                state: ListState::default(),
            })),
            cursor: Arc::new(Mutex::new(None)),
            request_worker_tx: tx,
        }
    }

    async fn handle_input_events(
        &self,
        agent: BskyAgent,
    ) -> Result<AppEvent, Box<dyn std::error::Error>> {
        let Event::Key(key) = event::read()? else {
            return Ok(AppEvent::None);
        };
        if key.kind != event::KeyEventKind::Press {
            return Ok(AppEvent::None);
        }

        let feed = Arc::clone(&self.feed);
        let mut feed = feed.lock().await;

        match key.code {
            KeyCode::Char('q') => {
                return Ok(AppEvent::Quit);
            }

            // Cursor move down
            KeyCode::Char('j') => {
                if feed.posts.len() > 0
                    && feed.state.selected == Some(feed.posts.len() - 1)
                {
                    let cursor = Arc::clone(&self.cursor);
                    if let Result::Err(_) = cursor.try_lock() {
                        feed.state.next();
                        return Ok(AppEvent::None);
                    };
                    self.request_worker_tx.send(RequestMsg::OldPost)
                        .unwrap_or_else(|_| {
                            log::error!("Cannot send message to worker fetching old post");
                        });
                } else {
                    feed.state.next();
                }
                return Ok(AppEvent::None);
            }

            // Cursor move up
            KeyCode::Char('k') => {
                feed.state.previous();
                return Ok(AppEvent::None);
            }

            // Like
            KeyCode::Char(' ') => {
                if feed.state.selected.is_none() {
                    return Ok(AppEvent::None);
                }
                let post = &feed.posts[feed.state.selected.unwrap()];
                let post =
                    POST_MANAGER.read().unwrap().at(&post.post_uri).unwrap();
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
                return Ok(AppEvent::None);
            }

            // Repost
            KeyCode::Char('o') => {
                if feed.state.selected.is_none() {
                    return Ok(AppEvent::None);
                }
                let post = &feed.posts[feed.state.selected.unwrap()];
                let post =
                    POST_MANAGER.read().unwrap().at(&post.post_uri).unwrap();
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
                return Ok(AppEvent::None);
            }

            KeyCode::Char('p') => {
                if feed.state.selected.is_none() {
                    return Ok(AppEvent::None);
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
                return Ok(AppEvent::None);
            }

            KeyCode::Enter => {
                if feed.state.selected.is_none() {
                    return Ok(AppEvent::None);
                }

                let out = agent.api.app.bsky.feed.get_post_thread(
                    atrium_api::app::bsky::feed::get_post_thread::ParametersData {
                        depth: Some(1.try_into().unwrap()),
                        parent_height: None,
                        uri: feed.posts[feed.state.selected.unwrap()].post_uri.clone(),
                    }.into()).await?;
                let Union::Refs(thread) = out.data.thread else {
                    log::error!("Unknown thread response");
                    return Ok(AppEvent::None);
                };

                match thread {
                    GetPostThreadOutput::AppBskyFeedDefsThreadViewPost(
                        thread,
                    ) => {
                        let post = Post::from(&thread.post);
                        let replies = thread.replies.as_ref().map(|replies| {
                            replies.iter().filter_map(|reply| {
                                let Union::Refs(reply) = reply else {
                                    return None;
                                };
                                if let ThreadViewPostRepliesItem::ThreadViewPost(post) = reply {
                                    Some(Post::from(&post.post))
                                } else {
                                    None
                                }
                            }).collect()
                        })
                        .unwrap_or_default();
                        return Ok(AppEvent::ColumnNewThreadLayer(
                            ThreadView::new(post, replies),
                        ));
                    }
                    GetPostThreadOutput::AppBskyFeedDefsBlockedPost(_) => {
                        log::error!("Blocked thread");
                        return Ok(AppEvent::None);
                    }
                    GetPostThreadOutput::AppBskyFeedDefsNotFoundPost(_) => {
                        log::error!("Thread not found");
                        return Ok(AppEvent::None);
                    }
                }
            }

            _ => {
                return Ok(AppEvent::None);
            }
        };
    }

    fn spawn_feed_autoupdate(&self, agent: BskyAgent) {
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

    fn spawn_request_worker(&self, agent: BskyAgent, rx: Receiver<RequestMsg>) {
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
