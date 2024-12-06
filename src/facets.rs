use regex::Regex;
use std::sync::OnceLock;

static RE_MENTION: OnceLock<Regex> = OnceLock::new();
static RE_URL: OnceLock<Regex> = OnceLock::new();
static RE_ENDING_PUNCTUATION: OnceLock<Regex> = OnceLock::new();
static RE_TRAILING_PUNCTUATION: OnceLock<Regex> = OnceLock::new();
static RE_TAG: OnceLock<Regex> = OnceLock::new();

#[derive(Clone)]
pub enum FacetFeature {
    Mention,
    Link,
    Tag,
}

#[derive(Clone, Copy)]
pub struct ByteSlice {
    byte_start: usize,
    byte_end: usize,
}

#[derive(Clone)]
pub struct Facet {
    pub feature: FacetFeature,
    pub index: ByteSlice,
}

pub fn detect_facets(text: &str) -> Vec<Facet> {
    let mut facets = Vec::new();
    // mentions
    {
        let re = RE_MENTION
            .get_or_init(|| Regex::new(r"(?:^|\s|\()@(([a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)\b").expect("invalid regex"));
        for capture in re.captures_iter(text) {
            let Some(m) = capture.get(1) else {
                continue;
            };
            facets.push(Facet {
                feature: FacetFeature::Mention,
                index: ByteSlice {
                    byte_end: m.end(),
                    byte_start: m.start() - 1,
                },
            });
        }
    }
    // links
    {
        let re = RE_URL.get_or_init(|| {
            Regex::new(
                r"(?:^|\s|\()((?:https?:\/\/[\S]+)|(?:(?<domain>[a-z][a-z0-9]*(?:\.[a-z0-9]+)+)[\S]*))",
            )
            .expect("invalid regex")
        });
        for capture in re.captures_iter(text) {
            let m = capture.get(1).expect("invalid capture");
            let mut uri = if let Some(domain) = capture.name("domain") {
                if !psl::suffix(domain.as_str().as_bytes())
                    .map_or(false, |suffix| suffix.is_known())
                {
                    continue;
                }
                format!("https://{}", m.as_str())
            } else {
                m.as_str().into()
            };
            let mut index =
                ByteSlice { byte_end: m.end(), byte_start: m.start() };
            // strip ending puncuation
            if (RE_ENDING_PUNCTUATION
                .get_or_init(|| {
                    Regex::new(r"[.,;:!?]$").expect("invalid regex")
                })
                .is_match(&uri))
                || (uri.ends_with(')') && !uri.contains('('))
            {
                uri.pop();
                index.byte_end -= 1;
            }
            facets.push(Facet { feature: FacetFeature::Link, index });
        }
    }
    // tags
    {
        let re = RE_TAG.get_or_init(|| {
            Regex::new(
                r"(?:^|\s)([#＃])([^\s\u00AD\u2060\u200A\u200B\u200C\u200D\u20e2]*[^\d\s\p{P}\u00AD\u2060\u200A\u200B\u200C\u200D\u20e2]+[^\s\u00AD\u2060\u200A\u200B\u200C\u200D\u20e2]*)?",
            )
            .expect("invalid regex")
        });
        for capture in re.captures_iter(text) {
            if let Some(tag) = capture.get(2) {
                // strip ending punctuation and any spaces
                let tag = RE_TRAILING_PUNCTUATION
                    .get_or_init(|| {
                        Regex::new(r"\p{P}+$").expect("invalid regex")
                    })
                    .replace(tag.as_str(), "");
                // look-around, including look-ahead and look-behind, is not supported in `regex`
                if tag.starts_with('\u{fe0f}') {
                    continue;
                }
                if tag.len() > 64 {
                    continue;
                }
                let leading = capture.get(1).expect("invalid capture");
                let index = ByteSlice {
                    byte_end: leading.end() + tag.len(),
                    byte_start: leading.start(),
                };
                facets.push(Facet { feature: FacetFeature::Tag, index });
            }
        }
    }
    facets
}

pub struct CharSlice {
    pub char_start: usize,
    pub char_end: usize,
}

impl CharSlice {
    pub fn from(s: &str, slice: ByteSlice) -> CharSlice {
        let start = (0..slice.byte_start)
            .fold(0, |acc, i| acc + s.is_char_boundary(i) as usize);
        let end = (0..slice.byte_end)
            .fold(0, |acc, i| acc + s.is_char_boundary(i) as usize);
        CharSlice { char_start: start, char_end: end }
    }
}
