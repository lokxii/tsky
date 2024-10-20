use atrium_api::{
    self,
    app::bsky::{
        embed::{record::ViewRecordRefs, record_with_media::ViewMediaRefs},
        feed::{
            defs::{
                FeedViewPost, FeedViewPostReasonRefs, PostView,
                PostViewEmbedRefs, ReplyRefParentRefs,
                ThreadViewPostRepliesItem,
            },
            get_post_thread::OutputThreadRefs as GetPostThreadOutput,
            get_timeline,
        },
    },
    types::{string::Cid, Object, Union},
};
use bsky_sdk::{
    agent::config::{Config, FileStore},
    BskyAgent,
};
use chrono::{DateTime, FixedOffset, Local};
use crossterm::event::{self, Event, KeyCode};
use itertools::Itertools;
use lazy_static::lazy_static;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Position},
    prelude::{CrosstermBackend, StatefulWidget},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
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
use tokio::{
    process::Command,
    sync::{Mutex, MutexGuard},
};

lazy_static! {
    static ref LOGSTORE: LogStore = LogStore::new();
}
static LOGGER: Logger = Logger;

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

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let logs = Arc::clone(&LOGSTORE.logs);
            let msg = format!(
                "[{}][{}]{}",
                record.level(),
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.args()
            );
            tokio::spawn(async move {
                let mut logs = logs.lock().await;
                logs.push(msg);
            });
        }
    }

    fn flush(&self) {}
}

struct LogStore {
    logs: Arc<Mutex<Vec<String>>>,
}

impl LogStore {
    fn new() -> LogStore {
        LogStore { logs: Arc::new(Mutex::new(vec![])) }
    }
}

struct ListState {
    selected: Option<usize>,
    selected_y: Option<i32>,
    height: u16,
    prev_height: u16,
}

impl Default for ListState {
    fn default() -> Self {
        ListState {
            selected: None,
            selected_y: None,
            height: 0,
            prev_height: 0,
        }
    }
}

impl ListState {
    fn select(&mut self, i: Option<usize>) {
        self.selected = i;
        if let None = self.selected_y {
            self.selected_y = Some(0);
        }
    }

    fn next(&mut self) {
        self.selected.as_mut().map(|i| *i += 1);
        self.selected_y.as_mut().map(|y| *y += self.height as i32);
    }

    fn previous(&mut self) {
        self.selected.as_mut().map(|i| {
            if *i > 0 {
                *i -= 1
            }
        });
        self.selected_y.as_mut().map(|y| {
            *y -= self.prev_height as i32;
            if *y < 0 {
                *y = 0
            }
        });
    }
}

struct ListContext {
    index: usize,
    is_selected: bool,
}

struct List<T>
where
    T: Widget,
{
    len: usize,
    f: Box<dyn Fn(ListContext) -> (T, u16)>,
}

impl<T> List<T>
where
    T: Widget,
{
    fn new(len: usize, f: Box<dyn Fn(ListContext) -> (T, u16)>) -> Self {
        List { len, f }
    }
}

impl<T> StatefulWidget for List<T>
where
    T: Widget,
{
    type State = ListState;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let borders = Block::bordered();
        let inner_area = borders.inner(area);
        borders.render(area, buf);

        if self.len == 0 {
            return;
        }

        if state.selected.is_some() {
            if state.selected.unwrap() >= self.len {
                state.select(Some(self.len - 1));
                state.selected_y.as_mut().map(|y| *y -= state.height as i32);
            }
        }
        let mut i = state.selected.unwrap_or(0) as i32;
        let mut y = state.selected_y.unwrap_or(0);
        let mut bottom_y = 0;
        let mut first = true;

        while i >= 0 {
            let (item, height) = (self.f)(ListContext {
                index: i as usize,
                is_selected: state
                    .selected
                    .map(|s| i == s as i32)
                    .unwrap_or(false),
            });

            if first {
                state.height = height;
                if i > 0 {
                    let (_, h) = (self.f)(ListContext {
                        index: i as usize - 1,
                        is_selected: false,
                    });
                    state.prev_height = h;
                }
                bottom_y = y as u16 + height;
                if bottom_y > inner_area.height {
                    y = (inner_area.height - height) as i32;
                    state.selected_y = Some(y);
                }
                first = false;
            } else {
                y -= height as i32;
            }

            render_truncated(
                item,
                SignedRect {
                    x: inner_area.left() as i32,
                    y: inner_area.top() as i32 + y,
                    width: inner_area.width,
                    height,
                },
                inner_area,
                buf,
            );
            i -= 1;
        }

        let mut i = state.selected.map(|i| i + 1).unwrap_or(0);
        let mut y = bottom_y;
        while i < self.len && y < inner_area.height {
            let (item, height) =
                (self.f)(ListContext { index: i as usize, is_selected: false });

            render_truncated(
                item,
                SignedRect {
                    x: inner_area.left() as i32,
                    y: (inner_area.top() + y) as i32,
                    width: inner_area.width,
                    height,
                },
                inner_area,
                buf,
            );
            i += 1;
            y += height;
        }
    }
}

#[derive(Clone, Copy)]
struct SignedRect {
    x: i32,
    y: i32,
    width: u16,
    height: u16,
}

fn render_truncated<T>(
    widget: T,
    widget_area: SignedRect,
    available_area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
) where
    T: Widget,
{
    let mut internal_buf =
        ratatui::buffer::Buffer::empty(ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: widget_area.width,
            height: widget_area.height,
        });
    widget.render(internal_buf.area, &mut internal_buf);

    for y in widget_area.y..widget_area.y + widget_area.height as i32 {
        for x in widget_area.x..widget_area.x + widget_area.width as i32 {
            if !(y as u16 >= available_area.top()
                && (y as u16) < available_area.bottom()
                && x as u16 >= available_area.left()
                && (x as u16) < available_area.right())
            {
                continue;
            }
            if let Some(to) = buf.cell_mut(Position::new(x as u16, y as u16)) {
                if let Some(from) = internal_buf.cell(Position::new(
                    (x - widget_area.x) as u16,
                    (y - widget_area.y) as u16,
                )) {
                    *to = from.clone();
                }
            }
        }
    }
}

enum AppEvent {
    None,
    Quit,
    ColumnNewThreadLayer(Thread),
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
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let logs = Arc::clone(&LOGSTORE.logs);
        let logs = logs.lock().await;

        match self.column.last() {
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
        &self,
        agent: BskyAgent,
    ) -> Result<AppEvent, Box<dyn std::error::Error>> {
        if !event::poll(std::time::Duration::from_millis(500))? {
            return Ok(AppEvent::None);
        }

        match self.column.last() {
            None => Ok(AppEvent::None),
            Some(Column::UpdatingFeed(feed)) => {
                feed.handle_input_events(agent).await
            }
            Some(Column::Thread(_)) => {
                let Event::Key(key) = event::read()? else {
                    return Ok(AppEvent::None);
                };
                if key.kind != event::KeyEventKind::Press {
                    return Ok(AppEvent::None);
                }

                if key.code == KeyCode::Backspace {
                    return Ok(AppEvent::ColumnPopLayer);
                } else {
                    return Ok(AppEvent::None);
                }
            }
        }
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
    Thread(Thread),
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
                    .collect_vec();
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
                        return Ok(
                            AppEvent::ColumnNewThreadLayer(Thread {
                                post: Post::from_post_view(&thread.post),
                                replies: thread.replies.as_ref().map(|replies| {
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
                                .unwrap_or_default()
                            }));
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

struct Feed {
    posts: Vec<Post>,
    state: ListState,
}

impl Feed {
    async fn insert_new_posts<T>(&mut self, new_posts: T) -> bool
    where
        T: Iterator<Item = Post> + Clone,
    {
        let new_posts = new_posts.collect::<Vec<_>>();
        if new_posts.len() == 0 {
            return true;
        }

        if self.posts.len() == 0 {
            self.posts = new_posts;
            self.state.select(Some(0));
            self.remove_duplicate();
            return true;
        }

        let selected = self.state.selected.map(|s| self.posts[s].clone());
        let new_last = new_posts.last().unwrap();
        let Some(overlap_idx) = self.posts.iter().position(|p| p == new_last)
        else {
            self.posts = new_posts;
            self.state.select(Some(0));
            self.remove_duplicate();
            return true;
        };

        self.posts = new_posts
            .into_iter()
            .chain(self.posts.clone().into_iter().skip(overlap_idx + 1))
            .collect();
        self.state.select(selected.map(|post| {
            self.posts.iter().position(|p| *p == post).unwrap_or(0)
        }));
        self.remove_duplicate();

        return false;
    }

    fn append_old_posts<T>(&mut self, new_posts: T)
    where
        T: Iterator<Item = Post> + Clone,
    {
        if self.posts.len() == 0 {
            return;
        }

        let mut new_posts = new_posts.collect();
        self.posts.append(&mut new_posts);
        self.remove_duplicate();
    }

    fn remove_duplicate(&mut self) {
        let selected_post = self.state.selected.map(|i| self.posts[i].clone());
        let new_view = self
            .posts
            .iter()
            .unique_by(|p| &p.uri)
            .map(Post::clone)
            .collect::<Vec<_>>();

        self.state.select(selected_post.map(|post| {
            if let Some(i) = new_view.iter().position(|p| p.uri == post.uri) {
                return i;
            }
            panic!("Cannot decide which post to select after removing duplications");
        }));
        self.posts = new_view;
    }
}

impl Widget for &mut Feed {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let width = area.width;
        let posts = self.posts.clone();

        List::new(
            self.posts.len(),
            Box::new(move |context: ListContext| {
                let item = PostWidget::new(
                    posts[context.index].clone(),
                    context.is_selected,
                );
                let height = item.line_count(width - 2) as u16;
                return (item, height);
            }),
        )
        .render(area, buf, &mut self.state);
    }
}

#[derive(PartialEq, Eq, Clone)]
struct RepostBy {
    author: String,
    handle: String,
}

#[derive(PartialEq, Eq, Clone)]
struct ReplyToAuthor {
    author: String,
    handle: String,
}

#[derive(PartialEq, Eq, Clone)]
enum Reply {
    Author(ReplyToAuthor),
    DeletedPost,
    BlockedUser,
}

#[derive(Clone)]
struct LikeRepostViewer {
    count: u32,
    uri: Option<String>,
}

impl LikeRepostViewer {
    fn new(count: Option<i64>, uri: Option<String>) -> LikeRepostViewer {
        LikeRepostViewer { count: count.unwrap_or(0) as u32, uri }
    }
}

#[derive(Clone)]
struct Post {
    uri: String,
    cid: Cid,
    author: String,
    handle: String,
    created_at: DateTime<FixedOffset>,
    text: String,
    reason: Option<RepostBy>,
    reply_to: Option<Reply>,
    like: LikeRepostViewer,
    repost: LikeRepostViewer,
    quote: u32,
    reply: u32,
    embed: Option<Embed>,
    // label
}

impl Post {
    fn from_post_view(view: &PostView) -> Post {
        let author = &view.author;
        let content = &view.record;

        let atrium_api::types::Unknown::Object(record) = content else {
            panic!("Invalid content type");
        };

        let ipld_core::ipld::Ipld::String(created_at) = &*record["createdAt"]
        else {
            panic!("createdAt is not a string")
        };

        let ipld_core::ipld::Ipld::String(text) = &*record["text"] else {
            panic!("text is not a string")
        };
        let text = text.clone();

        let dt = Local::now();
        let created_at_utc =
            DateTime::parse_from_rfc3339(created_at).unwrap().naive_local();
        let created_at =
            DateTime::from_naive_utc_and_offset(created_at_utc, *dt.offset());

        let like = match &view.viewer {
            Some(viewer) => {
                LikeRepostViewer::new(view.like_count, viewer.like.clone())
            }
            None => LikeRepostViewer::new(None, None),
        };

        let repost = match &view.viewer {
            Some(viewer) => {
                LikeRepostViewer::new(view.repost_count, viewer.repost.clone())
            }
            None => LikeRepostViewer::new(None, None),
        };

        let embed = view.embed.as_ref().map(Embed::from);

        return Post {
            uri: view.uri.clone(),
            cid: view.cid.clone(),
            author: author.display_name.clone().unwrap_or("(None)".to_string()),
            handle: author.handle.to_string(),
            created_at,
            text,
            reason: None,
            reply_to: None,
            like,
            quote: view.quote_count.unwrap_or(0) as u32,
            repost,
            reply: view.reply_count.unwrap_or(0) as u32,
            embed,
        };
    }

    fn from_feed_view_post(view: &FeedViewPost) -> Post {
        let post = Post::from_post_view(&view.post);

        let reason = view.reason.as_ref().map(|r| {
            let Union::Refs(r) = r else {
                panic!("Unknown reason type");
            };
            let FeedViewPostReasonRefs::ReasonRepost(r) = r;
            RepostBy {
                author: r.by.display_name.clone().unwrap_or(String::new()),
                handle: r.by.handle.to_string(),
            }
        });

        let reply_to = view.reply.as_ref().map(|r| {
            let Union::Refs(parent) = &r.data.parent else {
                panic!("Unknown parent type");
            };
            match parent {
                ReplyRefParentRefs::PostView(view) => {
                    Reply::Author(ReplyToAuthor {
                        author: view
                            .data
                            .author
                            .display_name
                            .clone()
                            .unwrap_or("".to_string()),
                        handle: view.data.author.handle.to_string(),
                    })
                }
                ReplyRefParentRefs::NotFoundPost(_) => Reply::DeletedPost,
                ReplyRefParentRefs::BlockedPost(_) => Reply::BlockedUser,
            }
        });

        return Post { reason, reply_to, ..post };
    }
}

impl PartialEq for Post {
    fn eq(&self, other: &Self) -> bool {
        return self.uri == other.uri && self.reason == other.reason;
    }
}

impl Eq for Post {}

struct PostWidget {
    post: Post,
    style: Style,
    is_selected: bool,
}

impl PostWidget {
    fn new(post: Post, is_selected: bool) -> PostWidget {
        PostWidget {
            post,
            style: if is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            },
            is_selected,
        }
    }

    fn line_count(&self, width: u16) -> u16 {
        self.post.reason.is_some() as u16
            + self.post.reply_to.is_some() as u16
            + 2 // author and date
            + self.body_paragraph().line_count(width) as u16
            + 1 // stats
            + self.post.embed.clone().map(|e| EmbedWidget::new(e, false).line_count(width) as u16).unwrap_or(0)
            + 2 // borders
    }

    fn body_paragraph(&self) -> Paragraph {
        Paragraph::new(
            self.post
                .text
                .split('\n')
                .map(|line| Line::from(line).style(Color::White))
                .collect::<Vec<Line>>(),
        )
        .wrap(ratatui::widgets::Wrap { trim: true })
    }
}

impl Widget for PostWidget {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let borders = Block::bordered().style(self.style);
        let inner_area = borders.inner(area);
        let post = &self.post;

        borders.render(area, buf);

        let text = self.body_paragraph();
        let embed = self
            .post
            .embed
            .clone()
            .map(|e| EmbedWidget::new(e.into(), self.is_selected));

        let [top_area, author_area, datetime_area, text_area, embed_area, stats_area] =
            Layout::vertical([
                Constraint::Length(
                    self.post.reason.is_some() as u16
                        + self.post.reply_to.is_some() as u16,
                ),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(text.line_count(inner_area.width) as u16),
                Constraint::Length(
                    embed
                        .as_ref()
                        .map(|e| e.line_count(inner_area.width))
                        .unwrap_or(0),
                ),
                Constraint::Length(1),
            ])
            .areas(inner_area);

        let [repost_area, reply_area] = Layout::vertical([
            Constraint::Length(self.post.reason.is_some() as u16),
            Constraint::Length(self.post.reply_to.is_some() as u16),
        ])
        .areas(top_area);

        if let Some(repost) = &self.post.reason {
            Line::from(Span::styled(
                format!("⭮ Reposted by {}", repost.author),
                Color::Green,
            ))
            .render(repost_area, buf);
        }

        if let Some(reply_to) = &self.post.reply_to {
            let reply_to = match reply_to {
                Reply::Author(a) => &a.author,
                Reply::DeletedPost => "[deleted post]",
                Reply::BlockedUser => "[blocked user]",
            };
            Line::from(Span::styled(
                format!("⮡ Reply to {}", reply_to),
                Color::Rgb(180, 180, 180),
            ))
            .render(reply_area, buf);
        }

        Line::from(
            Span::styled(post.author.clone(), Color::Cyan)
                + Span::styled(format!(" @{}", post.handle), Color::Gray),
        )
        .render(author_area, buf);

        Line::from(post.created_at.to_string())
            .style(Color::DarkGray)
            .render(datetime_area, buf);

        self.body_paragraph().render(text_area, buf);

        let [reply_area, quote_area, repost_area, like_area, bsky_area] =
            Layout::horizontal([
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .areas(stats_area);

        let stat_color = Color::Rgb(130, 130, 130);

        Line::from(format!(
            "{} {}",
            post.reply,
            if post.reply == 1 { "reply" } else { "replies" }
        ))
        .style(stat_color)
        .alignment(Alignment::Left)
        .render(reply_area, buf);

        Line::from(format!(
            "{} {}",
            post.quote,
            if post.quote == 1 { "quote" } else { "quotes" }
        ))
        .style(stat_color)
        .alignment(Alignment::Left)
        .render(quote_area, buf);

        Line::from(format!(
            "{} {}",
            post.repost.count,
            if post.repost.count == 1 { "repost" } else { "reposts" }
        ))
        .style(if post.repost.uri.is_some() {
            Color::Green
        } else {
            stat_color
        })
        .alignment(Alignment::Left)
        .render(repost_area, buf);

        Line::from(format!(
            "{} {}{}",
            post.like.count,
            if post.like.count == 1 { "like" } else { "likes" },
            if self.is_selected { " (space)" } else { "" }
        ))
        .style(if post.like.uri.is_some() { Color::Green } else { stat_color })
        .alignment(Alignment::Left)
        .render(like_area, buf);

        if self.is_selected {
            Line::from("🦋 (p)")
                .style(stat_color)
                .alignment(Alignment::Left)
                .render(bsky_area, buf);
        }

        embed.map(|e| e.render(embed_area, buf));
    }
}

#[derive(Clone, Debug)]
enum Embed {
    Images(Vec<Image>),
    Video(Video),
    External(External),
    Record(Record),
}

impl Embed {
    fn from(e: &Union<PostViewEmbedRefs>) -> Embed {
        let Union::Refs(e) = e else {
            panic!("Unknown embed type");
        };
        match e {
            PostViewEmbedRefs::AppBskyEmbedImagesView(view) => {
                Embed::Images(view.images.iter().map(Image::from).collect())
            }
            PostViewEmbedRefs::AppBskyEmbedVideoView(view) => {
                Embed::Video(Video::from(view))
            }
            PostViewEmbedRefs::AppBskyEmbedExternalView(view) => {
                Embed::External(External::from(view))
            }
            PostViewEmbedRefs::AppBskyEmbedRecordView(view) => {
                Embed::Record(Record::from(&*view, None))
            }
            PostViewEmbedRefs::AppBskyEmbedRecordWithMediaView(view) => {
                let media = Some(EmbededPostMedia::from(&view.media));
                Embed::Record(Record::from(&view.record, media))
            }
        }
    }
}

struct EmbedWidget {
    embed: Embed,
    style: Style,
    is_selected: bool,
}

impl EmbedWidget {
    fn new(embed: Embed, is_selected: bool) -> EmbedWidget {
        EmbedWidget {
            embed,
            style: if is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            },
            is_selected,
        }
    }

    fn non_record_paragraph(&self) -> Paragraph {
        match &self.embed {
            Embed::Images(images) => Paragraph::new(
                images
                    .iter()
                    .map(|image| {
                        Line::from(format!("[image, alt: {}]", image.alt))
                    })
                    .collect::<Vec<Line>>(),
            ),

            Embed::Video(video) => {
                Paragraph::new(format!("[video, alt: {}]", video.alt))
            }

            Embed::External(external) => Paragraph::new(vec![
                Line::from(external.title.clone())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                Line::from(external.description.clone()),
                Line::from(external.url.clone())
                    .style(Style::default().add_modifier(Modifier::UNDERLINED)),
            ]),

            Embed::Record(_) => panic!("Shouldn't happen"),
        }
    }

    fn line_count(&self, width: u16) -> u16 {
        if let Embed::Record(record) = &self.embed {
            RecordWidget::new(record.clone(), false).line_count(width) as u16
        } else {
            self.non_record_paragraph().line_count(width - 2) as u16 + 2
        }
    }
}

impl Widget for EmbedWidget {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        if let Embed::Record(record) = self.embed {
            RecordWidget::new(record, self.is_selected).render(area, buf);
        } else {
            let borders = Block::bordered().style(self.style);
            let inner_area = borders.inner(area);
            borders.render(area, buf);
            self.non_record_paragraph().render(inner_area, buf);
        }
    }
}

#[derive(Clone, Debug)]
enum Record {
    Post(EmbededPost),
    Blocked,
    NotFound,
    Detached,
    // List(EmbededList),
    // Generator(EmbededGenerator),
    // Labler(EmbededLabler),
    // StarterPack(EmbededStarterPack),
    NotImplemented,
}

impl Record {
    fn from(
        view: &Object<atrium_api::app::bsky::embed::record::ViewData>,
        media: Option<EmbededPostMedia>,
    ) -> Record {
        let Union::Refs(record) = &view.record else {
            panic!("Unknown embeded record type");
        };
        match record {
            ViewRecordRefs::ViewRecord(post) => {
                let atrium_api::types::Unknown::Object(record) = &post.value
                else {
                    panic!("Unknown embeded post value type");
                };
                let ipld_core::ipld::Ipld::String(text) = &*record["text"]
                else {
                    panic!("embeded text is not a string");
                };
                let text = text.clone();

                Record::Post(EmbededPost {
                    uri: post.uri.clone(),
                    author: post
                        .author
                        .display_name
                        .clone()
                        .unwrap_or_default(),
                    handle: post.author.handle.to_string(),
                    has_embed: post
                        .embeds
                        .as_ref()
                        .map(|v| v.len() > 0)
                        .unwrap_or(false),
                    media,
                    text,
                })
            }

            ViewRecordRefs::ViewBlocked(_) => Record::Blocked,
            ViewRecordRefs::ViewNotFound(_) => Record::NotFound,
            ViewRecordRefs::ViewDetached(_) => Record::Detached,
            _ => Record::NotImplemented,
        }
    }
}

struct RecordWidget {
    record: Record,
    style: Style,
    is_selected: bool,
}

impl RecordWidget {
    fn new(record: Record, is_selected: bool) -> RecordWidget {
        RecordWidget {
            record,
            style: if is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            },
            is_selected,
        }
    }

    fn line_count(&self, width: u16) -> u16 {
        match &self.record {
            Record::Post(post) => {
                let text_lines = Paragraph::new(
                    post.text
                        .split('\n')
                        .map(|line| Line::from(line).style(Color::White))
                        .collect::<Vec<Line>>(),
                )
                .wrap(ratatui::widgets::Wrap { trim: true })
                .line_count(width - 2) as u16;

                let media_lines = post
                    .media
                    .clone()
                    .map(|e| {
                        EmbedWidget::new(e.into(), false).line_count(width - 2)
                            + 2
                    })
                    .unwrap_or(0);

                media_lines + (1 + text_lines) + post.has_embed as u16 + 2
            }
            _ => 1 + 2,
        }
    }
}

impl Widget for RecordWidget {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        match self.record {
            Record::Post(post) => {
                let text = Paragraph::new(
                    post.text
                        .split('\n')
                        .map(|line| Line::from(line).style(Color::White))
                        .collect::<Vec<Line>>(),
                )
                .wrap(ratatui::widgets::Wrap { trim: true });

                let media = post
                    .media
                    .map(|e| EmbedWidget::new(e.into(), self.is_selected));

                let [media_area, quote_area] = Layout::vertical([
                    Constraint::Length(
                        media
                            .as_ref()
                            .map(|m| m.line_count(area.width - 2))
                            .unwrap_or(0),
                    ),
                    Constraint::Length(
                        text.line_count(area.width - 2) as u16
                            + 1
                            + post.has_embed as u16
                            + 2,
                    ),
                ])
                .areas(area);

                media.map(|e| e.render(media_area, buf));

                let quote_border = Block::bordered().style(self.style);
                let quote_inner_area = quote_border.inner(quote_area);
                quote_border.render(quote_area, buf);

                let [author_area, text_area, quote_embed_area] =
                    Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Length(
                            text.line_count(area.width - 2) as u16
                        ),
                        Constraint::Length(post.has_embed as u16),
                    ])
                    .areas(quote_inner_area);

                Line::from(
                    Span::styled(post.author.clone(), Color::Cyan)
                        + Span::styled(
                            format!(" @{}", post.handle),
                            Color::Gray,
                        ),
                )
                .render(author_area, buf);
                text.render(text_area, buf);
                if post.has_embed {
                    Line::from("[embed]")
                        .style(Color::DarkGray)
                        .render(quote_embed_area, buf);
                }
            }

            Record::Blocked => {
                Line::from("[blocked]").render(area, buf);
            }
            Record::NotFound => {
                Line::from("[Not found]").render(area, buf);
            }
            Record::Detached => {
                Line::from("[Detached]").render(area, buf);
            }
            Record::NotImplemented => {
                Line::from("[Not implemented]").render(area, buf);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct EmbededPost {
    uri: String,
    author: String,
    handle: String,
    has_embed: bool,
    media: Option<EmbededPostMedia>,
    text: String,
    // label
}

#[derive(Clone, Debug)]
enum EmbededPostMedia {
    Images(Vec<Image>),
    Video(Video),
    External(External),
}

impl EmbededPostMedia {
    fn from(
        media: &Union<
            atrium_api::app::bsky::embed::record_with_media::ViewMediaRefs,
        >,
    ) -> EmbededPostMedia {
        let Union::Refs(media) = media else {
            panic!("Unknown embed media type");
        };
        match media {
            ViewMediaRefs::AppBskyEmbedImagesView(data) => {
                EmbededPostMedia::Images(
                    data.images
                        .iter()
                        .map(|image| Image {
                            url: image.fullsize.clone(),
                            alt: image.alt.clone(),
                        })
                        .collect(),
                )
            }
            ViewMediaRefs::AppBskyEmbedVideoView(data) => {
                EmbededPostMedia::Video(Video {
                    m3u8: data.playlist.clone(),
                    alt: data.alt.clone().unwrap_or_default(),
                })
            }
            ViewMediaRefs::AppBskyEmbedExternalView(data) => {
                EmbededPostMedia::External(External {
                    url: data.external.uri.clone(),
                    title: data.external.title.clone(),
                    description: data.external.description.clone(),
                })
            }
        }
    }
}

impl Into<Embed> for EmbededPostMedia {
    fn into(self) -> Embed {
        match self {
            Self::Images(images) => Embed::Images(images),
            Self::Video(video) => Embed::Video(video),
            Self::External(external) => Embed::External(external),
        }
    }
}

#[derive(Clone, Debug)]
struct Image {
    alt: String,
    url: String, // full size image
}

impl Image {
    fn from(
        image: &Object<atrium_api::app::bsky::embed::images::ViewImageData>,
    ) -> Image {
        Image { url: image.fullsize.clone(), alt: image.alt.clone() }
    }
}

#[derive(Clone, Debug)]
struct Video {
    alt: String,
    m3u8: String,
}

impl Video {
    fn from(
        video: &Object<atrium_api::app::bsky::embed::video::ViewData>,
    ) -> Video {
        Video {
            alt: video.alt.clone().unwrap_or_default(),
            m3u8: video.playlist.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct External {
    url: String,
    title: String,
    description: String,
}

impl External {
    fn from(
        external: &Object<atrium_api::app::bsky::embed::external::ViewData>,
    ) -> External {
        External {
            url: external.external.uri.clone(),
            title: external.external.title.clone(),
            description: external.external.description.clone(),
        }
    }
}

// #[derive(Clone)]
// struct EmbededList {
//     uri: String,
//     name: String,
//     description: String,
//     author: String,
//     handle: String,
// }
//
// #[derive(Clone)]
// struct EmbededGenerator {
//     uri: String,
//     name: String,
//     description: String,
//     author: String,
//     handle: String,
//     // label
// }
//
// #[derive(Clone)]
// struct EmbededLabler {
//     // No name?
//     uri: String,
//     // name: String,
//     // description: String,
//     author: String,
//     handle: String,
// }
//
// #[derive(Clone)]
// struct EmbededStarterPack {
//     uri: String,
//     author: String,
//     handle: String,
// }

// Without all the reasons and reply to
struct Thread {
    post: Post,
    replies: Vec<Post>,
}

impl Widget for &Thread {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let post_widget = PostWidget::new(self.post.clone(), false);
        let post_height = post_widget.line_count(area.width);

        let [post_area, replies_area] = Layout::vertical([
            Constraint::Length(post_height),
            Constraint::Fill(1),
        ])
        .areas(area);

        post_widget.render(post_area, buf);

        let replies_block = Block::new().borders(Borders::TOP);
        let replies_block_inner = replies_block.inner(replies_area);
        replies_block.render(replies_area, buf);

        let mut state = ListState::default();
        let replies = self.replies.clone();
        List::new(
            self.replies.len(),
            Box::new(move |context: ListContext| {
                let item = PostWidget::new(
                    replies[context.index].clone(),
                    context.is_selected,
                );
                let height =
                    item.line_count(replies_block_inner.width - 2) as u16;
                return (item, height);
            }),
        )
        .render(replies_block_inner, buf, &mut state);
    }
}
