use atrium_api::{
    app::bsky::actor::defs::ProfileViewBasicData, types::string::Did,
};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Widget},
};

use crate::components::paragraph::Paragraph;

#[derive(Clone)]
pub struct ActorBasic {
    pub did: Did,
    pub name: String,
    pub handle: String,
    pub labels: Vec<String>,
}

impl ActorBasic {
    pub fn from(author: &ProfileViewBasicData) -> Self {
        ActorBasic {
            name: author.display_name.clone().unwrap_or("(None)".to_string()),
            handle: author.handle.to_string(),
            labels: author
                .labels
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(|label| label.val.clone())
                .collect(),
            did: author.did.clone(),
        }
    }
}

pub struct ActorBasicWidget<'a> {
    basic: &'a ActorBasic,
    focused: bool,
}

impl<'a> ActorBasicWidget<'a> {
    pub fn new(basic: &'a ActorBasic) -> Self {
        ActorBasicWidget { basic, focused: false }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl<'a> Widget for ActorBasicWidget<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let style = if self.focused {
            Style::default().bg(Color::Rgb(45, 50, 55))
        } else {
            Style::default()
        };
        let labels = self
            .basic
            .labels
            .iter()
            .fold(String::new(), |acc, e| format!("{} [{}]", acc, e));
        (Span::styled(self.basic.name.clone(), Color::Cyan)
            + Span::styled(format!(" @{}", self.basic.handle), Color::Gray)
            + Span::styled(labels, Color::LightRed))
        .style(style)
        .render(area, buf);
    }
}

#[derive(Clone)]
pub struct Actor {
    pub basic: ActorBasic,
    pub description: Option<String>,
}

impl Actor {
    pub fn new(
        data: atrium_api::app::bsky::actor::defs::ProfileViewData,
    ) -> Self {
        let atrium_api::app::bsky::actor::defs::ProfileViewData {
            associated,
            avatar,
            created_at,
            did,
            display_name,
            handle,
            labels,
            viewer,
            description,
            ..
        } = data;
        let basic = atrium_api::app::bsky::actor::defs::ProfileViewBasicData {
            associated,
            avatar,
            created_at,
            did,
            display_name,
            handle,
            labels,
            viewer,
        };
        Actor { basic: ActorBasic::from(&basic), description }
    }
}

pub struct ActorWidget<'a> {
    actor: &'a Actor,
    block: Option<Block<'a>>,
    focused: bool,
}

impl<'a> ActorWidget<'a> {
    pub fn new(actor: &'a Actor) -> Self {
        ActorWidget { actor, block: None, focused: false }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn line_count(&self, width: u16) -> usize {
        let b = self.block.is_some() as usize * 2;
        1 + b
            + Paragraph::new(
                self.actor.description.clone().unwrap_or(String::new()),
            )
            .line_count(width - b as u16)
    }
}

impl<'a> Widget for ActorWidget<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let style = if self.focused {
            Style::default().bg(Color::Rgb(45, 50, 55))
        } else {
            Style::default()
        };
        let area = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.style(style).border_style(style).render(area, buf);
            inner
        } else {
            area
        };

        let [basic_area, description_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)])
                .areas(area);
        ActorBasicWidget::new(&self.actor.basic)
            .focused(self.focused)
            .render(basic_area, buf);
        Paragraph::new(self.actor.description.clone().unwrap_or(String::new()))
            .render(description_area, buf);
    }
}

#[derive(Clone)]
pub struct ActorDetailed {
    actor: Actor,
    following_count: u64,
    follower_count: u64,
    posts_count: u64,
    avatar: Option<String>,
    banner: Option<String>,
    blocking: bool,
    blocked_by: bool,
    following: bool,
    followed_by: bool,
    muted: bool,
}

impl ActorDetailed {
    pub fn new(
        data: atrium_api::app::bsky::actor::defs::ProfileViewDetailedData,
    ) -> Self {
        let atrium_api::app::bsky::actor::defs::ProfileViewDetailedData {
            associated,
            avatar,
            banner,
            created_at,
            description,
            did,
            display_name,
            handle,
            indexed_at,
            labels,
            viewer,
            followers_count,
            follows_count,
            posts_count,
            ..
        } = data;
        let actor = atrium_api::app::bsky::actor::defs::ProfileViewData {
            associated,
            avatar: avatar.clone(),
            created_at,
            description,
            did,
            display_name,
            handle,
            indexed_at,
            labels,
            viewer: viewer.clone(),
        };
        let actor = Actor::new(actor);
        ActorDetailed {
            actor,
            follower_count: followers_count.unwrap_or(0) as u64,
            following_count: follows_count.unwrap_or(0) as u64,
            posts_count: posts_count.unwrap_or(0) as u64,
            avatar,
            banner,
            blocking: viewer
                .as_ref()
                .map(|v| v.blocking.is_some())
                .unwrap_or(false),
            blocked_by: viewer
                .as_ref()
                .map(|v| v.blocked_by.unwrap_or(false))
                .unwrap_or(false),
            following: viewer
                .as_ref()
                .map(|v| v.following.is_some())
                .unwrap_or(false),
            followed_by: viewer
                .as_ref()
                .map(|v| v.followed_by.is_some())
                .unwrap_or(false),
            muted: viewer
                .as_ref()
                .map(|v| v.muted.unwrap_or(false))
                .unwrap_or(false),
        }
    }
}
