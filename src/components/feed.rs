use atrium_api::{
    app::bsky::feed::defs::{
        FeedViewPost, FeedViewPostReasonRefs, ReplyRefParentRefs,
    },
    types::Union,
};
use itertools::Itertools;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};

use crate::{
    components::{
        list::{List, ListContext, ListState},
        post::{post_widget::PostWidget, Post},
    },
    post_manager,
};

pub struct Feed {
    pub posts: Vec<FeedPost>,
    pub state: ListState,
    pub cursor: Option<String>,
}

impl Feed {
    pub fn insert_new_posts<T>(&mut self, new_posts: T) -> bool
    where
        T: Iterator<Item = FeedPost> + Clone,
    {
        let new_posts = new_posts.collect::<Vec<_>>();
        if new_posts.len() == 0 {
            return true;
        }

        if self.posts.len() == 0 {
            self.posts = new_posts;
            self.state.select(Some(0));
            self.remove_duplicate();
            return true;
        }

        let autoscrolling = self.state.selected == Some(0);

        let Some(overlap_idx) = ({
            new_posts
                .iter()
                .rev()
                .find_map(|np| self.posts.iter().position(|p| p == np))
        }) else {
            self.posts = new_posts;
            self.state.select(Some(0));
            self.remove_duplicate();
            return true;
        };

        let new_posts = new_posts
            .into_iter()
            .chain(self.posts.clone().into_iter().skip(overlap_idx + 1))
            .collect::<Vec<_>>();

        if autoscrolling {
            self.posts = new_posts;
            self.remove_duplicate();
            self.state.select(Some(0));
            return false;
        }

        self.state.select(self.state.selected.map(|i| {
            let mut i = i;
            while i < self.posts.len() {
                let post = &self.posts[i];
                if let Some(i) = new_posts.iter().position(|p| p == post) {
                    return i;
                } else {
                    i += 1;
                }
            }
            return 0;
        }));
        self.posts = new_posts;
        self.remove_duplicate();

        return false;
    }

    pub fn append_old_posts<T>(&mut self, new_posts: T)
    where
        T: Iterator<Item = FeedPost> + Clone,
    {
        if self.posts.len() == 0 {
            return;
        }

        let mut new_posts = new_posts.collect();
        self.posts.append(&mut new_posts);
        self.remove_duplicate();
    }

    fn remove_duplicate(&mut self) {
        let new_view = self
            .posts
            .iter()
            .unique_by(|p| &p.post_uri)
            .map(FeedPost::clone)
            .collect::<Vec<_>>();

        if let Some(i) = self.state.selected {
            let mut j = i as i64;
            while j >= 0 {
                let selected_post = &self.posts[j as usize];
                let position = new_view.iter().position(|p| p == selected_post);
                if position.is_some() {
                    self.state.select(position);
                    self.posts = new_view;
                    return;
                } else {
                    j -= 1;
                }
            }
            let mut j = i + 1;
            while j < self.posts.len() {
                let selected_post = &self.posts[j as usize];
                let position = new_view.iter().position(|p| p == selected_post);
                if position.is_some() {
                    self.state.select(position);
                    self.posts = new_view;
                    return;
                } else {
                    j += 1;
                }
            }
            // don't change selected
        }

        self.posts = new_view;
    }
}

impl Widget for &mut Feed {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let width = area.width;
        let posts = self.posts.clone();

        List::new(self.posts.len(), |context: ListContext| {
            let post = &posts[context.index];
            let item = FeedPostWidget::new(post, context.is_selected);
            let height = item.line_count(width) as u16;
            return (item, height);
        })
        .render(area, buf, &mut self.state);
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RepostBy {
    pub author: String,
    pub handle: String,
}

#[derive(PartialEq, Eq, Clone)]
pub struct ReplyData {
    pub author: String,
    pub handle: String,
    pub following: bool,
}

#[derive(Clone)]
pub enum Reply {
    Reply(ReplyData),
    DeletedPost,
    BlockedUser,
}

#[derive(Clone)]
pub struct FeedPost {
    pub post_uri: String,
    pub reason: Option<RepostBy>,
    pub reply_to: Option<Reply>,
}

impl FeedPost {
    pub fn from(view: &FeedViewPost) -> FeedPost {
        let post = Post::from(&view.post);
        let uri = post.uri.clone();
        post_manager!().insert(post);

        let reason = match view.reason.as_ref() {
            Some(Union::Refs(FeedViewPostReasonRefs::ReasonRepost(r))) => {
                Some(RepostBy {
                    author: r.by.display_name.clone().unwrap_or(String::new()),
                    handle: r.by.handle.to_string(),
                })
            }
            Some(Union::Unknown(u)) => {
                panic!("Unknown reason type: {}", u.r#type)
            }
            _ => None,
        };

        let reply_to = view.reply.as_ref().map(|r| {
            let parent = match &r.parent {
                Union::Refs(e) => e,
                Union::Unknown(u) => {
                    panic!("Unknown parent type: {}", u.r#type)
                }
            };
            match parent {
                ReplyRefParentRefs::PostView(view) => {
                    let author = view
                        .author
                        .display_name
                        .clone()
                        .unwrap_or("".to_string());
                    let handle = view.author.handle.to_string();
                    #[rustfmt::skip]
                    let following = view.author.viewer.is_some()
                        && view .author.viewer.as_ref().unwrap().following.is_some();
                    Reply::Reply(ReplyData { author, handle, following })
                }
                ReplyRefParentRefs::NotFoundPost(_) => Reply::DeletedPost,
                ReplyRefParentRefs::BlockedPost(_) => Reply::BlockedUser,
            }
        });

        return FeedPost { post_uri: uri, reason, reply_to };
    }
}

impl PartialEq for FeedPost {
    fn eq(&self, other: &Self) -> bool {
        return self.post_uri == other.post_uri && self.reason == other.reason;
    }
}

impl Eq for FeedPost {}

struct FeedPostWidget<'a> {
    feed_post: &'a FeedPost,
    is_selected: bool,
    style: Style,
}

impl<'a> FeedPostWidget<'a> {
    fn new(feed_post: &'a FeedPost, is_selected: bool) -> Self {
        FeedPostWidget {
            feed_post,
            style: if is_selected {
                Style::default().bg(Color::Rgb(45, 50, 55))
            } else {
                Style::default()
            },
            is_selected,
        }
    }

    fn line_count(&self, width: u16) -> u16 {
        let post = post_manager!().at(&self.feed_post.post_uri).unwrap();
        PostWidget::new(post).line_count(width)
            + self.feed_post.reply_to.is_some() as u16
            + self.feed_post.reason.is_some() as u16
            + 2 // borders
    }
}

impl<'a> Widget for FeedPostWidget<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let borders = Block::bordered()
            .style(self.style)
            .border_set(symbols::border::ROUNDED);
        let inner_area = borders.inner(area);
        borders.render(area, buf);

        let post = post_manager!().at(&self.feed_post.post_uri).unwrap();
        let post_widget = PostWidget::new(post).is_selected(self.is_selected);

        let [top_area, post_area] = Layout::vertical([
            Constraint::Length(
                self.feed_post.reason.is_some() as u16
                    + self.feed_post.reply_to.is_some() as u16,
            ),
            Constraint::Length(post_widget.line_count(inner_area.width)),
        ])
        .areas(inner_area);

        let [repost_area, reply_area] = Layout::vertical([
            Constraint::Length(self.feed_post.reason.is_some() as u16),
            Constraint::Length(self.feed_post.reply_to.is_some() as u16),
        ])
        .areas(top_area);

        if let Some(repost) = &self.feed_post.reason {
            Line::from(Span::styled(
                format!("⭮ Reposted by {}", repost.author),
                Color::Green,
            ))
            .render(repost_area, buf);
        }

        if let Some(reply_to) = &self.feed_post.reply_to {
            let reply_to = match reply_to {
                Reply::Reply(a) => &a.author,
                Reply::DeletedPost => "[deleted post]",
                Reply::BlockedUser => "[blocked user]",
            };
            Line::from(Span::styled(
                format!("⮡ Reply to {}", reply_to),
                Color::Rgb(180, 180, 180),
            ))
            .render(reply_area, buf);
        }

        post_widget.render(post_area, buf);
    }
}
