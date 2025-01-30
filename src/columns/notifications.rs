use std::sync::{Arc, Mutex};

use atrium_api::types::Object;
use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{Event, KeyCode},
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

use crate::{
    app::{AppEvent, EventReceiver},
    columns::{Column, ProfilePage, ThreadView},
    components::{
        list::{List, ListState},
        notification::{Notification, NotificationWidget, Record},
        post::Post,
    },
    post_manager,
};

pub struct Notifications {
    feed: Arc<Mutex<Feed>>,
    seen_at: Arc<Mutex<Option<atrium_api::types::string::Datetime>>>,
    is_terminate_worker: Arc<Mutex<bool>>,
}

struct Feed {
    notifs: Vec<Notification>,
    state: ListState,
}

impl Notifications {
    pub async fn new(agent: BskyAgent) -> Self {
        use atrium_api::app::bsky::notification::{
            list_notifications, update_seen,
        };

        let seen_at = Arc::new(Mutex::new(None));
        let feed = Feed { notifs: vec![], state: ListState::default() };
        let feed = Arc::new(Mutex::new(feed));
        let is_terminate_worker = Arc::new(Mutex::new(false));

        let seen_at_ = Arc::clone(&seen_at);
        let feed_ = Arc::clone(&feed);
        tokio::spawn(async move {
            let out = agent
                .api
                .app
                .bsky
                .notification
                .list_notifications(
                    list_notifications::ParametersData {
                        cursor: None,
                        limit: Some(100.try_into().unwrap()),
                        priority: None,
                        reasons: Some(
                            [
                                "like", "repost", "follow", "mention", "reply",
                                "quote",
                            ]
                            .into_iter()
                            .map(String::from)
                            .collect(),
                        ),
                        seen_at: None,
                    }
                    .into(),
                )
                .await;
            let Object { data, .. } = match out {
                Ok(o) => o,
                Err(e) => {
                    log::error!("Cannot fetch notifications {}", e);
                    return;
                }
            };
            let list_notifications::OutputData { notifications, .. } = data;
            let new_seen_at = atrium_api::types::string::Datetime::now();

            let out = agent
                .api
                .app
                .bsky
                .notification
                .update_seen(
                    update_seen::InputData { seen_at: new_seen_at.clone() }
                        .into(),
                )
                .await;
            if let Err(e) = out {
                log::error!("Cannot update notification seen time {}", e);
            }

            let notifs = notifications
                .into_iter()
                .map(|o| {
                    let Object { data, .. } = o;
                    Notification::new(data)
                })
                .collect::<Result<Vec<_>, String>>();
            let notifs = match notifs {
                Ok(o) => o,
                Err(e) => {
                    log::error!("{}", e);
                    return;
                }
            };

            if let Err(e) = fetch_missing_posts(&notifs, agent).await {
                log::error!("{}", e);
                return;
            }

            let mut seen_at = seen_at_.lock().unwrap();
            *seen_at = Some(new_seen_at);
            let mut feed = feed_.lock().unwrap();
            feed.notifs = notifs;
        });

        Self { feed, seen_at, is_terminate_worker }
    }

    pub fn spawn_worker(&self, agent: BskyAgent) {
        let feed = Arc::clone(&self.feed);
        let seen_at = Arc::clone(&self.seen_at);
        let is_terminate_worker = Arc::clone(&self.is_terminate_worker);
        tokio::spawn(async move {
            use atrium_api::app::bsky::notification::{
                list_notifications, update_seen,
            };

            while !*is_terminate_worker.lock().unwrap() {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

                let out = agent
                    .api
                    .app
                    .bsky
                    .notification
                    .list_notifications(
                        list_notifications::ParametersData {
                            cursor: None,
                            limit: Some(100.try_into().unwrap()),
                            priority: None,
                            reasons: Some(
                                [
                                    "like", "repost", "follow", "mention",
                                    "reply", "quote",
                                ]
                                .into_iter()
                                .map(String::from)
                                .collect(),
                            ),
                            seen_at: None,
                        }
                        .into(),
                    )
                    .await;
                let Object { data, .. } = match out {
                    Ok(o) => o,
                    Err(e) => {
                        log::error!("Cannot fetch notifications {}", e);
                        continue;
                    }
                };
                let list_notifications::OutputData { notifications, .. } = data;

                let new_seen_at = atrium_api::types::string::Datetime::now();
                let out = agent
                    .api
                    .app
                    .bsky
                    .notification
                    .update_seen(
                        update_seen::InputData { seen_at: new_seen_at.clone() }
                            .into(),
                    )
                    .await;
                if let Err(e) = out {
                    log::error!("Cannot update notification seen time {}", e);
                }

                let new_notifs = notifications
                    .into_iter()
                    .map(|o| {
                        let Object { data, .. } = o;
                        Notification::new(data)
                    })
                    .collect::<Result<Vec<Notification>, String>>();
                let new_notifs = match new_notifs {
                    Ok(o) => o,
                    Err(e) => {
                        log::error!("Cannot decode notification {}", e);
                        continue;
                    }
                };

                let out = fetch_missing_posts(&new_notifs, agent.clone()).await;
                if let Err(e) = out {
                    log::error!("Cannot update notification seen time {}", e);
                }

                // Lets hope there won't be race conditions
                {
                    let mut seen_at = seen_at.lock().unwrap();
                    *seen_at = Some(new_seen_at);
                    let mut feed = feed.lock().unwrap();
                    insert_notifs(&mut *feed, new_notifs);
                }
            }
        });
    }
}

async fn fetch_missing_posts(
    notifs: &Vec<Notification>,
    agent: BskyAgent,
) -> Result<(), String> {
    use atrium_api::app::bsky::feed::get_posts;

    let mut uris = notifs
        .iter()
        .filter_map(|n| match &n.record {
            Record::Like(u)
            | Record::Repost(u)
            | Record::Reply(u)
            | Record::Mention(u)
            | Record::Quote(u) => Some(u.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    uris.sort();
    uris.dedup();

    while !uris.is_empty() {
        let to_fetch = uris.drain(0..25.clamp(0, uris.len())).collect();
        let Object { data, .. } = agent
            .api
            .app
            .bsky
            .feed
            .get_posts(get_posts::ParametersData { uris: to_fetch }.into())
            .await
            .map_err(|e| e.to_string())?;
        let get_posts::OutputData { posts } = data;
        post_manager!().append(posts.iter().map(Post::from).collect());
    }

    Ok(())
}

fn insert_notifs(notifs: &mut Feed, new_notifs: Vec<Notification>) {
    if new_notifs.is_empty() {
        return;
    }
    if notifs.notifs.is_empty() {
        notifs.notifs = new_notifs;
        return;
    }

    let Some(overlap_idx) = new_notifs
        .iter()
        .rev()
        .find_map(|nn| notifs.notifs.iter().position(|n| n == nn))
    else {
        notifs.notifs = new_notifs;
        notifs.state = ListState::default();
        notifs.state.selected = Some(0);
        return;
    };

    let new_notifs = new_notifs
        .into_iter()
        .chain(
            notifs.notifs.iter().skip(overlap_idx + 1).map(Notification::clone),
        )
        .collect::<Vec<_>>();

    let autoscrolling = notifs.state.selected == Some(0);
    if autoscrolling {
        notifs.notifs = new_notifs;
        return;
    }

    notifs.state.selected = notifs.state.selected.map(|i| {
        let mut i = i;
        while i < notifs.notifs.len() {
            let n = &notifs.notifs[i];
            if let Some(i) = new_notifs.iter().position(|nn| nn == n) {
                return i;
            } else {
                i += 1;
            }
        }
        return 0;
    });
    notifs.notifs = new_notifs;
}

impl Drop for Notifications {
    fn drop(&mut self) {
        let mut t = self.is_terminate_worker.lock().unwrap();
        *t = true;
    }
}

impl EventReceiver for &mut Notifications {
    async fn handle_events(
        self,
        event: ratatui::crossterm::event::Event,
        agent: BskyAgent,
    ) -> crate::app::AppEvent {
        let Event::Key(key) = event.clone().into() else {
            return AppEvent::None;
        };
        match key.code {
            KeyCode::Backspace => return AppEvent::ColumnPopLayer,

            KeyCode::Char('j') => {
                let mut feed = self.feed.lock().unwrap();
                if feed.state.selected == None {
                    feed.state.selected = Some(0);
                } else {
                    feed.state.next();
                }
                return AppEvent::None;
            }
            KeyCode::Char('k') => {
                let mut feed = self.feed.lock().unwrap();
                feed.state.previous();
                return AppEvent::None;
            }

            KeyCode::Char('A') => {
                let n = {
                    let feed = self.feed.lock().unwrap();
                    let Some(i) = feed.state.selected else {
                        return AppEvent::None;
                    };
                    feed.notifs[i].clone()
                };
                let did = n.author.basic.did;
                let me = &agent.get_session().await.unwrap().did;
                let profile = ProfilePage::from_did(did, me, agent);
                return AppEvent::ColumnNewLayer(Column::ProfilePage(profile));
            }

            KeyCode::Enter => {
                let n = {
                    let feed = self.feed.lock().unwrap();
                    let Some(i) = feed.state.selected else {
                        return AppEvent::None;
                    };
                    feed.notifs[i].clone()
                };
                match &n.record {
                    Record::Like(u)
                    | Record::Repost(u)
                    | Record::Reply(u)
                    | Record::Mention(u)
                    | Record::Quote(u) => {
                        let view = ThreadView::from_uri(u.clone(), agent).await;
                        let view = match view {
                            Ok(view) => view,
                            Err(e) => {
                                log::error!("{}", e);
                                return AppEvent::None;
                            }
                        };
                        return AppEvent::ColumnNewLayer(Column::Thread(view));
                    }
                    Record::Follow => {
                        return AppEvent::None;
                    }
                }
            }

            _ => {
                let n = {
                    let feed = self.feed.lock().unwrap();
                    let Some(i) = feed.state.selected else {
                        return AppEvent::None;
                    };
                    feed.notifs[i].clone()
                };
                match &n.record {
                    Record::Like(u)
                    | Record::Repost(u)
                    | Record::Reply(u)
                    | Record::Mention(u)
                    | Record::Quote(u) => {
                        let post = post_manager!().at(u).unwrap();
                        return post.handle_events(event, agent).await;
                    }
                    Record::Follow => {
                        return AppEvent::None;
                    }
                }
            }
        }
    }
}

impl Widget for &mut Notifications {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        {
            let seen_at = self.seen_at.lock().unwrap();
            if *seen_at == None {
                Line::from("Loading").render(area, buf);
            }
        }
        let mut feed = self.feed.lock().unwrap();
        unsafe {
            let state = &mut feed.state as *mut ListState;
            let items = feed
                .notifs
                .iter()
                .map(NotificationWidget::new)
                .collect::<Vec<_>>();
            List::new(items.len(), move |context| {
                let item = items[context.index].clone();
                let style = if context.is_selected {
                    Style::default().bg(Color::Rgb(45, 50, 55))
                } else {
                    Style::default()
                };
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Color::DarkGray)
                    .style(style);
                let item = item.block(block).focused(context.is_selected);
                let height = item.line_count(area.width);
                (item, height)
            })
            .render(area, buf, &mut *state);
        }
    }
}
