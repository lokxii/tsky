use ratatui::{
    layout::{Constraint, Layout},
    style::Color,
    widgets::{block::Title, Block, BorderType, Borders, Widget},
};

#[derive(Default, Clone)]
pub struct Separation<'a> {
    text: Title<'a>,
    line: BorderType,
    padding: u16,
}

impl<'a> Separation<'a> {
    pub fn text(mut self, text: impl Into<Title<'a>>) -> Self {
        self.text = text.into();
        self
    }

    pub fn line(mut self, line: BorderType) -> Self {
        self.line = line;
        self
    }

    pub fn padding(mut self, padding: u16) -> Self {
        self.padding = padding;
        self
    }

    pub fn line_count(&self, _: u16) -> u16 {
        self.padding * 2 + 1
    }
}

impl<'a> Widget for Separation<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let [area] = Layout::vertical([Constraint::Length(1)])
            .vertical_margin(self.padding)
            .areas(area);
        Block::new()
            .borders(Borders::TOP)
            .title(self.text)
            .border_type(self.line)
            .border_style(Color::DarkGray)
            .render(area, buf);
    }
}
