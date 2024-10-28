mod app;
mod column;
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
mod updating_feed;

use app::{App, AppEvent};
use bsky_sdk::{
    agent::config::{Config, FileStore},
    BskyAgent,
};
use column::{Column, ColumnStack};
use lazy_static::lazy_static;
use logger::LOGGER;
use post_manager::PostManager;
use std::{
    env,
    sync::{mpsc, RwLock},
};
use updating_feed::UpdatingFeed;

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
                            feed.request_worker_tx
                                .send(updating_feed::RequestMsg::Close)?;
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
