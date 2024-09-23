use atrium_api;
use atrium_api::app::bsky::feed::defs::FeedViewPostData;
use atrium_api::types::Object;
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
use std::env;
use std::io;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();

    terminal.draw(|f| {
        f.render_widget(Text::from("Logging in"), f.area());
    })?;
    let agent = login().await?;
    terminal.clear()?;

    terminal.draw(|f| {
        f.render_widget(Text::from("Fetching posts"), f.area());
    })?;
    let posts = get_posts(&agent).await?;
    terminal.clear()?;

    let items = posts.iter().collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::bordered().title("TL"))
        .highlight_style(Style::default().bg(Color::Rgb(45, 50, 55)));
    let mut state = ListState::default();

    loop {
        terminal.draw(|f| {
            f.render_stateful_widget(list.clone(), f.area(), &mut state);
        })?;

        if handle_events(&mut state)? {
            break;
        }
    }

    ratatui::restore();
    return Ok(());
}

struct Post {
    author: String,
    handle: String,
    created_at: DateTime<FixedOffset>,
    text: String,
    // embeds: (),
}

impl Post {
    fn from(view_post: &Object<FeedViewPostData>) -> Post {
        let author = &view_post.post.author;
        let content = &view_post.post.record;

        let atrium_api::types::Unknown::Object(data) = content else {
            panic!("Invalid content type");
        };

        let ipld_core::ipld::Ipld::String(created_at) = &*data["createdAt"]
        else {
            panic!("Unknown datatype")
        };
        let ipld_core::ipld::Ipld::String(text) = &*data["text"] else {
            panic!("Unknown datatype")
        };

        let dt = Local::now();
        let created_at_utc = DateTime::parse_from_rfc3339(created_at)
            .unwrap()
            .naive_local();
        let created_at =
            DateTime::from_naive_utc_and_offset(created_at_utc, *dt.offset());

        return Post {
            author: author.display_name.clone().unwrap_or("(None)".to_string()),
            handle: author.handle.to_string(),
            created_at,
            text: text.clone(),
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

async fn get_posts(
    agent: &BskyAgent,
) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
    let out = agent
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
        .await?
        .feed
        .iter()
        .map(Post::from)
        .collect();

    return Ok(out);
}

fn handle_events(list_state: &mut ListState) -> io::Result<bool> {
    let Event::Key(key) = event::read()? else {
        return Ok(false);
    };
    if key.kind != event::KeyEventKind::Press {
        return Ok(false);
    }
    match key.code {
        KeyCode::Char('j') => {
            list_state.select_next();
            return Ok(false);
        }
        KeyCode::Char('k') => {
            list_state.select_previous();
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
