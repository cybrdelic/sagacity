use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};
use textwrap::wrap;

pub struct ChatMessage {
    content: String,
    from_user: bool,
}

impl ChatMessage {
    pub fn new(content: String, from_user: bool) -> Self {
        Self { content, from_user }
    }

    pub fn render(&self, area: Rect) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let style = Style::default().fg(if self.from_user {
            Color::Yellow
        } else {
            Color::Green
        });

        let indent = if self.from_user { "  " } else { "" };
        lines.push(Line::from(vec![
            Span::styled(indent.to_string(), style),
            Span::styled("│ ".to_string(), style),
        ]));

        let mut in_code_block = false;
        let mut code_buffer = String::new();
        let mut text_buffer = String::new();

        for line in self.content.lines() {
            if line.trim().starts_with("```") {
                if !text_buffer.is_empty() {
                    let wrapped = wrap(&text_buffer, (area.width as usize).saturating_sub(4));
                    for wrapped_line in wrapped {
                        lines.push(Line::from(vec![
                            Span::styled(indent.to_string(), style),
                            Span::styled("│ ".to_string(), style),
                            Span::styled(wrapped_line.to_string(), style),
                        ]));
                    }
                    text_buffer.clear();
                }

                if !code_buffer.is_empty() {
                    for code_line in code_buffer.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(indent.to_string(), style),
                            Span::styled("│ ".to_string(), style),
                            Span::styled("▎".to_string(), Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                format!(" {}", code_line),
                                Style::default().fg(Color::Rgb(209, 154, 102)),
                            ),
                        ]));
                    }
                    code_buffer.clear();
                }

                in_code_block = !in_code_block;
                continue;
            }

            if in_code_block {
                code_buffer.push_str(line);
                code_buffer.push('\n');
            } else {
                text_buffer.push_str(line);
                text_buffer.push('\n');
            }
        }

        if !text_buffer.is_empty() {
            let wrapped = wrap(&text_buffer, (area.width as usize).saturating_sub(4));
            for wrapped_line in wrapped {
                lines.push(Line::from(vec![
                    Span::styled(indent.to_string(), style),
                    Span::styled("│ ".to_string(), style),
                    Span::styled(wrapped_line.to_string(), style),
                ]));
            }
        }

        if !code_buffer.is_empty() {
            for code_line in code_buffer.lines() {
                lines.push(Line::from(vec![
                    Span::styled(indent.to_string(), style),
                    Span::styled("│ ".to_string(), style),
                    Span::styled("▎".to_string(), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(" {}", code_line),
                        Style::default().fg(Color::Rgb(209, 154, 102)),
                    ),
                ]));
            }
        }

        lines.push(Line::from(vec![
            Span::styled(indent.to_string(), style),
            Span::styled("╰─".to_string(), style),
        ]));

        lines
    }
}
