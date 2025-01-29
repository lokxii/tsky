use std::sync::{Arc, Mutex};

use atrium_api::types::Object;
use bsky_sdk::BskyAgent;
use ratatui::{
    crossterm::event::{Event, KeyCode},
    style::{Color, Style},
    widgets::{Block, BorderType, StatefulWidget, Widget},
};

use crate::{
    app::{AppEvent, EventReceiver},
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
}

struct Feed {
    notifs: Vec<Notification>,
    state: ListState,
}

impl Notifications {
    pub async fn new(agent: BskyAgent) -> Result<Self, String> {
        use atrium_api::app::bsky::notification::{
            list_notifications, update_seen,
        };

        let seen_at = Arc::new(Mutex::new(None));
        let seen_at_lock = Arc::clone(&seen_at);
        let mut seen_at_lock = seen_at_lock.lock().unwrap();

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
                    seen_at: seen_at_lock.clone(),
                }
                .into(),
            )
            .await;
        let Object { data, .. } =
            out.map_err(|e| format!("Cannot fetch notifications {}", e))?;
        let list_notifications::OutputData {
            notifications,
            seen_at: new_seen_at,
            ..
        } = data;

        let out = agent
            .api
            .app
            .bsky
            .notification
            .update_seen(
                update_seen::InputData {
                    seen_at: new_seen_at.clone().unwrap(),
                }
                .into(),
            )
            .await;
        if let Err(e) = out {
            log::error!("Cannot update notification seen time {}", e);
        }

        *seen_at_lock = new_seen_at;

        let notifs = notifications
            .into_iter()
            .map(|o| {
                let Object { data, .. } = o;
                Notification::new(data)
            })
            .collect::<Result<Vec<_>, String>>()?;

        fetch_missing_posts(&notifs, agent).await?;

        let feed =
            Arc::new(Mutex::new(Feed { notifs, state: ListState::default() }));

        Ok(Self { feed, seen_at })
    }

    pub fn spawn_worker(&self, agent: BskyAgent) {
        let feed = Arc::clone(&self.feed);
        let seen_at = Arc::clone(&self.seen_at);
        tokio::spawn(async move {
            use atrium_api::app::bsky::notification::{
                list_notifications, update_seen,
            };

            loop {
                let old_seen_at = { seen_at.lock().unwrap().clone() };
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
                            seen_at: old_seen_at.clone(),
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
                let list_notifications::OutputData {
                    notifications,
                    seen_at: new_seen_at,
                    ..
                } = data;

                let out = agent
                    .api
                    .app
                    .bsky
                    .notification
                    .update_seen(
                        update_seen::InputData {
                            seen_at: new_seen_at.clone().unwrap(),
                        }
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
                        tokio::time::sleep(tokio::time::Duration::from_secs(5))
                            .await;
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
                    *seen_at = new_seen_at;
                    let mut feed = feed.lock().unwrap();
                    insert_notifs(&mut *feed, new_notifs);
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
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

impl EventReceiver for &mut Notifications {
    async fn handle_events(
        self,
        event: ratatui::crossterm::event::Event,
        agent: BskyAgent,
    ) -> crate::app::AppEvent {
        let Event::Key(key) = event.into() else {
            return AppEvent::None;
        };
        match key.code {
            KeyCode::Backspace => return AppEvent::ColumnPopLayer,
            _ => return AppEvent::None,
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
        let mut feed = self.feed.lock().unwrap();
        let mut state = feed.state.clone();
        let items =
            feed.notifs.iter().map(NotificationWidget::new).collect::<Vec<_>>();
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
            let item = item.block(block);
            let height = item.line_count(area.width);
            (item, height)
        })
        .render(area, buf, &mut state);
        feed.state = state;
    }
}
