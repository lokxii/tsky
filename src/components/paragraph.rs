use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Widget},
};
use unicode_width::UnicodeWidthStr;

#[derive(Debug)]
pub struct Paragraph<'a> {
    block: Option<Block<'a>>,
    wrap: bool,
    text: Text<'a>,
    scroll_y: usize,
}

impl<'a> Paragraph<'a> {
    pub fn new<T: Into<Text<'a>>>(text: T) -> Self {
        Self { block: None, wrap: true, text: text.into(), scroll_y: 0 }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll_y = scroll;
        self
    }

    // without considering block
    pub fn line_count(&self, width: u16) -> usize {
        if width < 1 {
            return 0;
        }
        if !self.wrap {
            return self.text.height();
        }

        return break_lines(&self.text.lines, width).len();
    }
}

impl<'a> Widget for Paragraph<'a> {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let area = if let Some(block) = self.block {
            let inner_area = block.inner(area);
            block.render(area, buf);
            inner_area
        } else {
            area
        };

        let lines = if self.wrap {
            break_lines(&self.text.lines, area.width)
        } else {
            self.text.lines
        };

        log::info!(
            "{:?}",
            lines.iter().map(Line::to_string).collect::<Vec<_>>()
        );

        lines
            .into_iter()
            .skip(self.scroll_y)
            .take(area.height as usize)
            .enumerate()
            .for_each(|(i, l)| {
                let a = Rect {
                    x: area.x,
                    y: area.y + i as u16,
                    height: 1,
                    width: area.width,
                };
                l.render(a, buf);
            });
    }
}

fn break_lines<'a>(lines: &'a Vec<Line<'a>>, width: u16) -> Vec<Line<'a>> {
    let mut new_lines = vec![];

    for line in lines {
        let mut words = vec![];
        let last_word = line
            .spans
            .iter()
            .flat_map(|s| s.styled_graphemes(Style::default()))
            .fold(Line::from(""), |acc, e| {
                let acc_s = acc.to_string();
                let e_space = e.symbol.chars().all(char::is_whitespace);
                let acc_space = acc_s.chars().all(char::is_whitespace);
                if e_space || !acc_s.is_empty() && acc_space {
                    words.push(acc);
                    return Line::from(Span::styled(e.symbol, e.style));
                } else {
                    return acc + Span::styled(e.symbol, e.style);
                }
            });
        words.push(last_word);

        let last_acc = words.into_iter().fold(Line::from(""), |acc, e| {
            let acc_w = acc.to_string().width_cjk();
            let e_w = e.to_string().width_cjk();
            if acc_w + e_w <= width as usize {
                return e.spans.into_iter().fold(acc, |acc, e| acc + e);
            }

            if !acc.to_string().is_empty() {
                new_lines.push(acc);
            }
            let mut acc = Line::from("");

            for g in e.styled_graphemes(Style::default()) {
                let acc_w = acc.to_string().width_cjk();
                if acc_w + g.symbol.width_cjk() > width as usize {
                    new_lines.push(acc);
                    acc =
                        Line::from(Span::styled(g.symbol.to_string(), g.style));
                } else {
                    acc += Span::styled(g.symbol.to_string(), g.style);
                }
            }
            return acc;
        });
        new_lines.push(last_acc);
    }

    return new_lines;
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn no_breaking_line() {
        let lines = vec![Line::from("abc")];
        let lines = break_lines(&lines, 3);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "abc");
    }

    #[test]
    fn no_breaking_line_with_space() {
        let lines = vec![Line::from("abc  def")];
        let lines = break_lines(&lines, 8);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "abc  def");
    }

    #[test]
    fn break_line() {
        let lines = vec![Line::from("abcd")];
        let lines = break_lines(&lines, 3);
        dbg!(lines.iter().map(Line::to_string).collect::<Vec<_>>());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), "abc");
        assert_eq!(lines[1].to_string(), "d");
    }

    #[test]
    fn middle_of_word() {
        let lines = vec![Line::from("abc def")];
        let lines = break_lines(&lines, 6);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), "abc ");
        assert_eq!(lines[1].to_string(), "def");
    }

    #[test]
    fn break_line_with_space() {
        let lines = vec![Line::from("abc def")];
        let lines = break_lines(&lines, 4);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), "abc ");
        assert_eq!(lines[1].to_string(), "def");
    }

    #[test]
    fn long_line() {
        let lines = vec![Line::from("abcdef")];
        let lines = break_lines(&lines, 4);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), "abcd");
        assert_eq!(lines[1].to_string(), "ef");
    }

    #[test]
    fn wide_char_one_line() {
        let lines = vec![Line::from("クソワロタ")];
        let lines = break_lines(&lines, 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "クソワロタ");
    }
    #[test]
    fn wide_char_break_line() {
        let lines = vec![Line::from("クソワロタ")];
        let lines = break_lines(&lines, 5);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].to_string(), "クソ");
        assert_eq!(lines[1].to_string(), "ワロ");
        assert_eq!(lines[2].to_string(), "タ");
    }
}
