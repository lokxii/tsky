use atrium_api::{
    self,
    app::bsky::feed::{
        defs::{FeedViewPostData, FeedViewPostReasonRefs, ReplyRefParentRefs},
        get_timeline,
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
    layout::{Alignment, Constraint, Layout},
    prelude::{CrosstermBackend, StatefulWidget},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
    Terminal,
};
use std::{
    collections::VecDeque,
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
use tui_widget_list::{ListBuilder, ListState, ListView};

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
    let column = Column::new(tx);
    column.spawn_feed_autoupdate(agent.clone());
    column.spawn_request_worker(agent.clone(), rx);

    let app = App::new(column);

    loop {
        app.render(&mut terminal).await?;

        if app.handle_events().await? {
            app.column.request_worker_tx.send(RequestMsg::Close)?;
            break;
        }
    }

    ratatui::restore();
    agent
        .to_config()
        .await
        .save(&FileStore::new("session.json"))
        .await?;
    return Ok(());
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
        LogStore {
            logs: Arc::new(Mutex::new(vec![])),
        }
    }
}

struct App {
    column: Column,
}

impl App {
    fn new(column: Column) -> App {
        App { column }
    }

    async fn render(
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let feed = Arc::clone(&self.column.feed);
        let mut feed = feed.lock().await;

        let logs = Arc::clone(&LOGSTORE.logs);
        let logs = logs.lock().await;

        terminal.draw(move |f| {
            let [main_area, log_area] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                    .areas(f.area());
            f.render_widget(&mut *feed, main_area);

            f.render_widget(
                String::from("log: ") + logs.last().unwrap_or(&String::new()),
                log_area,
            );
        })?;

        return Ok(());
    }

    async fn handle_events(&self) -> Result<bool, Box<dyn std::error::Error>> {
        if !event::poll(std::time::Duration::from_millis(500))? {
            return Ok(false);
        }
        let Event::Key(key) = event::read()? else {
            return Ok(false);
        };
        if key.kind != event::KeyEventKind::Press {
            return Ok(false);
        }

        let feed = Arc::clone(&self.column.feed);
        let mut feed = feed.lock().await;

        match key.code {
            KeyCode::Char('q') => {
                return Ok(true);
            }

            // Cursor move down
            KeyCode::Char('j') => {
                if feed.posts.len() > 0
                    && feed.state.selected == Some(feed.posts.len() - 1)
                {
                    let cursor = Arc::clone(&self.column.cursor);
                    if let Result::Err(_) = cursor.try_lock() {
                        feed.state.next();
                        return Ok(false);
                    };
                    self.column.request_worker_tx.send(RequestMsg::OldPost)
                        .unwrap_or_else(|_| {
                            log::error!("Cannot send message to worker fetching old post");
                        });
                } else {
                    feed.state.next();
                }
                return Ok(false);
            }

            // Cursor move up
            KeyCode::Char('k') => {
                feed.state.previous();
                return Ok(false);
            }

            // Like
            KeyCode::Char(' ') => {
                if feed.state.selected.is_none() {
                    return Ok(false);
                }
                let post = &feed.posts[feed.state.selected.unwrap()];
                if post.like.uri.is_some() {
                    self.column
                        .request_worker_tx
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
                    self.column
                        .request_worker_tx
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
                return Ok(false);
            }

            // Repost
            KeyCode::Char('o') => {
                if feed.state.selected.is_none() {
                    return Ok(false);
                }
                let post = &feed.posts[feed.state.selected.unwrap()];
                if post.repost.uri.is_some() {
                    self.column
                        .request_worker_tx
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
                    self.column
                        .request_worker_tx
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
                return Ok(false);
            }

            KeyCode::Char('p') => {
                if feed.state.selected.is_none() {
                    return Ok(false);
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
                return Ok(false);
            }

            _ => {
                return Ok(false);
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

struct CreateRecordData {
    post_uri: String,
    post_cid: Cid,
}

struct DeleteRecordData {
    post_uri: String,
    record_uri: String,
}

enum RequestMsg {
    OldPost,
    LikePost(CreateRecordData),
    UnlikePost(DeleteRecordData),
    RepostPost(CreateRecordData),
    UnrepostPost(DeleteRecordData),
    Close,
}

struct Column {
    feed: Arc<Mutex<Feed>>,
    cursor: Arc<Mutex<Option<String>>>,
    request_worker_tx: Sender<RequestMsg>,
}

impl Column {
    fn new(tx: Sender<RequestMsg>) -> Column {
        Column {
            feed: Arc::new(Mutex::new(Feed {
                posts: VecDeque::new(),
                state: ListState::default(),
            })),
            cursor: Arc::new(Mutex::new(None)),
            request_worker_tx: tx,
        }
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
                let new_posts = posts.iter().map(Post::from);

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

    let get_timeline::OutputData {
        feed: posts,
        cursor: new_cursor,
    } = new_posts.data;
    *cursor = new_cursor;

    let mut feed = feed.lock().await;
    feed.append_old_posts(posts.iter().map(Post::from));
}

struct Feed {
    posts: VecDeque<Post>,
    state: ListState,
}

impl Feed {
    async fn insert_new_posts<T>(&mut self, new_posts: T) -> bool
    where
        T: Iterator<Item = Post> + Clone,
    {
        let new_posts = new_posts.collect::<VecDeque<_>>();
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
        let new_last = new_posts.back().unwrap();
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
            .collect::<VecDeque<_>>();

        self.state.selected = selected_post.map(|post| {
            if let Some(i) = new_view.iter().position(|p| p.uri == post.uri) {
                return i;
            }
            panic!("Cannot decide which post to select after removing duplications");
        });
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

        let builder = ListBuilder::new(move |context| {
            let item = PostWidget::new(
                posts[context.index].clone(),
                context.is_selected,
            );
            let height = item.calculate_height(width - 2) as u16;
            return (item, height);
        });

        ListView::new(builder, self.posts.len())
            .block(Block::default())
            .infinite_scrolling(false)
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
        LikeRepostViewer {
            count: count.unwrap_or(0) as u32,
            uri,
        }
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
    // embeds: (),
}

impl Post {
    fn from(view: &Object<FeedViewPostData>) -> Post {
        let author = &view.post.author;
        let content = &view.post.record;

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
        let created_at_utc = DateTime::parse_from_rfc3339(created_at)
            .unwrap()
            .naive_local();
        let created_at =
            DateTime::from_naive_utc_and_offset(created_at_utc, *dt.offset());

        let like = match &view.post.viewer {
            Some(viewer) => {
                LikeRepostViewer::new(view.post.like_count, viewer.like.clone())
            }
            None => LikeRepostViewer::new(None, None),
        };

        let repost = match &view.post.viewer {
            Some(viewer) => LikeRepostViewer::new(
                view.post.repost_count,
                viewer.repost.clone(),
            ),
            None => LikeRepostViewer::new(None, None),
        };

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

        return Post {
            uri: view.post.uri.clone(),
            cid: view.post.cid.clone(),
            author: author.display_name.clone().unwrap_or("(None)".to_string()),
            handle: author.handle.to_string(),
            created_at,
            text,
            reason,
            reply_to,
            like,
            quote: view.post.quote_count.unwrap_or(0) as u32,
            repost,
            reply: view.post.reply_count.unwrap_or(0) as u32,
        };
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

    fn calculate_height(&self, width: u16) -> u16 {
        self.post.reason.is_some() as u16
            + self.post.reply_to.is_some() as u16
            + 2 // author and date
            + if self.post.text.len() == 0 { 0 } else { self.body_paragraph().line_count(width) as u16 }
            + 1 // stats
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

        let [top_area, author_area, datetime_area, text_area, stats_area] =
            Layout::vertical([
                Constraint::Length(
                    self.post.reason.is_some() as u16
                        + self.post.reply_to.is_some() as u16,
                ),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
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
            if post.repost.count == 1 {
                "repost"
            } else {
                "reposts"
            }
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
            if post.like.count == 1 {
                "like"
            } else {
                "likes"
            },
            if self.is_selected { " (space)" } else { "" }
        ))
        .style(if post.like.uri.is_some() {
            Color::Green
        } else {
            stat_color
        })
        .alignment(Alignment::Left)
        .render(like_area, buf);

        if self.is_selected {
            Line::from("🦋 (p)")
                .style(stat_color)
                .alignment(Alignment::Left)
                .render(bsky_area, buf);
        }
    }
}

impl std::fmt::Display for Post {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(
            f,
            "{} @{}\n{}\n{}\n",
            self.author,
            self.handle,
            self.created_at.to_string(),
            self.text
        );
    }
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
