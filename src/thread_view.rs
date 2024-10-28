use std::process::Command;

use atrium_api::{
    app::bsky::feed::{
        defs::ThreadViewPostRepliesItem,
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

pub struct ThreadView {
    post_uri: String,
    replies: Vec<String>,
    state: ListState,
}

impl ThreadView {
    pub fn new(post: Post, replies: Vec<Post>) -> ThreadView {
        let post_uri = post.uri.clone();
        post_manager!().insert(post);

        let reply_uri = replies.iter().map(|r| r.uri.clone()).collect();
        post_manager!().append(replies);
        ThreadView { post_uri, replies: reply_uri, state: ListState::default() }
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
                let post = post_manager!()
                    .at(self
                        .state
                        .selected
                        .map(|i| &self.replies[i])
                        .unwrap_or(&self.post_uri))
                    .unwrap();
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
                let post = post_manager!()
                    .at(self
                        .state
                        .selected
                        .map(|i| &self.replies[i])
                        .unwrap_or(&self.post_uri))
                    .unwrap();
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
                let post_uri = self
                    .state
                    .selected
                    .map(|i| &self.replies[i])
                    .unwrap_or(&self.post_uri)
                    .split('/')
                    .collect::<Vec<_>>();
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
                let uri = self
                    .state
                    .selected
                    .map(|i| self.replies[i].clone())
                    .unwrap_or(self.post_uri.clone());
                post_manager_tx!()
                    .send(post_manager::RequestMsg::OpenMedia(uri))?;

                return Ok(AppEvent::None);
            }

            KeyCode::Enter => {
                let uri = if self.state.selected.is_none() {
                    let post = post_manager!().at(&self.post_uri).unwrap();
                    let Some(Embed::Record(crate::embed::Record::Post(post))) =
                        post.embed
                    else {
                        return Ok(AppEvent::None);
                    };
                    post.uri
                } else {
                    self.replies[self.state.selected.unwrap()].clone()
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
                        let post = Post::from(&thread.post);
                        let replies = thread.replies.as_ref().map(|replies| {
                            replies.iter().filter_map(|reply| {
                                let Union::Refs(reply) = reply else {
                                    return None;
                                };
                                if let ThreadViewPostRepliesItem::ThreadViewPost(post) = reply {
                                    Some(Post::from(&post.post))
                                } else {
                                    None
                                }
                            }).collect()
                        })
                        .unwrap_or_default();
                        return Ok(AppEvent::ColumnNewThreadLayer(
                            ThreadView::new(post, replies),
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
