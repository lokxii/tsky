use atrium_api::{
    app::bsky::{
        actor::defs::ProfileViewBasicData,
        feed::{defs::PostView, post},
        richtext::facet::MainFeaturesItem,
    },
    types::{string::Cid, TryFromUnknown, Union},
};
use bsky_sdk::BskyAgent;
use chrono::{DateTime, FixedOffset, Local};
use crossterm::event::{self, Event, KeyCode};
use std::{ops::Range, process::Command};

use crate::{app::AppEvent, embed::Embed, post_manager, post_manager_tx};

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
    Link(String),
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
            .filter_map(|facet| {
                let range = facet.index.byte_start..facet.index.byte_end;
                let Union::Refs(feature) = &facet.features[0] else {
                    log::warn!(
                        "Ignoring unknown feature found in post {}",
                        view.uri
                    );
                    return None;
                };
                let r#type = match feature {
                    MainFeaturesItem::Mention(_) => FacetType::Mention,
                    MainFeaturesItem::Link(link) => {
                        FacetType::Link(link.uri.clone())
                    }
                    MainFeaturesItem::Tag(_) => FacetType::Tag,
                };
                Some(Facet { r#type, range })
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

    pub fn handle_input_events(
        &self,
        event: event::Event,
        _: BskyAgent,
    ) -> AppEvent {
        let Event::Key(key) = event else {
            return AppEvent::None;
        };

        match key.code {
            KeyCode::Char(' ') => {
                if self.like.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnlikePost(
                            post_manager::DeleteRecordData {
                                post_uri: self.uri.clone(),
                                record_uri: self.like.uri.clone().unwrap(),
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
                                post_uri: self.uri.clone(),
                                post_cid: self.cid.clone(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unliking post"
                            );
                        });
                }
                return AppEvent::None;
            }

            KeyCode::Char('o') => {
                if self.repost.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnrepostPost(
                            post_manager::DeleteRecordData {
                                post_uri: self.uri.clone(),
                                record_uri: self.repost.uri.clone().unwrap(),
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
                                post_uri: self.uri.clone(),
                                post_cid: self.cid.clone(),
                            },
                        ))
                        .unwrap_or_else(|_| {
                            log::error!(
                                "Cannot send message to worker unrepost post"
                            );
                        });
                }
                return AppEvent::None;
            }

            KeyCode::Char('p') => {
                let post_uri = self.uri.split('/').collect::<Vec<_>>();
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
                return AppEvent::None;
            }

            KeyCode::Char('m') => {
                if self.embed.is_some() {
                    self.embed.as_ref().unwrap().open_media();
                }
                return AppEvent::None;
            }

            _ => return AppEvent::None,
        }
    }
}
