use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub enum SplashScreenAction {
    Quit,
    StartChat,
    DbDetails,
}

#[derive(Debug)]
pub struct SplashScreen {
    pub selected_idx: usize,
    pub menu_items: Vec<&'static str>,
}

impl SplashScreen {
    pub fn new() -> Self {
        Self {
            selected_idx: 0,
            menu_items: vec!["Start Chat", "DB Details", "Quit"],
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let ascii_art = vec![
            "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄",
            "██ ▄▀▄ █ ██ █ ▄▄▀█▀ ██ ▄▄▀█ ▄▀████ ▄▄▀██▄█",
            "██ █ █ █ ▀▀ █ ▀▀▄██ ██ ▀▀ █ █ █▀▀█ ▀▀ ██ ▄",
            "██ ███ █▀▀▀▄█▄█▄▄█▀ ▀█▄██▄█▄▄██▄▄█▄██▄█▄▄▄",
            "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
            "an intelligent software development copilot",
        ];

        let art_width = ascii_art.iter().map(|line| line.len()).max().unwrap_or(0) as u16;
        let ascii_section_width = art_width;

        let hsplit = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(ascii_section_width), Constraint::Min(20)].as_ref())
            .split(area);

        let ascii_vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(50),
                    Constraint::Length(ascii_art.len() as u16),
                    Constraint::Percentage(50),
                ]
                .as_ref(),
            )
            .split(hsplit[0]);

        let ascii_art_str = ascii_art.join("\n");
        let ascii_par = Paragraph::new(ascii_art_str)
            .alignment(Alignment::Center)
            .block(Block::default())
            .wrap(Wrap { trim: false });
        f.render_widget(ascii_par, ascii_vert[1]);

        let mut menu_lines = Vec::new();
        for (i, item) in self.menu_items.iter().enumerate() {
            let selected = i == self.selected_idx;
            let style = if selected {
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            menu_lines.push(Line::from(Span::styled(
                format!("{} {}", if selected { "▶" } else { " " }, item),
                style,
            )));
        }
        let menu_par = Paragraph::new(menu_lines)
            .alignment(Alignment::Left)
            .block(Block::default())
            .wrap(Wrap { trim: true });
        let menu_vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(50),
                    Constraint::Length(self.menu_items.len() as u16),
                    Constraint::Percentage(50),
                ]
                .as_ref(),
            )
            .split(hsplit[1]);
        f.render_widget(menu_par, menu_vert[1]);
    }

    pub fn handle_input(&mut self, key: crossterm::event::KeyEvent) -> Option<SplashScreenAction> {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Down) => {
                self.selected_idx = (self.selected_idx + 1) % self.menu_items.len();
                None
            }
            (KeyModifiers::NONE, KeyCode::Up) => {
                if self.selected_idx == 0 {
                    self.selected_idx = self.menu_items.len() - 1;
                } else {
                    self.selected_idx -= 1;
                }
                None
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let selected = self.menu_items[self.selected_idx];
                match selected {
                    "Quit" => Some(SplashScreenAction::Quit),
                    "Start Chat" => Some(SplashScreenAction::StartChat),
                    "DB Details" => Some(SplashScreenAction::DbDetails),
                    _ => None,
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(SplashScreenAction::Quit),
            _ => None,
        }
    }
}
