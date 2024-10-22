mod embed;
mod embed_widget;
mod feed;
mod list;
mod logger;
mod post;
mod post_widget;
mod record_widget;
mod thread_view;

use atrium_api::{
    self,
    app::bsky::feed::{
        defs::ThreadViewPostRepliesItem,
        get_post_thread::OutputThreadRefs as GetPostThreadOutput, get_timeline,
    },
    types::{string::Cid, Union},
};
use bsky_sdk::{
    agent::config::{Config, FileStore},
    BskyAgent,
};
use crossterm::event::{self, Event, KeyCode};
use feed::Feed;
use list::ListState;
use logger::{LOGGER, LOGSTORE};
use post::Post;
use ratatui::{
    layout::{Constraint, Layout},
    prelude::CrosstermBackend,
    Terminal,
};
use std::{
    env,
    io::Stdout,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
};
use thread_view::ThreadView;
use tokio::{
    process::Command,
    sync::{Mutex, MutexGuard},
};

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

pub enum AppEvent {
    None,
    Quit,
    ColumnNewThreadLayer(ThreadView),
    ColumnPopLayer,
}

struct App {
    column: ColumnStack,
}

impl App {
    fn new(column: ColumnStack) -> App {
        App { column }
    }

    async fn render(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let logs = Arc::clone(&LOGSTORE.logs);
        let logs = logs.lock().await;

        match self.column.last_mut() {
            None => {}
            Some(Column::UpdatingFeed(feed)) => {
                let feed = Arc::clone(&feed.feed);
                let mut feed = feed.lock().await;

                terminal.draw(|f| {
                    let [main_area, log_area] = Layout::vertical([
                        Constraint::Fill(1),
                        Constraint::Length(1),
                    ])
                    .areas(f.area());
                    f.render_widget(&mut *feed, main_area);

                    f.render_widget(
                        String::from("log: ")
                            + logs.last().unwrap_or(&String::new()),
                        log_area,
                    );
                })?;
            }
            Some(Column::Thread(thread)) => {
                terminal.draw(|f| {
                    let [main_area, log_area] = Layout::vertical([
                        Constraint::Fill(1),
                        Constraint::Length(1),
                    ])
                    .areas(f.area());
                    f.render_widget(thread, main_area);

                    f.render_widget(
                        String::from("log: ")
                            + logs.last().unwrap_or(&String::new()),
                        log_area,
                    );
                })?;
            }
        }

        return Ok(());
    }

    async fn handle_events(
        &mut self,
        agent: BskyAgent,
    ) -> Result<AppEvent, Box<dyn std::error::Error>> {
        if !event::poll(std::time::Duration::from_millis(500))? {
            return Ok(AppEvent::None);
        }

        match self.column.last_mut() {
            None => return Ok(AppEvent::None),
            Some(Column::UpdatingFeed(feed)) => {
                return feed.handle_input_events(agent).await
            }
            Some(Column::Thread(thread)) => {
                return thread.handle_input_events().await
            }
        };
    }
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
    LikePost(CreateRecordData),
    UnlikePost(DeleteRecordData),
    RepostPost(CreateRecordData),
    UnrepostPost(DeleteRecordData),
    Close,
}

struct CreateRecordData {
    post_uri: String,
    post_cid: Cid,
}

struct DeleteRecordData {
    post_uri: String,
    record_uri: String,
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
                if post.like.uri.is_some() {
                    self.request_worker_tx
                        .send(RequestMsg::UnlikePost(DeleteRecordData {
                            post_uri: post.uri.clone(),
                            record_uri: post.like.uri.clone().unwrap(),
                        }))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                } else {
                    self.request_worker_tx
                        .send(RequestMsg::LikePost(CreateRecordData {
                            post_uri: post.uri.clone(),
                            post_cid: post.cid.clone(),
                        }))
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
                if post.repost.uri.is_some() {
                    self.request_worker_tx
                        .send(RequestMsg::UnrepostPost(DeleteRecordData {
                            post_uri: post.uri.clone(),
                            record_uri: post.repost.uri.clone().unwrap(),
                        }))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker repost post"
                            );
                        });
                } else {
                    self.request_worker_tx
                        .send(RequestMsg::RepostPost(CreateRecordData {
                            post_uri: post.uri.clone(),
                            post_cid: post.cid.clone(),
                        }))
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
                    .uri
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
                        uri: feed.posts[feed.state.selected.unwrap()].uri.clone(),
                    }.into()).await?;
                let Union::Refs(thread) = out.data.thread else {
                    log::error!("Unknown thread response");
                    return Ok(AppEvent::None);
                };

                match thread {
                    GetPostThreadOutput::AppBskyFeedDefsThreadViewPost(
                        thread,
                    ) => {
                        let post = Post::from_post_view(&thread.post);
                        let replies = thread.replies.as_ref().map(|replies| {
                            replies.iter().filter_map(|reply| {
                                let Union::Refs(reply) = reply else {
                                    return None;
                                };
                                if let ThreadViewPostRepliesItem::ThreadViewPost(post) = reply {
                                    Some(Post::from_post_view(&post.post))
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
                let new_posts = posts.iter().map(Post::from_feed_view_post);

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

                        let mut feed = feed.lock().await;
                        let post = feed
                            .posts
                            .iter_mut()
                            .find(|post| post.uri == data.post_uri)
                            .unwrap();
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

                        let mut feed = feed.lock().await;
                        let post = feed
                            .posts
                            .iter_mut()
                            .find(|post| post.uri == data.post_uri)
                            .unwrap();
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

                        let mut feed = feed.lock().await;
                        let post = feed
                            .posts
                            .iter_mut()
                            .find(|post| post.uri == data.post_uri)
                            .unwrap();
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

                        let mut feed = feed.lock().await;
                        let post = feed
                            .posts
                            .iter_mut()
                            .find(|post| post.uri == data.post_uri)
                            .unwrap();
                        post.repost.uri = None;
                        post.repost.count -= 1;
                        tokio::spawn(async {});
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
    feed.append_old_posts(posts.iter().map(Post::from_feed_view_post));
}
