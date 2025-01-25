pub mod composer_view;
pub mod facet_modal;
pub mod post_likes;
pub mod profile_page;
pub mod thread_view;
pub mod updating_feed;

use composer_view::ComposerView;
use facet_modal::FacetModal;
use post_likes::PostLikes;
use profile_page::ProfilePage;
use thread_view::ThreadView;
use updating_feed::UpdatingFeed;

pub enum Column {
    UpdatingFeed(UpdatingFeed),
    Thread(ThreadView),
    Composer(ComposerView),
    FacetModal(FacetModal),
    PostLikes(PostLikes),
    ProfilePage(ProfilePage),
}

pub struct ColumnStack {
    pub stack: Vec<Column>,
}

impl ColumnStack {
    pub fn from(stack: Vec<Column>) -> ColumnStack {
        ColumnStack { stack }
    }

    pub fn push(&mut self, column: Column) {
        self.stack.push(column);
    }

    pub fn pop(&mut self) -> Option<Column> {
        self.stack.pop()
    }

    pub fn last(&self) -> Option<&Column> {
        self.stack.last()
    }

    pub fn last_mut(&mut self) -> Option<&mut Column> {
        self.stack.last_mut()
    }
}
