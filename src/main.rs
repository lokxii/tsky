mod app;
mod column;
mod composer_view;
mod connected_list;
mod embed;
mod embed_widget;
mod facets;
mod feed;
mod langs;
mod list;
mod logger;
mod post;
mod post_manager;
mod post_widget;
mod record_widget;
mod textarea;
mod thread_view;
mod updating_feed;

use app::{App, AppEvent};
use bsky_sdk::{
    agent::config::{Config, FileStore},
    BskyAgent,
};
use column::{Column, ColumnStack};
use dotenvy::dotenv;
use lazy_static::lazy_static;
use logger::LOGGER;
use post_manager::PostManager;
use std::{
    env, fs,
    path::PathBuf,
    sync::{mpsc, RwLock},
};
use updating_feed::UpdatingFeed;

lazy_static! {
    static ref POST_MANAGER: RwLock<PostManager> =
        RwLock::new(PostManager::new());
    static ref SESSION_FILE: String = {
        let home = env::var("HOME").unwrap();
        format!("{}/.local/share/tsky/session.json", home)
    };
}

#[tokio::main]
async fn main() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    eprintln!("Logging in");
    let agent = login().await;

    let mut terminal = ratatui::init();
    terminal
        .draw(|f| {
            f.render_widget("Creating column (starting workers)", f.area())
        })
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let feed = UpdatingFeed::new(tx);
    feed.spawn_feed_autoupdate(agent.clone());
    feed.spawn_request_worker(agent.clone(), rx);

    terminal
        .draw(|f| f.render_widget("Starting post manager worker", f.area()))
        .unwrap();
    {
        POST_MANAGER.write().unwrap().spawn_worker(agent.clone());
    }

    let mut app = App::new(ColumnStack::from(vec![Column::UpdatingFeed(feed)]));

    loop {
        app.render(&mut terminal).await;

        match app.handle_events(agent.clone()).await {
            AppEvent::None => {}

            AppEvent::Quit => {
                for col in &app.column.stack {
                    match col {
                        Column::UpdatingFeed(feed) => feed
                            .request_worker_tx
                            .send(updating_feed::RequestMsg::Close)
                            .expect("Cannot close worker"),
                        _ => {}
                    }
                }
                break;
            }

            AppEvent::ColumnNewLayer(view) => {
                app.column.push(view);
            }

            AppEvent::ColumnPopLayer => {
                app.column.pop();
            }
        };
    }

    post_manager_tx!()
        .send(post_manager::RequestMsg::Close)
        .expect("Cannot close worker");
    ratatui::restore();
    agent
        .to_config()
        .await
        .save(&FileStore::new(SESSION_FILE.as_str()))
        .await
        .expect(
            format!("Cannot save session file {}", SESSION_FILE.as_str())
                .as_str(),
        );
}

async fn login() -> BskyAgent {
    match Config::load(&FileStore::new(SESSION_FILE.as_str())).await {
        Ok(config) => {
            let agent = BskyAgent::builder()
                .config(config)
                .build()
                .await
                .expect("Cannot create bsky agent from session file");
            return agent;
        }
        Err(e) => {
            eprintln!(
                "Cannot load session file {}: {}\r",
                SESSION_FILE.as_str(),
                e
            );
            eprintln!("Using environment variables to login\r");

            dotenv().unwrap_or_else(|e| {
                eprintln!("Cannot load .env: {}\r", e);
                PathBuf::new()
            });

            let handle = env::var("handle").expect("Cannot get $handle");
            let password = env::var("password").expect("Cannot get $password");

            let agent = BskyAgent::builder()
                .build()
                .await
                .expect("Cannot create bsky agent");
            agent.login(handle, password).await.expect("Cannot login to bsky");

            let path = PathBuf::from(SESSION_FILE.as_str());
            let dir = path.parent().unwrap();
            if !dir.exists() {
                fs::create_dir_all(dir).expect(
                    format!(
                        "Cannot create directory {}",
                        dir.to_str().unwrap()
                    )
                    .as_str(),
                );
            }
            agent
                .to_config()
                .await
                .save(&FileStore::new(SESSION_FILE.as_str()))
                .await
                .expect(
                    format!(
                        "Cannot save session file {}",
                        SESSION_FILE.as_str()
                    )
                    .as_str(),
                );
            return agent;
        }
    };
}
