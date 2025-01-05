use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
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
            menu_items: vec!["Start Chat", "Quit"],
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        // Define the ASCII art as a vector of lines
        let ascii_art = vec![
            "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄",
            "██ ▄▀▄ █ ██ █ ▄▄▀█▀ ██ ▄▄▀█ ▄▀████ ▄▄▀██▄█",
            "██ █ █ █ ▀▀ █ ▀▀▄██ ██ ▀▀ █ █ █▀▀█ ▀▀ ██ ▄",
            "██ ███ █▀▀▀▄█▄█▄▄█▀ ▀█▄██▄█▄▄██▄▄█▄██▄█▄▄▄",
            "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
            "An Intelligent Software Development Copilot",
        ];

        // Calculate the maximum width of the ASCII art
        let art_width = ascii_art.iter().map(|line| line.len()).max().unwrap_or(0) as u16;

        // Calculate the required width for the ASCII art section
        let ascii_section_width = art_width;

        // Split the area horizontally: left for ASCII art, right for menu
        let hsplit = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Length(ascii_section_width), // Fixed width for ASCII art
                    Constraint::Min(20),                     // Minimum width for menu
                ]
                .as_ref(),
            )
            .split(area);

        // Optional: Add borders for debugging
        // Uncomment the following lines to visualize layout sections during development
        /*
        let ascii_block = Block::default().borders(Borders::ALL).title("ASCII Art");
        f.render_widget(ascii_block, hsplit[0]);

        let menu_block = Block::default().borders(Borders::ALL).title("Menu");
        f.render_widget(menu_block, hsplit[1]);
        */

        // Split the left (ASCII art) vertically to center the art
        let ascii_vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(50), // Top padding to center vertically
                    Constraint::Length(ascii_art.len() as u16), // ASCII art height
                    Constraint::Percentage(50), // Bottom padding to center vertically
                ]
                .as_ref(),
            )
            .split(hsplit[0]);

        // Prepare the ASCII art as a single string without wrapping
        let ascii_art_str = ascii_art.join("\n");

        let ascii_par = Paragraph::new(ascii_art_str)
            .alignment(Alignment::Center)
            .block(Block::default())
            .wrap(Wrap { trim: false }); // Disable trimming to preserve ASCII art

        f.render_widget(ascii_par, ascii_vert[1]);

        // Prepare the menu items
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
            .wrap(Wrap { trim: true }); // Enable wrapping for menu items

        // Split the right (menu) vertically to center the menu
        let menu_vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(50), // Top padding to center vertically
                    Constraint::Length(self.menu_items.len() as u16), // Menu height
                    Constraint::Percentage(50), // Bottom padding to center vertically
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
