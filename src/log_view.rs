use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub struct LogView {
    entries: Vec<String>,
    scroll_offset: u16,
}

impl LogView {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scroll_offset: 0,
        }
    }

    pub fn add_entry(&mut self, entry: String) {
        self.entries.push(entry);
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    pub fn scroll_down(&mut self, max_scroll: u16) {
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let log_lines: Vec<Line> = self
            .entries
            .iter()
            .map(|entry| {
                Line::from(vec![
                    Span::styled("• ", Style::default().fg(Color::DarkGray)),
                    Span::raw(entry),
                ])
            })
            .collect();

        let total_lines = log_lines.len() as u16;
        let available_height = area.height;
        let max_scroll = total_lines.saturating_sub(available_height);
        let log_scroll = self.scroll_offset.min(max_scroll);

        let logs_para = Paragraph::new(log_lines)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true });

        frame.render_widget(logs_para.scroll((log_scroll, 0)), area);
    }

    pub fn calculate_max_scroll(&self, area_height: u16) -> u16 {
        self.entries.len().saturating_sub(area_height as usize) as u16
    }
}
