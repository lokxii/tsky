use std::process::{Command, Stdio};

use atrium_api::{
    app::bsky::{
        embed::{record::ViewRecordRefs, record_with_media::ViewMediaRefs},
        feed::defs::PostViewEmbedRefs,
    },
    types::{Object, Union},
};

use crate::post::Author;

#[derive(Clone)]
pub enum Embed {
    Images(Vec<Image>),
    Video(Video),
    External(External),
    Record(Record),
}

impl Embed {
    pub fn from(e: &Union<PostViewEmbedRefs>) -> Embed {
        let e = match e {
            Union::Refs(e) => e,
            Union::Unknown(u) => panic!("Unknown embed type: {}", u.r#type),
        };
        match e {
            PostViewEmbedRefs::AppBskyEmbedImagesView(view) => {
                Embed::Images(view.images.iter().map(Image::from).collect())
            }
            PostViewEmbedRefs::AppBskyEmbedVideoView(view) => {
                Embed::Video(Video::from(view))
            }
            PostViewEmbedRefs::AppBskyEmbedExternalView(view) => {
                Embed::External(External::from(view))
            }
            PostViewEmbedRefs::AppBskyEmbedRecordView(view) => {
                Embed::Record(Record::from(&*view, None))
            }
            PostViewEmbedRefs::AppBskyEmbedRecordWithMediaView(view) => {
                let media = Some(EmbededPostMedia::from(&view.media));
                Embed::Record(Record::from(&view.record, media))
            }
        }
    }

    pub fn open_media(&self) {
        match self {
            Self::Images(images) => {
                let Result::Err(e) = Command::new("feh")
                    .args(["--output-dir", "/tmp"])
                    .args(["--zoom", "50%"])
                    .arg("--")
                    .args(images.iter().map(|i| i.url.clone()))
                    .stderr(Stdio::null())
                    .stdout(Stdio::null())
                    .spawn()
                else {
                    return;
                };
                log::error!("{:?}", e);
            }
            Self::Video(video) => {
                let Result::Err(e) = Command::new("vlc")
                    .arg(video.m3u8.clone())
                    .stderr(Stdio::null())
                    .stdout(Stdio::null())
                    .spawn()
                else {
                    return;
                };
                log::error!("{:?}", e);
            }
            Self::External(external) => {
                let Result::Err(e) = Command::new("xdg-open")
                    .arg(external.url.clone())
                    .stderr(Stdio::null())
                    .stdout(Stdio::null())
                    .spawn()
                else {
                    return;
                };
                log::error!("{:?}", e);
            }
            Self::Record(record) => {
                let Record::Post(post) = record else {
                    return;
                };
                let Some(media) = &post.media else {
                    return;
                };
                match media {
                    EmbededPostMedia::Images(images) => {
                        Embed::Images(images.clone()).open_media();
                    }
                    EmbededPostMedia::Video(video) => {
                        Embed::Video(video.clone()).open_media();
                    }
                    EmbededPostMedia::External(external) => {
                        Embed::External(external.clone()).open_media();
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub enum Record {
    Post(EmbededPost),
    Blocked,
    NotFound,
    Detached,
    // List(EmbededList),
    // Generator(EmbededGenerator),
    // Labler(EmbededLabler),
    // StarterPack(EmbededStarterPack),
    NotImplemented,
}

impl Record {
    pub fn from(
        view: &Object<atrium_api::app::bsky::embed::record::ViewData>,
        media: Option<EmbededPostMedia>,
    ) -> Record {
        let record = match &view.record {
            Union::Refs(e) => e,
            Union::Unknown(u) => {
                panic!("Unknown embed record type: {}", u.r#type)
            }
        };
        match record {
            ViewRecordRefs::ViewRecord(post) => {
                let atrium_api::types::Unknown::Object(record) = &post.value
                else {
                    panic!("Unknown embeded post value type");
                };
                let ipld_core::ipld::Ipld::String(text) = &*record["text"]
                else {
                    panic!("embeded text is not a string");
                };
                let text = text.replace("\t", "    ");

                let author = Author::from(&post.author);

                Record::Post(EmbededPost {
                    uri: post.uri.clone(),
                    author,
                    has_embed: post
                        .embeds
                        .as_ref()
                        .map(|v| v.len() > 0)
                        .unwrap_or(false),
                    media,
                    text,
                })
            }

            ViewRecordRefs::ViewBlocked(_) => Record::Blocked,
            ViewRecordRefs::ViewNotFound(_) => Record::NotFound,
            ViewRecordRefs::ViewDetached(_) => Record::Detached,
            _ => Record::NotImplemented,
        }
    }
}

#[derive(Clone)]
pub struct EmbededPost {
    pub uri: String,
    pub author: Author,
    pub has_embed: bool,
    pub media: Option<EmbededPostMedia>,
    pub text: String,
    // label
}

#[derive(Clone, Debug)]
pub enum EmbededPostMedia {
    Images(Vec<Image>),
    Video(Video),
    External(External),
}

impl EmbededPostMedia {
    pub fn from(
        media: &Union<
            atrium_api::app::bsky::embed::record_with_media::ViewMediaRefs,
        >,
    ) -> EmbededPostMedia {
        let media = match media {
            Union::Refs(e) => e,
            Union::Unknown(u) => {
                panic!("Unknown embed media type: {}", u.r#type)
            }
        };
        match media {
            ViewMediaRefs::AppBskyEmbedImagesView(data) => {
                EmbededPostMedia::Images(
                    data.images
                        .iter()
                        .map(|image| Image {
                            url: image.fullsize.clone(),
                            alt: image.alt.clone(),
                        })
                        .collect(),
                )
            }
            ViewMediaRefs::AppBskyEmbedVideoView(data) => {
                EmbededPostMedia::Video(Video {
                    m3u8: data.playlist.clone(),
                    alt: data.alt.clone().unwrap_or_default(),
                })
            }
            ViewMediaRefs::AppBskyEmbedExternalView(data) => {
                EmbededPostMedia::External(External {
                    url: data.external.uri.clone(),
                    title: data.external.title.clone(),
                    description: data.external.description.clone(),
                })
            }
        }
    }
}

impl Into<Embed> for EmbededPostMedia {
    fn into(self) -> Embed {
        match self {
            Self::Images(images) => Embed::Images(images),
            Self::Video(video) => Embed::Video(video),
            Self::External(external) => Embed::External(external),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Image {
    pub alt: String,
    pub url: String, // full size image
}

impl Image {
    pub fn from(
        image: &Object<atrium_api::app::bsky::embed::images::ViewImageData>,
    ) -> Image {
        Image { url: image.fullsize.clone(), alt: image.alt.clone() }
    }
}

#[derive(Clone, Debug)]
pub struct Video {
    pub alt: String,
    pub m3u8: String,
}

impl Video {
    pub fn from(
        video: &Object<atrium_api::app::bsky::embed::video::ViewData>,
    ) -> Video {
        Video {
            alt: video.alt.clone().unwrap_or_default(),
            m3u8: video.playlist.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct External {
    pub url: String,
    pub title: String,
    pub description: String,
}

impl External {
    pub fn from(
        external: &Object<atrium_api::app::bsky::embed::external::ViewData>,
    ) -> External {
        External {
            url: external.external.uri.clone(),
            title: external.external.title.clone(),
            description: external.external.description.clone(),
        }
    }
}

// #[derive(Clone)]
// struct EmbededList {
//     uri: String,
//     name: String,
//     description: String,
//     author: String,
//     handle: String,
// }
//
// #[derive(Clone)]
// struct EmbededGenerator {
//     uri: String,
//     name: String,
//     description: String,
//     author: String,
//     handle: String,
//     // label
// }
//
// #[derive(Clone)]
// struct EmbededLabler {
//     // No name?
//     uri: String,
//     // name: String,
//     // description: String,
//     author: String,
//     handle: String,
// }
//
// #[derive(Clone)]
// struct EmbededStarterPack {
//     uri: String,
//     author: String,
//     handle: String,
// }
