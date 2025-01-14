pub mod facets;
pub mod post_widget;

use std::{
    ops::Range,
    process::{Command, Stdio},
};

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
use ratatui::crossterm::event::{self, Event, KeyCode};

use crate::{
    app::{AppEvent, EventReceiver},
    columns::{
        composer_view::ComposerView,
        facet_modal::{FacetModal, Link},
        Column,
    },
    components::{
        composer, connected_list::ConnectedListState, embed::Embed,
        post_manager,
    },
    post_manager_tx,
};

#[derive(Clone)]
pub struct LikeRepostView {
    pub count: u32,
    pub uri: Option<String>,
}

impl LikeRepostView {
    fn new(count: Option<i64>, uri: Option<String>) -> LikeRepostView {
        LikeRepostView { count: count.unwrap_or(0) as u32, uri }
    }
}

#[derive(PartialEq, Eq, Clone)]
pub struct PostRef {
    pub cid: Cid,
    pub uri: String,
}

#[derive(PartialEq, Eq, Clone)]
pub struct ReplyRef {
    pub parent: PostRef,
    pub root: PostRef,
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
    pub like_view: LikeRepostView,
    pub repost_view: LikeRepostView,
    pub quote: u32,
    pub reply: u32,
    pub reply_to: Option<ReplyRef>,
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
                LikeRepostView::new(view.like_count, viewer.like.clone())
            }
            None => LikeRepostView::new(None, None),
        };

        let repost = match &view.viewer {
            Some(viewer) => {
                LikeRepostView::new(view.repost_count, viewer.repost.clone())
            }
            None => LikeRepostView::new(None, None),
        };

        let reply_to = record.reply.map(|reply| ReplyRef {
            root: PostRef {
                uri: reply.root.uri.clone(),
                cid: reply.root.cid.clone(),
            },
            parent: PostRef {
                uri: reply.parent.uri.clone(),
                cid: reply.parent.cid.clone(),
            },
        });

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
            like_view: like,
            quote: view.quote_count.unwrap_or(0) as u32,
            repost_view: repost,
            reply: view.reply_count.unwrap_or(0) as u32,
            reply_to,
            embed,
            labels,
            facets,
        };
    }
}

impl EventReceiver for &Post {
    async fn handle_events(
        self,
        event: event::Event,
        _: BskyAgent,
    ) -> AppEvent {
        let Event::Key(key) = event else {
            return AppEvent::None;
        };

        match key.code {
            KeyCode::Char(' ') => {
                if self.like_view.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnlikePost(
                            post_manager::DeleteRecordData {
                                post_uri: self.uri.clone(),
                                record_uri: self.like_view.uri.clone().unwrap(),
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
                if self.repost_view.uri.is_some() {
                    post_manager_tx!()
                        .send(post_manager::RequestMsg::UnrepostPost(
                            post_manager::DeleteRecordData {
                                post_uri: self.uri.clone(),
                                record_uri: self
                                    .repost_view
                                    .uri
                                    .clone()
                                    .unwrap(),
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

            KeyCode::Char('u') => {
                let root = self.reply_to.clone().map_or(
                    PostRef { uri: self.uri.clone(), cid: self.cid.clone() },
                    |reply| reply.root.clone(),
                );
                let reply_to = ReplyRef {
                    root,
                    parent: PostRef {
                        uri: self.uri.clone(),
                        cid: self.cid.clone(),
                    },
                };
                return AppEvent::ColumnNewLayer(Column::Composer(
                    ComposerView::new(
                        Some(reply_to),
                        composer::embed::Embed::None,
                    ),
                ));
            }

            KeyCode::Char('i') => {
                let post_ref =
                    PostRef { uri: self.uri.clone(), cid: self.cid.clone() };
                return AppEvent::ColumnNewLayer(Column::Composer(
                    ComposerView::new(
                        None,
                        composer::embed::Embed::Record(post_ref),
                    ),
                ));
            }

            KeyCode::Char('p') => {
                let post_uri = self.uri.split('/').collect::<Vec<_>>();
                let author = post_uri[2];
                let post_id = post_uri[4];
                let url = format!(
                    "https://bsky.app/profile/{}/post/{}",
                    author, post_id
                );
                if let Err(e) = Command::new("xdg-open")
                    .arg(url)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    log::error!("{:?}", e);
                }
                return AppEvent::None;
            }

            KeyCode::Char('a') => {
                let author = &self.author.handle;
                let url = format!("https://bsky.app/profile/{}", author);
                if let Err(e) = Command::new("xdg-open")
                    .arg(url)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
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

            KeyCode::Char('f') => {
                let links = self
                    .facets
                    .iter()
                    .filter_map(|facet| {
                        let FacetType::Link(url) = &facet.r#type else {
                            return None;
                        };
                        let text = self.text[facet.range.clone()].to_string();
                        Some(Link { text, url: url.clone() })
                    })
                    .collect::<Vec<_>>();
                return AppEvent::ColumnNewLayer(Column::FacetModal(
                    FacetModal {
                        links,
                        state: ConnectedListState::new(Some(0)),
                    },
                ));
            }

            _ => return AppEvent::None,
        }
    }
}
