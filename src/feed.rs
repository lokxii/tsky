use itertools::Itertools;
use ratatui::widgets::{StatefulWidget, Widget};

use crate::{
    list::{List, ListContext, ListState},
    post::Post,
    post_widget::PostWidget,
};

pub struct Feed {
    pub posts: Vec<Post>,
    pub state: ListState,
}

impl Feed {
    pub async fn insert_new_posts<T>(&mut self, new_posts: T) -> bool
    where
        T: Iterator<Item = Post> + Clone,
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

        // let selected = self.state.selected.map(|s| self.posts[s].clone());
        let new_last = new_posts.last().unwrap();
        let Some(overlap_idx) = self.posts.iter().position(|p| p == new_last)
        else {
            self.posts = new_posts;
            self.state.select(Some(0));
            self.remove_duplicate();
            return true;
        };

        // self.posts = new_posts
        let new_posts = new_posts
            .into_iter()
            .chain(self.posts.clone().into_iter().skip(overlap_idx + 1))
            .collect::<Vec<_>>();
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
        T: Iterator<Item = Post> + Clone,
    {
        if self.posts.len() == 0 {
            return;
        }

        let mut new_posts = new_posts.collect();
        self.posts.append(&mut new_posts);
        self.remove_duplicate();
    }

    fn remove_duplicate(&mut self) {
        let selected_post = self.state.selected.map(|i| self.posts[i].clone());
        let new_view = self
            .posts
            .iter()
            .unique_by(|p| &p.uri)
            .map(Post::clone)
            .collect::<Vec<_>>();

        self.state.select(selected_post.map(|post| {
            if let Some(i) = new_view.iter().position(|p| p.uri == post.uri) {
                return i;
            }
            panic!("Cannot decide which post to select after removing duplications");
        }));
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

        List::new(
            self.posts.len(),
            Box::new(move |context: ListContext| {
                let item = PostWidget::new(
                    posts[context.index].clone(),
                    context.is_selected,
                );
                let height = item.line_count(width) as u16;
                return (item, height);
            }),
        )
        .render(area, buf, &mut self.state);
    }
}
