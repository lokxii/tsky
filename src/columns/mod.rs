pub mod composer_view;
pub mod facet_modal;
pub mod notifications;
pub mod post_likes;
pub mod profile_page;
pub mod thread_view;
pub mod updating_feed;

use composer_view::ComposerView;
use facet_modal::FacetModal;
use notifications::Notifications;
use post_likes::PostLikes;
use profile_page::ProfilePage;
use thread_view::ThreadView;
use updating_feed::UpdatingFeed;

pub enum Column {
    UpdatingFeed(UpdatingFeed),
    Thread(ThreadView),
    Composer(ComposerView),
    FacetModal(FacetModal),
    Notifications(Notifications),
    PostLikes(PostLikes),
    ProfilePage(ProfilePage),
}

impl Column {
    pub fn name(&self) -> String {
        match self {
            Column::UpdatingFeed(_) => "Feed",
            Column::Thread(_) => "Thread",
            Column::Composer(_) => "Composer",
            Column::FacetModal(_) => "Facets",
            Column::Notifications(_) => "Notifications",
            Column::PostLikes(_) => "Likes",
            Column::ProfilePage(_) => "Profile",
        }
        .to_string()
    }
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

    pub fn last_mut(&mut self) -> Option<&mut Column> {
        self.stack.last_mut()
    }
}
