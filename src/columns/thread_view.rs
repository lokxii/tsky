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
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    style::Color,
    text::Line,
    widgets::{BorderType, StatefulWidget, Widget},
};

use crate::{
    app::EventReceiver,
    columns::{
        facet_modal::{FacetModal, FacetModalItem, Link, Mention},
        Column,
    },
    components::{
        embed::{Embed, Record},
        list::{List, ListState},
        post::{post_widget::PostWidget, FacetType, Post},
        separation::Separation,
    },
    post_manager, AppEvent,
};

pub struct ThreadView {
    post_uri: String,
    parent: Vec<String>,
    replies: Vec<String>,
    state: ListState,
}

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

impl ThreadView {
    pub async fn from_uri(
        uri: String,
        agent: BskyAgent,
    ) -> Result<ThreadView, String> {
        let out = agent
            .api
            .app
            .bsky
            .feed
            .get_post_thread(
                atrium_api::app::bsky::feed::get_post_thread::ParametersData {
                    depth: Some(1.try_into().unwrap()),
                    parent_height: None,
                    uri,
                }
                .into(),
            )
            .await
            .map_err(|e| format!("Cannot fetch thread: {}", e))?;
        let Union::Refs(thread) = out.data.thread else {
            return Err("Unknown thread response".to_string());
        };

        match thread {
            GetPostThreadOutput::AppBskyFeedDefsThreadViewPost(thread) => {
                return Ok(ThreadView::new(thread.data));
            }
            GetPostThreadOutput::AppBskyFeedDefsBlockedPost(_) => {
                return Err("Blocked thread".to_string());
            }
            GetPostThreadOutput::AppBskyFeedDefsNotFoundPost(_) => {
                return Err("Thread not found".to_string());
            }
        }
    }

    fn new(thread: ThreadViewPostData) -> ThreadView {
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
        ThreadView { post_uri, parent, replies, state: ListState::new(Some(l)) }
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
}

impl EventReceiver for &mut ThreadView {
    async fn handle_events(
        self,
        event: event::Event,
        agent: BskyAgent,
    ) -> AppEvent {
        let Event::Key(key) = event else {
            return AppEvent::None;
        };

        match key.code {
            KeyCode::Backspace => return AppEvent::ColumnPopLayer,

            KeyCode::Char('q') => {
                return AppEvent::Quit;
            }

            KeyCode::Char('j') => {
                if let None = self.state.selected {
                    self.state.selected = Some(0);
                } else {
                    if self.state.selected.unwrap() == self.parent.len() {
                        if !self.replies.is_empty() {
                            self.state.next();
                            self.state.next();
                        }
                    } else {
                        self.state.next();
                    }
                }
                return AppEvent::None;
            }

            KeyCode::Char('k') => {
                if matches!(self.state.selected, Some(i) if i == self.parent.len() + 2)
                {
                    self.state.previous();
                    self.state.previous();
                } else {
                    self.state.previous();
                }
                return AppEvent::None;
            }

            KeyCode::Char('f') => {
                let post = post_manager!().at(&self.post_uri).unwrap();
                let facets = post
                    .facets
                    .iter()
                    .filter_map(|facet| match &facet.r#type {
                        FacetType::Link(url) => {
                            let text =
                                post.text[facet.range.clone()].to_string();
                            Some(FacetModalItem::Link(Link {
                                text,
                                url: url.clone(),
                            }))
                        }
                        FacetType::Mention(m) => {
                            let text =
                                post.text[facet.range.clone()].to_string();
                            Some(FacetModalItem::Mention(Mention {
                                text,
                                did: m.clone(),
                            }))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                return AppEvent::ColumnNewLayer(Column::FacetModal(
                    FacetModal {
                        links: facets,
                        state: ListState::new(Some(0)),
                    },
                ));
            }

            KeyCode::Enter => {
                let Some(selected) = self.selected() else {
                    return AppEvent::None;
                };
                let uri = if self.is_selecting_main_post() {
                    let post = post_manager!().at(&self.post_uri).unwrap();
                    let Some(Embed::Record(Record::Post(post))) = post.embed
                    else {
                        return AppEvent::None;
                    };
                    post.uri
                } else {
                    selected.clone()
                };

                let view = match ThreadView::from_uri(uri, agent).await {
                    Ok(view) => view,
                    Err(e) => {
                        log::error!("{}", e);
                        return AppEvent::None;
                    }
                };
                return AppEvent::ColumnNewLayer(Column::Thread(view));
            }

            _ => {
                let Some(selected) = self.selected() else {
                    return AppEvent::None;
                };
                let post = post_manager!().at(selected).unwrap();
                return post.handle_events(event, agent).await;
            }
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

        List::new(items.len(), move |context| match &items[context.index] {
            ThreadViewItem::Post(uri) => {
                let post = post_manager!().at(&uri).unwrap();
                let item = PostWidget::new(post)
                    .is_selected(context.is_selected)
                    .has_border(true);
                let height = item.line_count(area.width);
                return (ThreadViewItemWidget::Post(item), height);
            }
            ThreadViewItem::Bar => {
                let item = Separation::default()
                    .text(Line::from("Replies ").style(Color::Green))
                    .line(BorderType::Double)
                    .padding(1);
                return (ThreadViewItemWidget::Bar(item), 3);
            }
        })
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
    Bar(Separation<'a>),
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
