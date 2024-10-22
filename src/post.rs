use atrium_api::{
    app::bsky::feed::defs::{
        FeedViewPost, FeedViewPostReasonRefs, PostView, ReplyRefParentRefs,
    },
    types::{string::Cid, Union},
};
use chrono::{DateTime, FixedOffset, Local};

use crate::Embed;

#[derive(PartialEq, Eq, Clone)]
pub struct RepostBy {
    pub author: String,
    pub handle: String,
}

#[derive(PartialEq, Eq, Clone)]
pub struct ReplyToAuthor {
    pub author: String,
    pub handle: String,
}

#[derive(PartialEq, Eq, Clone)]
pub enum Reply {
    Author(ReplyToAuthor),
    DeletedPost,
    BlockedUser,
}

#[derive(Clone)]
pub struct LikeRepostViewer {
    pub count: u32,
    pub uri: Option<String>,
}

impl LikeRepostViewer {
    fn new(count: Option<i64>, uri: Option<String>) -> LikeRepostViewer {
        LikeRepostViewer { count: count.unwrap_or(0) as u32, uri }
    }
}

#[derive(Clone)]
pub struct Post {
    pub uri: String,
    pub cid: Cid,
    pub author: String,
    pub handle: String,
    pub created_at: DateTime<FixedOffset>,
    pub text: String,
    pub reason: Option<RepostBy>,
    pub reply_to: Option<Reply>,
    pub like: LikeRepostViewer,
    pub repost: LikeRepostViewer,
    pub quote: u32,
    pub reply: u32,
    pub embed: Option<Embed>,
    // label
}

impl Post {
    pub fn from_post_view(view: &PostView) -> Post {
        let author = &view.author;
        let content = &view.record;

        let atrium_api::types::Unknown::Object(record) = content else {
            panic!("Invalid content type");
        };

        let ipld_core::ipld::Ipld::String(created_at) = &*record["createdAt"]
        else {
            panic!("createdAt is not a string")
        };

        let ipld_core::ipld::Ipld::String(text) = &*record["text"] else {
            panic!("text is not a string")
        };
        let text = text.clone();

        let dt = Local::now();
        let created_at_utc =
            DateTime::parse_from_rfc3339(created_at).unwrap().naive_local();
        let created_at =
            DateTime::from_naive_utc_and_offset(created_at_utc, *dt.offset());

        let like = match &view.viewer {
            Some(viewer) => {
                LikeRepostViewer::new(view.like_count, viewer.like.clone())
            }
            None => LikeRepostViewer::new(None, None),
        };

        let repost = match &view.viewer {
            Some(viewer) => {
                LikeRepostViewer::new(view.repost_count, viewer.repost.clone())
            }
            None => LikeRepostViewer::new(None, None),
        };

        let embed = view.embed.as_ref().map(Embed::from);

        return Post {
            uri: view.uri.clone(),
            cid: view.cid.clone(),
            author: author.display_name.clone().unwrap_or("(None)".to_string()),
            handle: author.handle.to_string(),
            created_at,
            text,
            reason: None,
            reply_to: None,
            like,
            quote: view.quote_count.unwrap_or(0) as u32,
            repost,
            reply: view.reply_count.unwrap_or(0) as u32,
            embed,
        };
    }

    pub fn from_feed_view_post(view: &FeedViewPost) -> Post {
        let post = Post::from_post_view(&view.post);

        let reason = view.reason.as_ref().map(|r| {
            let Union::Refs(r) = r else {
                panic!("Unknown reason type");
            };
            let FeedViewPostReasonRefs::ReasonRepost(r) = r;
            RepostBy {
                author: r.by.display_name.clone().unwrap_or(String::new()),
                handle: r.by.handle.to_string(),
            }
        });

        let reply_to = view.reply.as_ref().map(|r| {
            let Union::Refs(parent) = &r.data.parent else {
                panic!("Unknown parent type");
            };
            match parent {
                ReplyRefParentRefs::PostView(view) => {
                    Reply::Author(ReplyToAuthor {
                        author: view
                            .data
                            .author
                            .display_name
                            .clone()
                            .unwrap_or("".to_string()),
                        handle: view.data.author.handle.to_string(),
                    })
                }
                ReplyRefParentRefs::NotFoundPost(_) => Reply::DeletedPost,
                ReplyRefParentRefs::BlockedPost(_) => Reply::BlockedUser,
            }
        });

        return Post { reason, reply_to, ..post };
    }
}

impl PartialEq for Post {
    fn eq(&self, other: &Self) -> bool {
        return self.uri == other.uri && self.reason == other.reason;
    }
}

impl Eq for Post {}
