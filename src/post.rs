use std::ops::Range;

use atrium_api::{
    app::bsky::{actor::defs::ProfileViewBasicData, feed::defs::PostView},
    types::string::Cid,
};
use chrono::{DateTime, FixedOffset, Local};
use ipld_core::ipld::Ipld;

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
        let content = &view.record;

        let atrium_api::types::Unknown::Object(record) = content else {
            panic!("Invalid content type");
        };

        let created_at = {
            let Ipld::String(created_at) = &*record["createdAt"] else {
                panic!("createdAt is not a string")
            };
            let dt = Local::now();
            let created_at_utc =
                DateTime::parse_from_rfc3339(created_at).unwrap().naive_local();
            DateTime::from_naive_utc_and_offset(created_at_utc, *dt.offset())
        };

        let text = {
            let Ipld::String(text) = &*record["text"] else {
                panic!("text is not a string")
            };
            text.replace("\t", "    ")
        };

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

        let mut facets = if !record.contains_key("facets") {
            vec![]
        } else {
            match &*record["facets"] {
                Ipld::List(facets) => facets
                    .iter()
                    .map(|facet| {
                        let Ipld::Map(facet) = facet else {
                            panic!("Unknown facet");
                        };
                        let Ipld::List(feature) = &facet["features"] else {
                            panic!("Unknown facet map");
                        };
                        let Ipld::Map(feature) = &feature[0] else {
                            panic!("Unknown facet map");
                        };
                        let Ipld::String(r#type) = &feature["$type"] else {
                            panic!("Unknown feature map");
                        };
                        let Ipld::Map(index) = &facet["index"] else {
                            panic!("Unknown facet map");
                        };
                        let Ipld::Integer(byte_start) = index["byteStart"]
                        else {
                            panic!("Unknown index map");
                        };
                        let Ipld::Integer(byte_end) = index["byteEnd"] else {
                            panic!("Unknown index map");
                        };
                        match r#type.as_str() {
                            "app.bsky.richtext.facet#mention" => Facet {
                                r#type: FacetType::Mention,
                                range: byte_start.try_into().unwrap()
                                    ..byte_end.try_into().unwrap(),
                            },
                            "app.bsky.richtext.facet#link" => Facet {
                                r#type: FacetType::Link,
                                range: byte_start.try_into().unwrap()
                                    ..byte_end.try_into().unwrap(),
                            },
                            "app.bsky.richtext.facet#tag" => Facet {
                                r#type: FacetType::Tag,
                                range: byte_start.try_into().unwrap()
                                    ..byte_end.try_into().unwrap(),
                            },
                            _ => panic!("Unknown feature type {}", r#type),
                        }
                    })
                    .collect::<Vec<_>>(),
                _ => panic!("facets is not a list"),
            }
        };
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
