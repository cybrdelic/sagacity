use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

#[derive(Debug)]
pub struct StatusIndicator {
    thinking: bool,
    status_text: String,
    spinner_idx: usize,
}

impl StatusIndicator {
    pub fn new() -> Self {
        Self {
            thinking: false,
            status_text: String::new(),
            spinner_idx: 0,
        }
    }

    pub fn set_thinking(&mut self, thinking: bool) {
        self.thinking = thinking;
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status_text = status.into();
    }

    pub fn update_spinner(&mut self) {
        self.spinner_idx = self.spinner_idx.wrapping_add(1);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let spinner_frames = ["◐", "◓", "◑", "◒"];
        let thinking_indicator = if self.thinking {
            spinner_frames[self.spinner_idx % spinner_frames.len()]
        } else {
            " "
        };

        let status = Line::from(vec![
            Span::styled(thinking_indicator, Style::default().fg(Color::Gray)),
            Span::raw(" "),
            Span::styled(
                if self.thinking { &self.status_text } else { "" },
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        frame.render_widget(
            Paragraph::new(status).alignment(ratatui::layout::Alignment::Left),
            Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            },
        );
    }
}
