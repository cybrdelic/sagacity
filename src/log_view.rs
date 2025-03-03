use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub struct LogView {
    pub entries: Vec<String>,
    pub scroll_offset: u16,
}

impl LogView {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scroll_offset: 0,
        }
    }

    pub fn add(&mut self, entry: String) {
        self.entries.push(entry);
        if self.entries.len() > 200 {
            self.entries.remove(0);
        }
    }
}
