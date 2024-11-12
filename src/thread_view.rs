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
    layout::{Constraint, Layout},
    widgets::{Block, Borders, Padding, StatefulWidget, Widget},
};

use crate::{
    embed::Embed,
    list::{List, ListContext, ListState},
    post::Post,
    post_manager, post_manager_tx,
    post_widget::PostWidget,
    AppEvent,
};

#[derive(Clone)]
pub struct Thread {
    post_uris: Vec<String>,
    state: ListState,
}

fn parent_posts(
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
    return parent_posts(posts, parent);
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
    parent: Thread,
    replies: Vec<String>,
    state: ListState,
}

impl ThreadView {
    pub fn from(thread: ThreadViewPostData) -> ThreadView {
        let post = Post::from(&thread.post);
        let post_uri = post.uri.clone();
        post_manager!().insert(post);

        let parent_uris = parent_posts(vec![], thread.parent);
        let parent =
            Thread { post_uris: parent_uris, state: ListState::default() };
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

        ThreadView { post_uri, parent, replies, state: ListState::default() }
    }

    pub fn selected(&self) -> &String {
        if let Some(i) = self.state.selected {
            return &self.replies[i];
            // let thread = &self.replies[i];
            // return &thread.post_uris[thread.state.selected.unwrap()];
        } else {
            let thread = &self.parent;
            return thread
                .state
                .selected
                .map(|i| &thread.post_uris[i])
                .unwrap_or(&self.post_uri);
        }
    }

    pub async fn handle_input_events(
        &mut self,
        agent: BskyAgent,
    ) -> Result<AppEvent, Box<dyn std::error::Error>> {
        let Event::Key(key) = event::read()? else {
            return Ok(AppEvent::None);
        };
        if key.kind != event::KeyEventKind::Press {
            return Ok(AppEvent::None);
        }

        match key.code {
            KeyCode::Backspace => return Ok(AppEvent::ColumnPopLayer),

            KeyCode::Char('j') => {
                if let None = self.state.selected {
                    self.state.select(Some(0));
                } else {
                    self.state.next();
                }
                return Ok(AppEvent::None);
            }

            KeyCode::Char('k') => {
                if let Some(0) = self.state.selected {
                    self.state.select(None)
                } else {
                    self.state.previous();
                }
                return Ok(AppEvent::None);
            }

            // Like
            KeyCode::Char(' ') => {
                let post = post_manager!().at(self.selected()).unwrap();
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
                return Ok(AppEvent::None);
            }

            // Repost
            KeyCode::Char('o') => {
                let post = post_manager!().at(self.selected()).unwrap();
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
                return Ok(AppEvent::None);
            }

            KeyCode::Char('p') => {
                let post_uri = self.selected().split('/').collect::<Vec<_>>();
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
                return Ok(AppEvent::None);
            }

            KeyCode::Char('m') => {
                post_manager_tx!().send(
                    post_manager::RequestMsg::OpenMedia(
                        self.selected().clone(),
                    ),
                )?;

                return Ok(AppEvent::None);
            }

            KeyCode::Enter => {
                let uri = if self.state.selected.is_none()
                    && self.parent.state.selected.is_none()
                {
                    let post = post_manager!().at(&self.post_uri).unwrap();
                    let Some(Embed::Record(crate::embed::Record::Post(post))) =
                        post.embed
                    else {
                        return Ok(AppEvent::None);
                    };
                    post.uri
                } else {
                    self.selected().clone()
                };

                let out = agent.api.app.bsky.feed.get_post_thread(
                    atrium_api::app::bsky::feed::get_post_thread::ParametersData {
                        depth: Some(1.try_into().unwrap()),
                        parent_height: None,
                        uri,
                    }.into()).await?;
                let Union::Refs(thread) = out.data.thread else {
                    log::error!("Unknown thread response");
                    return Ok(AppEvent::None);
                };

                match thread {
                    GetPostThreadOutput::AppBskyFeedDefsThreadViewPost(
                        thread,
                    ) => {
                        return Ok(AppEvent::ColumnNewThreadLayer(
                            ThreadView::from(thread.data),
                        ));
                    }
                    GetPostThreadOutput::AppBskyFeedDefsBlockedPost(_) => {
                        log::error!("Blocked thread");
                        return Ok(AppEvent::None);
                    }
                    GetPostThreadOutput::AppBskyFeedDefsNotFoundPost(_) => {
                        log::error!("Thread not found");
                        return Ok(AppEvent::None);
                    }
                }
            }

            _ => return Ok(AppEvent::None),
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
        let post = post_manager!().at(&self.post_uri).unwrap();
        let post_widget =
            PostWidget::new(post, self.state.selected.is_none(), true);
        let post_height = post_widget.line_count(area.width);

        let [post_area, _, replies_area] = Layout::vertical([
            Constraint::Length(post_height),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(area);

        post_widget.render(post_area, buf);

        let replies_block =
            Block::new().borders(Borders::TOP).padding(Padding::uniform(1));
        let replies_block_inner = replies_block.inner(replies_area);
        replies_block.render(replies_area, buf);

        let replies = self.replies.clone();
        List::new(
            self.replies.len(),
            Box::new(move |context: ListContext| {
                let post = post_manager!().at(&replies[context.index]).unwrap();
                let item = PostWidget::new(post, context.is_selected, true);
                let height = item.line_count(replies_block_inner.width) as u16;
                return (item, height);
            }),
        )
        .render(replies_block_inner, buf, &mut self.state);
    }
}
