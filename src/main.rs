use atrium_api::{
    self,
    app::bsky::feed::{
        defs::{FeedViewPostData, FeedViewPostReasonRefs, ReplyRefParentRefs},
        get_timeline,
    },
    types::{string::Cid, Object, Union},
};
use bsky_sdk::BskyAgent;
use chrono::{DateTime, FixedOffset, Local};
use crossterm::event::{self, Event, KeyCode};
use lazy_static::lazy_static;
use log::error;
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
use tokio::sync::{Mutex, MutexGuard, RwLock};
use tui_widget_list::{ListBuilder, ListState, ListView};

lazy_static! {
    static ref LOGSTORE: LogStore = LogStore::new();
}
static LOGGER: Logger = Logger;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);

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
                let mut logs = logs.write().await;
                logs.push(msg);
            });
        }
    }

    fn flush(&self) {}
}

struct LogStore {
    logs: Arc<RwLock<Vec<String>>>,
}

impl LogStore {
    fn new() -> LogStore {
        LogStore {
            logs: Arc::new(RwLock::new(vec![])),
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
        let logs = logs.read().await;

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
        let Event::Key(key) = event::read()? else {
            return Ok(false);
        };
        let feed = Arc::clone(&self.column.feed);
        let mut feed = feed.lock().await;
        if key.kind != event::KeyEventKind::Press {
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('q') => {
                return Ok(true);
            }

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
                            error!("Cannot send message to worker fetching old post");
                        });
                } else {
                    feed.state.next();
                }
                return Ok(false);
            }
            KeyCode::Char('k') => {
                feed.state.previous();
                return Ok(false);
            }

            KeyCode::Char(' ') => {
                if feed.state.selected.is_none() {
                    return Ok(false);
                }
                let post = &feed.posts[feed.state.selected.unwrap()];
                if post.like.uri.is_some() {
                    self.column
                        .request_worker_tx
                        .send(RequestMsg::UnlikePost(UnlikePostData {
                            post_uri: post.uri.clone(),
                            like_uri: post.like.uri.clone().unwrap(),
                        }))
                        .unwrap_or_else(|_| {
                            error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                } else {
                    self.column
                        .request_worker_tx
                        .send(RequestMsg::LikePost(LikePostData {
                            post_uri: post.uri.clone(),
                            post_cid: post.cid.clone(),
                        }))
                        .unwrap_or_else(|_| {
                            error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                }
                return Ok(false);
            }

            _ => {
                return Ok(false);
            }
        };
    }
}

struct LikePostData {
    post_uri: String,
    post_cid: Cid,
}

struct UnlikePostData {
    post_uri: String,
    like_uri: String,
}

enum RequestMsg {
    OldPost,
    LikePost(LikePostData),
    UnlikePost(UnlikePostData),
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
                    error!("Cannot fetch new posts");
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
                    error!("Error receiving request message in worker");
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
                        let Ok(output) = agent.create_record(
                            atrium_api::app::bsky::feed::like::RecordData {
                                created_at: atrium_api::types::string::Datetime::now(),
                                subject: atrium_api::com::atproto::repo::strong_ref::MainData {
                                    cid: data.post_cid,
                                    uri: data.post_uri.clone(),
                                }.into()
                            },
                        ).await else {
                            error!("Could not post create record liking post");
                            continue;
                        };
                        let mut feed = feed.lock().await;
                        feed.posts.iter_mut().for_each(|post| {
                            if post.uri == data.post_uri {
                                post.like.uri = Some(output.uri.clone());
                            }
                        });
                    }
                    RequestMsg::UnlikePost(data) => {
                        let Ok(_) =
                            agent.delete_record(data.like_uri.clone()).await
                        else {
                            error!("Could not post delete record unliking post");
                            continue;
                        };
                        let mut feed = feed.lock().await;
                        feed.posts.iter_mut().for_each(|post| {
                            if post.uri == data.post_uri {
                                post.like.uri = None;
                            }
                        });
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
        error!("Cannot fetch old posts");
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
        if self.posts.len() == 0 {
            self.posts = new_posts.collect();
            self.state.select(Some(0));
            return true;
        }

        let last_newest = &self.posts[0];
        let overlap_idx = new_posts.clone().position(|p| &p == last_newest);
        match overlap_idx {
            Some(idx) => {
                new_posts.take(idx).for_each(|p| self.posts.push_front(p));
                if let Some(i) = self.state.selected {
                    self.state.select(Some(i + idx));
                }
            }
            None => {
                self.posts = new_posts.collect();
                return true;
            }
        }
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

        let list = ListView::new(builder, self.posts.len())
            .block(Block::default())
            .infinite_scrolling(false);

        list.render(area, buf, &mut self.state);
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

#[derive(PartialEq, Eq, Clone)]
struct LikeRepostViewer {
    count: u32,
    uri: Option<String>,
}

impl LikeRepostViewer {
    fn new(count: Option<i64>, uri: Option<String>) -> LikeRepostViewer {
        LikeRepostViewer {
            count: count.unwrap_or(0) as u32 - uri.is_some() as u32,
            uri,
        }
    }

    fn count(&self) -> u32 {
        self.count + self.uri.is_some() as u32
    }
}

#[derive(PartialEq, Eq, Clone)]
struct Post {
    uri: String,
    cid: Cid,
    author: String,
    handle: String,
    created_at: DateTime<FixedOffset>,
    indexed_at_utc: DateTime<FixedOffset>,
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

        let indexed_at_utc: DateTime<FixedOffset>;
        if let Some(reason) = &view.reason {
            let Union::Refs(reason) = reason else {
                panic!("Unknown reason type");
            };
            let FeedViewPostReasonRefs::ReasonRepost(reason) = reason;
            indexed_at_utc = *reason.indexed_at.as_ref();
        } else {
            indexed_at_utc = *view.post.indexed_at.as_ref();
        }

        let ipld_core::ipld::Ipld::String(text) = &*record["text"] else {
            panic!("text is not a string")
        };

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

        return Post {
            uri: view.post.uri.clone(),
            cid: view.post.cid.clone(),
            author: author.display_name.clone().unwrap_or("(None)".to_string()),
            handle: author.handle.to_string(),
            created_at,
            indexed_at_utc,
            text: text.clone(),
            reason: view.reason.as_ref().map(|r| {
                let Union::Refs(r) = r else {
                    panic!("Unknown reason type");
                };
                let FeedViewPostReasonRefs::ReasonRepost(r) = r;
                RepostBy {
                    author: r.by.display_name.clone().unwrap_or(String::new()),
                    handle: r.by.handle.to_string(),
                }
            }),
            reply_to: view.reply.as_ref().map(|r| {
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
            }),
            like,
            quote: view.post.quote_count.unwrap_or(0) as u32,
            repost,
            reply: view.post.reply_count.unwrap_or(0) as u32,
        };
    }
}

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
            + 2 // author and date
            + self.body_paragraph().line_count(width) as u16
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

        let [reply_area, quote_area, repost_area, like_area] =
            Layout::horizontal([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
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
            post.repost.count(),
            if post.repost.count() == 1 {
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
            post.like.count(),
            if post.like.count() == 1 {
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

    let agent = BskyAgent::builder().build().await?;
    agent.login(handle, password).await?;

    return Ok(agent);
}
