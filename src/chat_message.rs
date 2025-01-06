use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::BorderType,
};
use textwrap::wrap;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    content: String,
    from_user: bool,
    timestamp: DateTime<Local>,
    status: MessageStatus,
    reactions: Vec<Reaction>,
}

#[derive(Debug, Clone)]
pub enum MessageStatus {
    Sending,
    Sent,
    Delivered,
    Read,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Reaction {
    emoji: String,
    count: u32,
}

impl ChatMessage {
    pub fn new(content: String, from_user: bool) -> Self {
        Self {
            content,
            from_user,
            timestamp: Local::now(),
            status: MessageStatus::Sending,
            reactions: Vec::new(),
        }
    }

    pub fn render(&self, area: Rect) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let base_style = self.get_base_style();

        // Add header with timestamp and status
        self.render_header(&mut lines, area, base_style);

        // Process message content
        self.render_content(&mut lines, area, base_style);

        // Add reactions if any
        self.render_reactions(&mut lines, base_style);

        // Add footer
        self.render_footer(&mut lines, base_style);

        lines
    }

    fn get_base_style(&self) -> Style {
        let mut style = Style::default().fg(if self.from_user {
            Color::Rgb(255, 223, 128) // Warmer yellow
        } else {
            Color::Rgb(144, 238, 144) // Softer green
        });

        match self.status {
            MessageStatus::Failed => style = style.fg(Color::Red).add_modifier(Modifier::DIM),
            MessageStatus::Sending => style = style.add_modifier(Modifier::DIM),
            _ => {}
        }

        style
    }

    fn render_header(&self, lines: &mut Vec<Line<'static>>, area: Rect, style: Style) {
        let timestamp = self.timestamp.format("%H:%M").to_string();
        let status_icon = self.get_status_icon();
        let indent = if self.from_user { "  " } else { "" };

        let header_line = Line::from(vec![
            Span::styled(indent.to_string(), style),
            Span::styled("┌─".to_string(), style),
            Span::styled(timestamp, style.add_modifier(Modifier::DIM)),
            Span::styled(" ", style),
            Span::styled(status_icon, style),
        ]);

        lines.push(header_line);
    }

    fn render_content(&self, lines: &mut Vec<Line<'static>>, area: Rect, style: Style) {
        let indent = if self.from_user { "  " } else { "" };
        let mut in_code_block = false;
        let mut code_buffer = String::new();
        let mut text_buffer = String::new();

        for line in self.content.lines() {
            if line.trim().starts_with("```") {
                self.flush_text_buffer(lines, &text_buffer, area, style, indent);
                self.flush_code_buffer(lines, &code_buffer, style, indent);
                text_buffer.clear();
                code_buffer.clear();
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

        self.flush_text_buffer(lines, &text_buffer, area, style, indent);
        self.flush_code_buffer(lines, &code_buffer, style, indent);
    }

    fn flush_text_buffer(
        &self,
        lines: &mut Vec<Line<'static>>,
        buffer: &str,
        area: Rect,
        style: Style,
        indent: &str,
    ) {
        if buffer.is_empty() {
            return;
        }

        let wrap_width = (area.width as usize).saturating_sub(4);
        let wrapped = wrap(buffer, wrap_width);

        for wrapped_line in wrapped {
            let line = Line::from(vec![
                Span::styled(indent.to_string(), style),
                Span::styled("│ ".to_string(), style),
                Span::styled(wrapped_line.to_string(), style),
            ]);
            lines.push(line);
        }
    }

    fn flush_code_buffer(
        &self,
        lines: &mut Vec<Line<'static>>,
        buffer: &str,
        style: Style,
        indent: &str,
    ) {
        if buffer.is_empty() {
            return;
        }

        let code_style = Style::default()
            .fg(Color::Rgb(209, 154, 102))
            .add_modifier(Modifier::BOLD);

        for code_line in buffer.lines() {
            let line = Line::from(vec![
                Span::styled(indent.to_string(), style),
                Span::styled("│ ".to_string(), style),
                Span::styled("▎".to_string(), Style::default().fg(Color::DarkGray)),
                Span::styled(format!(" {}", code_line), code_style),
            ]);
            lines.push(line);
        }
    }

    fn render_reactions(&self, lines: &mut Vec<Line<'static>>, style: Style) {
        if self.reactions.is_empty() {
            return;
        }

        let indent = if self.from_user { "  " } else { "" };
        let mut reaction_line = Vec::new();
        reaction_line.push(Span::styled(indent.to_string(), style));
        reaction_line.push(Span::styled("│ ".to_string(), style));

        for (i, reaction) in self.reactions.iter().enumerate() {
            if i > 0 {
                reaction_line.push(Span::styled(" ", style));
            }
            reaction_line.push(Span::styled(
                format!("{} {}", reaction.emoji, reaction.count),
                style.add_modifier(Modifier::BOLD),
            ));
        }

        lines.push(Line::from(reaction_line));
    }

    fn render_footer(&self, lines: &mut Vec<Line<'static>>, style: Style) {
        let indent = if self.from_user { "  " } else { "" };
        lines.push(Line::from(vec![
            Span::styled(indent.to_string(), style),
            Span::styled("╰─".to_string(), style),
        ]));
    }

    fn get_status_icon(&self) -> String {
        match self.status {
            MessageStatus::Sending => "○".to_string(),
            MessageStatus::Sent => "●".to_string(),
            MessageStatus::Delivered => "✓".to_string(),
            MessageStatus::Read => "✓✓".to_string(),
            MessageStatus::Failed => "✗".to_string(),
        }
    }

    // Public methods for interaction
    pub fn set_status(&mut self, status: MessageStatus) {
        self.status = status;
    }

    pub fn add_reaction(&mut self, emoji: String) {
        if let Some(reaction) = self.reactions.iter_mut().find(|r| r.emoji == emoji) {
            reaction.count += 1;
        } else {
            self.reactions.push(Reaction { emoji, count: 1 });
        }
    }

    pub fn remove_reaction(&mut self, emoji: &str) {
        if let Some(index) = self.reactions.iter().position(|r| r.emoji == emoji) {
            let reaction = &mut self.reactions[index];
            reaction.count -= 1;
            if reaction.count == 0 {
                self.reactions.remove(index);
            }
        }
    }
}
