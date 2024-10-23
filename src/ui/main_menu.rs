use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::App;

pub fn draw_main_menu(f: &mut Frame<'_>, area: Rect, app: &App) {
    // Create a block for the menu background
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Main Menu")
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

    f.render_widget(block, area);

    // Define menu items with icons
    let items: Vec<ListItem> = app
        .menu_items
        .iter()
        .enumerate()
        .map(|(i, &item)| {
            if i == app.selected_menu_item {
                ListItem::new(item).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightMagenta)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(item).style(Style::default().fg(Color::White))
            }
        })
        .collect();

    let list = List::new(items)
        .block(Block::default())
        .highlight_style(
            Style::default()
                .bg(Color::LightMagenta)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    // Calculate the layout for the list
    let list_area = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Min(1)].as_ref())
        .split(area)[0];

    f.render_widget(list, list_area);
}
