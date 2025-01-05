use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub struct SplashScreen {
    pub selected_idx: usize,
    pub menu_items: Vec<&'static str>,
}

impl SplashScreen {
    pub fn new() -> Self {
        Self {
            selected_idx: 0,
            menu_items: vec!["start chat", "quit"],
        }
    }

    pub fn draw(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let hsplit = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        let ascii_art = r#"
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
██ ▄▀▄ █ ██ █ ▄▄▀█▀ ██ ▄▄▀█ ▄▀████ ▄▄▀██▄█
██ █ █ █ ▀▀ █ ▀▀▄██ ██ ▀▀ █ █ █▀▀█ ▀▀ ██ ▄
██ ███ █▀▀▀▄█▄█▄▄█▀ ▀█▄██▄█▄▄██▄▄█▄██▄█▄▄▄
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
An Intelligent Software Development Copilot
    "#;

        let ascii_par = Paragraph::new(ascii_art)
            .alignment(Alignment::Center)
            .block(Block::default())
            .wrap(Wrap { trim: true });

        let ascii_vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(hsplit[0]);

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
            .alignment(Alignment::Center)
            .block(Block::default());

        let menu_line_count = self.menu_items.len() as u16;

        let menu_vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Length(menu_line_count),
                Constraint::Percentage(50),
            ])
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
                    "quit" => Some(SplashScreenAction::Quit),
                    "start chat" => Some(SplashScreenAction::StartChat),
                    _ => None,
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(SplashScreenAction::Quit),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum SplashScreenAction {
    Quit,
    StartChat,
}
