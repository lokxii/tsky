use atrium_api;
use atrium_api::app::bsky::feed::defs::{
    FeedViewPostData, FeedViewPostReasonRefs,
};
use atrium_api::types::{Object, Union};
use bsky_sdk::BskyAgent;
use chrono::Local;
use chrono::{DateTime, FixedOffset};
use crossterm::event;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use std::collections::VecDeque;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();

    terminal.draw(|f| f.render_widget("Logging in", f.area()))?;
    let agent = login().await.unwrap();

    let app = App::new();

    terminal.draw(|f| f.render_widget("Fetching posts", f.area()))?;
    {
        let c = Arc::clone(&app.column);
        let mut c = c.lock().await;
        c.get_new_posts(&agent).await?;
    }

    loop {
        let list = List::new(app.column.lock().await.posts.iter())
            .block(Block::bordered().title("TL"))
            .highlight_style(Style::default().bg(Color::Rgb(45, 50, 55)));

        {
            let column = Arc::clone(&app.column);
            let mut column = column.lock().await;
            terminal.draw(|f| {
                f.render_stateful_widget(
                    list.clone(),
                    f.area(),
                    &mut column.state,
                );
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
    column: Arc<Mutex<Column>>,
}

impl App {
    fn new() -> App {
        App {
            column: Arc::new(Mutex::new(Column::new())),
        }
    }

    async fn handle_events(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let Event::Key(key) = event::read()? else {
            return Ok(false);
        };
        let column = Arc::clone(&self.column);
        let mut column = column.lock().await;
        if key.kind != event::KeyEventKind::Press {
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('j') => {
                column.state.select_next();
                return Ok(false);
            }
            KeyCode::Char('k') => {
                column.state.select_previous();
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
    posts: VecDeque<Post>,
    state: ListState,
}

impl Column {
    fn new() -> Column {
        Column {
            posts: VecDeque::new(),
            state: ListState::default(),
        }
    }

    async fn get_new_posts(
        &mut self,
        agent: &BskyAgent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let new_posts = agent
            .api
            .app
            .bsky
            .feed
            .get_timeline(
                atrium_api::app::bsky::feed::get_timeline::ParametersData {
                    algorithm: None,
                    cursor: None,
                    limit: None,
                }
                .into(),
            )
            .await?;
        let new_posts = new_posts.feed.iter().map(Post::from);

        if self.posts.len() == 0 {
            self.posts = new_posts.collect();
            self.state.select(Some(0));
            return Ok(());
        }

        let last_newest = &self.posts[0];
        let overlap_idx = new_posts.clone().position(|p| &p == last_newest);
        match overlap_idx {
            Some(idx) => {
                new_posts.take(idx).for_each(|p| self.posts.push_front(p))
            }
            None => self.posts = new_posts.collect(),
        }
        return Ok(());
    }
}

#[derive(PartialEq, Eq)]
struct RepostBy {
    author: String,
    handle: String,
}

#[derive(PartialEq, Eq)]
struct Post {
    uri: String,
    author: String,
    handle: String,
    created_at: DateTime<FixedOffset>,
    indexed_at_utc: DateTime<FixedOffset>,
    text: String,
    reason: Option<RepostBy>,
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
        };
    }
}

impl Into<Text<'_>> for &Post {
    fn into(self) -> Text<'static> {
        let mut t = Text::from(self.author.clone()).style(Color::Cyan);
        t.push_span(Span::styled(
            String::from("  @") + &self.handle,
            Color::Gray,
        ));
        t += Line::from(self.created_at.to_string()).style(Color::DarkGray);
        t += Line::from(self.text.to_string()).style(Color::White);
        return t;
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
