use std::process::Command;

use atrium_api::{
    app::bsky::feed::{
        defs::{
            ThreadViewPostData, ThreadViewPostParentRefs,
            ThreadViewPostRepliesItem,
        },
        get_post_thread::OutputThreadRefs as GetPostThreadOutput,
    },
    types::Union,
};
use bsky_sdk::BskyAgent;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    style::Color,
    text::Line,
    widgets::{Block, BorderType, Borders, Padding, StatefulWidget, Widget},
};

use crate::{
    column::Column,
    connected_list::{ConnectedList, ConnectedListContext, ConnectedListState},
    embed::Embed,
    post::Post,
    post_manager, post_manager_tx,
    post_widget::PostWidget,
    AppEvent,
};

fn parent_posts_rev(
    mut posts: Vec<String>,
    parent: Option<Union<ThreadViewPostParentRefs>>,
) -> Vec<String> {
    let Some(Union::Refs(ThreadViewPostParentRefs::ThreadViewPost(parent))) =
        parent
    else {
        return posts;
    };
    let ThreadViewPostData { parent, post, .. } = parent.data;
    let post = Post::from(&post);
    let post_uri = post.uri.clone();
    post_manager!().insert(post);

    posts.push(post_uri);
    return parent_posts_rev(posts, parent);
}

fn reply_posts(
    mut posts: Vec<String>,
    replies: Option<Vec<Union<ThreadViewPostRepliesItem>>>,
) -> Vec<String> {
    let Some(replies) = replies else {
        return posts;
    };
    for reply in replies.into_iter().rev() {
        let Union::Refs(ThreadViewPostRepliesItem::ThreadViewPost(reply)) =
            reply
        else {
            continue;
        };

        let ThreadViewPostData { post, replies, .. } = reply.data;
        let post = Post::from(&post);
        let post_uri = post.uri.clone();
        post_manager!().insert(post);

        posts.push(post_uri);
        posts = reply_posts(posts, replies);
        break;
    }

    return posts;
}

pub struct ThreadView {
    post_uri: String,
    parent: Vec<String>,
    replies: Vec<String>,
    state: ConnectedListState,
}

impl ThreadView {
    pub fn from(thread: ThreadViewPostData) -> ThreadView {
        let post = Post::from(&thread.post);
        let post_uri = post.uri.clone();
        post_manager!().insert(post);

        let mut parent = parent_posts_rev(vec![], thread.parent);
        parent.reverse();
        let replies = thread
            .replies
            .unwrap_or_default()
            .into_iter()
            .filter_map(|reply| match reply {
                Union::Refs(ThreadViewPostRepliesItem::ThreadViewPost(r)) => {
                    Some(r)
                }
                _ => None,
            })
            .map(|reply| {
                let post = Post::from(&reply.post);
                let post_uri = post.uri.clone();
                post_manager!().insert(post);
                return post_uri;
            })
            .collect();

        let l = parent.len();
        ThreadView {
            post_uri,
            parent,
            replies,
            state: ConnectedListState::new(Some(l)),
        }
    }

    pub fn selected(&self) -> Option<&String> {
        if let Some(i) = self.state.selected {
            if i < self.parent.len() {
                return Some(&self.parent[i]);
            }
            if i == self.parent.len() {
                return Some(&self.post_uri);
            }
            if i == self.parent.len() + 1 {
                return None;
            }
            if i > self.parent.len() + 1 {
                return Some(&self.replies[i - self.parent.len() - 2]);
            }
        }
        return None;
    }

    pub fn is_selecting_main_post(&self) -> bool {
        return self
            .state
            .selected
            .map(|i| i == self.parent.len())
            .unwrap_or(false);
    }

    pub async fn handle_input_events(&mut self, agent: BskyAgent) -> AppEvent {
        let Event::Key(key) = event::read().expect("Cannot read event") else {
            return AppEvent::None;
        };
        if key.kind != event::KeyEventKind::Press {
            return AppEvent::None;
        }

        match key.code {
            KeyCode::Backspace => return AppEvent::ColumnPopLayer,

            KeyCode::Char('q') => {
                return AppEvent::Quit;
            }

            KeyCode::Char('j') => {
                if let None = self.state.selected {
                    self.state.select(Some(0));
                } else {
                    self.state.next();
                }
                return AppEvent::None;
            }

            KeyCode::Char('k') => {
                self.state.previous();
                return AppEvent::None;
            }

            // Like
            KeyCode::Char(' ') => {
                let Some(selected) = self.selected() else {
                    return AppEvent::None;
                };
                let post = post_manager!().at(selected).unwrap();
                if post.like.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnlikePost(
                            post_manager::DeleteRecordData {
                                post_uri: post.uri.clone(),
                                record_uri: post.like.uri.clone().unwrap(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                } else {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::LikePost(
                            post_manager::CreateRecordData {
                                post_uri: post.uri.clone(),
                                post_cid: post.cid.clone(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                }
                return AppEvent::None;
            }

            // Repost
            KeyCode::Char('o') => {
                let Some(selected) = self.selected() else {
                    return AppEvent::None;
                };
                let post = post_manager!().at(selected).unwrap();
                if post.repost.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnrepostPost(
                            post_manager::DeleteRecordData {
                                post_uri: post.uri.clone(),
                                record_uri: post.repost.uri.clone().unwrap(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker repost post"
                            );
                        });
                } else {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::RepostPost(
                            post_manager::CreateRecordData {
                                post_uri: post.uri.clone(),
                                post_cid: post.cid.clone(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unrepost post"
                            );
                        });
                }
                return AppEvent::None;
            }

            KeyCode::Char('p') => {
                let Some(selected) = self.selected() else {
                    return AppEvent::None;
                };
                let post_uri = selected.split('/').collect::<Vec<_>>();
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
                return AppEvent::None;
            }

            KeyCode::Char('m') => {
                let Some(selected) = self.selected() else {
                    return AppEvent::None;
                };
                let Ok(_) = post_manager_tx!().send(
                    post_manager::RequestMsg::OpenMedia(selected.clone()),
                ) else {
                    return AppEvent::None;
                };
                return AppEvent::None;
            }

            KeyCode::Enter => {
                let Some(selected) = self.selected() else {
                    return AppEvent::None;
                };
                let uri = if self.is_selecting_main_post() {
                    let post = post_manager!().at(&self.post_uri).unwrap();
                    let Some(Embed::Record(crate::embed::Record::Post(post))) =
                        post.embed
                    else {
                        return AppEvent::None;
                    };
                    post.uri
                } else {
                    selected.clone()
                };

                let Ok(out) = agent.api.app.bsky.feed.get_post_thread(
                    atrium_api::app::bsky::feed::get_post_thread::ParametersData {
                        depth: Some(1.try_into().unwrap()),
                        parent_height: None,
                        uri,
                    }.into()).await else {
                    return AppEvent::None;
                };
                let Union::Refs(thread) = out.data.thread else {
                    log::error!("Unknown thread response");
                    return AppEvent::None;
                };

                match thread {
                    GetPostThreadOutput::AppBskyFeedDefsThreadViewPost(
                        thread,
                    ) => {
                        return AppEvent::ColumnNewLayer(Column::Thread(
                            ThreadView::from(thread.data),
                        ));
                    }
                    GetPostThreadOutput::AppBskyFeedDefsBlockedPost(_) => {
                        log::error!("Blocked thread");
                        return AppEvent::None;
                    }
                    GetPostThreadOutput::AppBskyFeedDefsNotFoundPost(_) => {
                        log::error!("Thread not found");
                        return AppEvent::None;
                    }
                }
            }

            _ => return AppEvent::None,
        }
    }
}

impl Widget for &mut ThreadView {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let parent_items =
            self.parent.clone().into_iter().map(|p| ThreadViewItem::Post(p));
        let reply_items =
            self.replies.clone().into_iter().map(|p| ThreadViewItem::Post(p));
        let items = parent_items
            .chain(std::iter::once(ThreadViewItem::Post(self.post_uri.clone())))
            .chain(std::iter::once(ThreadViewItem::Bar))
            .chain(reply_items)
            .collect::<Vec<_>>();

        ConnectedList::new(
            items.len(),
            move |context: ConnectedListContext| match &items[context.index] {
                ThreadViewItem::Post(uri) => {
                    let post = post_manager!().at(&uri).unwrap();
                    let item = PostWidget::new(post, context.is_selected, true);
                    let height = item.line_count(area.width) as u16;
                    return (ThreadViewItemWidget::Post(item), height);
                }
                ThreadViewItem::Bar => {
                    let item = Block::new()
                        .borders(Borders::TOP)
                        .title(Line::from("Replies").style(Color::Green))
                        .padding(Padding::uniform(1))
                        .border_type(BorderType::Double);
                    return (ThreadViewItemWidget::Bar(item), 1);
                }
            },
        )
        .connecting(vec![0..self.parent.len()])
        .render(area, buf, &mut self.state);
    }
}

enum ThreadViewItem {
    Post(String),
    Bar,
}

enum ThreadViewItemWidget<'a> {
    Post(PostWidget),
    Bar(Block<'a>),
}

impl<'a> Widget for ThreadViewItemWidget<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        match self {
            ThreadViewItemWidget::Post(p) => p.render(area, buf),
            ThreadViewItemWidget::Bar(b) => b.render(area, buf),
        }
    }
}
