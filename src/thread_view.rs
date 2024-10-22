use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::{Constraint, Layout},
    widgets::{Block, Borders, Padding, StatefulWidget, Widget},
};

use crate::{
    list::{List, ListContext, ListState},
    post::Post,
    post_widget::PostWidget,
    AppEvent,
};

pub struct ThreadView {
    pub post: Post,
    pub replies: Vec<Post>,
    pub state: ListState,
}

impl ThreadView {
    pub fn new(post: Post, replies: Vec<Post>) -> ThreadView {
        ThreadView { post, replies, state: ListState::default() }
    }

    pub async fn handle_input_events(
        &mut self,
    ) -> Result<AppEvent, Box<dyn std::error::Error>> {
        let Event::Key(key) = event::read()? else {
            return Ok(AppEvent::None);
        };
        if key.kind != event::KeyEventKind::Press {
            return Ok(AppEvent::None);
        }

        match key.code {
            KeyCode::Backspace => return Ok(AppEvent::ColumnPopLayer),

            KeyCode::Char('j') => {
                if let None = self.state.selected {
                    self.state.select(Some(0));
                } else {
                    self.state.next();
                }
                return Ok(AppEvent::None);
            }

            KeyCode::Char('k') => {
                if let Some(0) = self.state.selected {
                    self.state.select(None)
                } else {
                    self.state.previous();
                }
                return Ok(AppEvent::None);
            }

            _ => return Ok(AppEvent::None),
        }
    }
}

impl Widget for &mut ThreadView {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let post_widget = PostWidget::new(
            self.post.clone(),
            self.state.selected.is_none(),
            true,
        );
        let post_height = post_widget.line_count(area.width);

        let [post_area, _, replies_area] = Layout::vertical([
            Constraint::Length(post_height),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(area);

        post_widget.render(post_area, buf);

        let replies_block =
            Block::new().borders(Borders::TOP).padding(Padding::uniform(1));
        let replies_block_inner = replies_block.inner(replies_area);
        replies_block.render(replies_area, buf);

        let replies = self.replies.clone();
        List::new(
            self.replies.len(),
            Box::new(move |context: ListContext| {
                let item = PostWidget::new(
                    replies[context.index].clone(),
                    context.is_selected,
                    true,
                );
                let height = item.line_count(replies_block_inner.width) as u16;
                return (item, height);
            }),
        )
        .render(replies_block_inner, buf, &mut self.state);
    }
}
