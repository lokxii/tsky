use atrium_api::{
    self,
    app::bsky::feed::{
        defs::{FeedViewPostData, FeedViewPostReasonRefs, ReplyRefParentRefs},
        get_timeline::{self, ParametersData},
    },
    types::{Object, Union},
};
use bsky_sdk::BskyAgent;
use chrono::{DateTime, FixedOffset, Local};
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use std::{
    collections::VecDeque,
    env,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
};
use tokio::sync::Mutex;
use tui_widget_list::{ListBuilder, ListState, ListView};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();

    terminal.draw(|f| f.render_widget("Logging in", f.area()))?;
    let agent = login().await.unwrap();

    terminal.draw(|f| {
        f.render_widget("Creating column (starting workers)", f.area())
    })?;
    let (tx, rx) = mpsc::channel::<()>();
    let column = Column::new(tx);
    column.spawn_feed_autoupdate(agent.clone());
    column.spawn_get_old_posts_worker(agent.clone(), rx);

    let app = App::new(column);

    loop {
        {
            let feed = Arc::clone(&app.column.feed);
            let mut feed = feed.lock().await;

            terminal.draw(move |f| {
                let width = f.area().width;
                let posts = feed.posts.clone();

                let builder = ListBuilder::new(move |context| {
                    let mut item =
                        PostWidget::new(posts[context.index].clone());
                    if context.is_selected {
                        item = item
                            .style(Style::default().bg(Color::Rgb(45, 50, 55)));
                    }
                    let height = item.calculate_height(width - 2) as u16;
                    return (item, height);
                });

                let list = ListView::new(builder, feed.posts.len())
                    .block(Block::default())
                    .infinite_scrolling(false);

                f.render_stateful_widget(list, f.area(), &mut feed.state);
            })?;
        }

        if app.handle_events().await? {
            break;
        }
    }

    ratatui::restore();
    return Ok(());
}

struct App {
    column: Column,
}

impl App {
    fn new(column: Column) -> App {
        App { column }
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
            KeyCode::Char('j') => {
                if feed.posts.len() > 0
                    && feed.state.selected == Some(feed.posts.len() - 1)
                {
                    let cursor = Arc::clone(&self.column.cursor);
                    if let Result::Err(_) = cursor.try_lock() {
                        feed.state.next();
                        return Ok(false);
                    };
                    self.column.old_post_worker_tx.send(())?;
                } else {
                    feed.state.next();
                }
                return Ok(false);
            }
            KeyCode::Char('k') => {
                feed.state.previous();
                return Ok(false);
            }
            KeyCode::Char('q') => {
                return Ok(true);
            }
            _ => {
                return Ok(false);
            }
        };
    }
}

struct Column {
    feed: Arc<Mutex<ColumnFeed>>,
    cursor: Arc<Mutex<Option<String>>>,
    old_post_worker_tx: Sender<()>,
}

impl Column {
    fn new(tx: Sender<()>) -> Column {
        Column {
            feed: Arc::new(Mutex::new(ColumnFeed {
                posts: VecDeque::new(),
                state: ListState::default(),
            })),
            cursor: Arc::new(Mutex::new(None)),
            old_post_worker_tx: tx,
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
                        ParametersData {
                            algorithm: None,
                            cursor: None,
                            limit: None,
                        }
                        .into(),
                    )
                    .await;
                let Result::Ok(new_posts) = new_posts else {
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

    fn spawn_get_old_posts_worker(&self, agent: BskyAgent, rx: Receiver<()>) {
        let feed = Arc::clone(&self.feed);
        let cursor = Arc::clone(&self.cursor);
        tokio::spawn(async move {
            loop {
                let _ = rx.recv();
                let mut cursor = cursor.lock().await;

                let new_posts = agent
                    .api
                    .app
                    .bsky
                    .feed
                    .get_timeline(
                        ParametersData {
                            algorithm: None,
                            cursor: cursor.clone(),
                            limit: None,
                        }
                        .into(),
                    )
                    .await;

                let Result::Ok(new_posts) = new_posts else {
                    continue;
                };
                let get_timeline::OutputData {
                    feed: posts,
                    cursor: new_cursor,
                } = new_posts.data;
                *cursor = new_cursor;

                let mut feed = feed.lock().await;
                feed.append_old_posts(posts.iter().map(Post::from));
            }
        });
    }
}

struct ColumnFeed {
    posts: VecDeque<Post>,
    state: ListState,
}

impl ColumnFeed {
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
struct Post {
    uri: String,
    author: String,
    handle: String,
    created_at: DateTime<FixedOffset>,
    indexed_at_utc: DateTime<FixedOffset>,
    text: String,
    reason: Option<RepostBy>,
    reply_to: Option<Reply>,
    like: u32,
    repost: u32,
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

        return Post {
            uri: view.post.uri.clone(),
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
            like: view.post.like_count.unwrap_or(0) as u32,
            quote: view.post.quote_count.unwrap_or(0) as u32,
            repost: view.post.repost_count.unwrap_or(0) as u32,
            reply: view.post.reply_count.unwrap_or(0) as u32,
        };
    }
}

struct PostWidget {
    post: Post,
    style: Style,
}

impl PostWidget {
    fn new(post: Post) -> PostWidget {
        PostWidget {
            post,
            style: Style::default(),
        }
    }

    fn style(self, style: Style) -> PostWidget {
        PostWidget { style, ..self }
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

        let [repost_area, author_area, datetime_area, text_area, stats_area];

        if let Some(repost) = &post.reason {
            [
                repost_area,
                author_area,
                datetime_area,
                text_area,
                stats_area,
            ] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .areas(inner_area);

            Line::from(Span::styled(
                String::from("Reposted by ") + &repost.author,
                Color::Green,
            ))
            .render(repost_area, buf);
        } else {
            [author_area, datetime_area, text_area, stats_area] =
                Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(0),
                    Constraint::Length(1),
                ])
                .areas(inner_area);
        }

        Line::from(
            Span::styled(post.author.clone(), Color::Cyan)
                + Span::styled(String::from("  @") + &post.handle, Color::Gray),
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
            post.repost,
            if post.repost == 1 {
                "repost"
            } else {
                "reposts"
            }
        ))
        .style(stat_color)
        .alignment(Alignment::Left)
        .render(repost_area, buf);

        Line::from(format!(
            "{} {}",
            post.like,
            if post.like == 1 { "like" } else { "likes" }
        ))
        .style(stat_color)
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
