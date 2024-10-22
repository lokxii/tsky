use atrium_api::{app::bsky::feed::defs::PostView, types::string::Cid};
use chrono::{DateTime, FixedOffset, Local};

use crate::embed::Embed;

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
    pub like: LikeRepostViewer,
    pub repost: LikeRepostViewer,
    pub quote: u32,
    pub reply: u32,
    pub embed: Option<Embed>,
    // label
}

impl Post {
    pub fn from(view: &PostView) -> Post {
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
            like,
            quote: view.quote_count.unwrap_or(0) as u32,
            repost,
            reply: view.reply_count.unwrap_or(0) as u32,
            embed,
        };
    }
}
