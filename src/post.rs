use std::ops::Range;

use atrium_api::{
    app::bsky::{
        actor::defs::ProfileViewBasicData,
        feed::{defs::PostView, post},
        richtext::facet::MainFeaturesItem,
    },
    types::{string::Cid, TryFromUnknown, Union},
};
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
pub struct Author {
    pub name: String,
    pub handle: String,
    pub labels: Vec<String>,
}

impl Author {
    pub fn from(author: &ProfileViewBasicData) -> Self {
        Author {
            name: author.display_name.clone().unwrap_or("(None)".to_string()),
            handle: author.handle.to_string(),
            labels: author
                .labels
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(|label| label.val.clone())
                .collect(),
        }
    }
}

#[derive(Clone)]
pub enum FacetType {
    Mention,
    Link,
    Tag,
}

#[derive(Clone)]
pub struct Facet {
    pub r#type: FacetType,
    pub range: Range<usize>,
}

#[derive(Clone)]
pub struct Post {
    pub uri: String,
    pub cid: Cid,
    pub author: Author,
    pub created_at: DateTime<FixedOffset>,
    pub text: String,
    pub like: LikeRepostViewer,
    pub repost: LikeRepostViewer,
    pub quote: u32,
    pub reply: u32,
    pub embed: Option<Embed>,
    pub labels: Vec<String>,
    pub facets: Vec<Facet>,
}

impl Post {
    pub fn from(view: &PostView) -> Post {
        let author = &view.author;

        let Ok(record) =
            post::RecordData::try_from_unknown(view.record.clone())
        else {
            panic!("Invalid record type");
        };

        let created_at = {
            let created_at = record.created_at.as_str();
            let dt = Local::now();
            let created_at_utc =
                DateTime::parse_from_rfc3339(created_at).unwrap().naive_local();
            DateTime::from_naive_utc_and_offset(created_at_utc, *dt.offset())
        };

        let text = record.text.replace("\t", "    ");
        let author = Author::from(author);

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

        let labels = view
            .labels
            .as_ref()
            .unwrap_or(&vec![])
            .iter()
            .map(|label| label.val.clone())
            .collect();

        let mut facets = record
            .facets
            .unwrap_or_default()
            .iter()
            .map(|facet| {
                let range = facet.index.byte_start..facet.index.byte_end;
                let Union::Refs(feature) = &facet.features[0] else {
                    panic!("Unknown feature type");
                };
                let r#type = match feature {
                    MainFeaturesItem::Mention(_) => FacetType::Mention,
                    MainFeaturesItem::Link(_) => FacetType::Link,
                    MainFeaturesItem::Tag(_) => FacetType::Tag,
                };
                Facet { r#type, range }
            })
            .collect::<Vec<_>>();

        facets.sort_by(|l, r| l.range.start.cmp(&r.range.start));

        return Post {
            uri: view.uri.clone(),
            cid: view.cid.clone(),
            author,
            created_at,
            text,
            like,
            quote: view.quote_count.unwrap_or(0) as u32,
            repost,
            reply: view.reply_count.unwrap_or(0) as u32,
            embed,
            labels,
            facets,
        };
    }
}
