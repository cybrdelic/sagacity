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
    
    pub fn clear_status(&mut self) {
        self.status_text.clear();
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
        
        // Cycling colors for the spinner
        let spinner_colors = [
            Color::Rgb(255, 165, 0),   // Orange
            Color::Rgb(255, 105, 180), // Hot Pink
            Color::Rgb(50, 205, 50),   // Lime Green
            Color::Rgb(30, 144, 255),  // Dodger Blue
        ];
        let spinner_color = if self.thinking {
            spinner_colors[self.spinner_idx % spinner_colors.len()]
        } else {
            Color::DarkGray
        };
        
        // Always show status text if available, otherwise show thinking status
        let status_text = if !self.status_text.is_empty() {
            &self.status_text
        } else if self.thinking {
            "Processing..."
        } else {
            ""
        };
        
        // Choose the appropriate color for the status text
        let status_color = if self.thinking {
            Color::Rgb(150, 150, 150)  // Lighter gray for thinking
        } else if !self.status_text.is_empty() {
            Color::Rgb(135, 206, 250)  // Light Sky Blue for navigation
        } else {
            Color::DarkGray
        };
        
        let status = Line::from(vec![
            Span::styled(thinking_indicator, Style::default().fg(spinner_color)),
            Span::raw(" "),
            Span::styled(status_text, Style::default().fg(status_color)),
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
