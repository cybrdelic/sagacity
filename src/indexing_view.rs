use crate::{App, AppScreen, TreeNode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use std::{sync::Arc, time::SystemTime};
use tokio::sync::Mutex;

use crate::chat_view::summarize_file;

pub fn draw_indexing(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
        .split(size);

    let left_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(main_chunks[0]);

    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin_char = spinner_frames[app.spinner_idx % spinner_frames.len()];

    let elapsed = app
        .indexing_start_time
        .map(|start| start.elapsed().unwrap_or_default())
        .unwrap_or_default();

    let top_line = format!(
        "Status: {} {}  ({} files)\nElapsed: {}s",
        spin_char,
        if app.indexing_done {
            "Complete!"
        } else {
            "Indexing..."
        },
        app.indexing_count,
        elapsed.as_secs()
    );

    let top_para = Paragraph::new(top_line)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title(" Status ")
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .alignment(ratatui::layout::Alignment::Left);
    f.render_widget(top_para, left_split[0]);

    let mut lines = Vec::new();
    for (i, node) in app.tree.iter().enumerate() {
        let bar_len: usize = 20;
        let filled = (node.progress * bar_len as f32).round() as usize;
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(bar_len - filled));
        let line_str = format!(
            "{}. {} ({}%)  {} [{}]",
            i + 1,
            node.filename,
            (node.progress * 100.0) as u8,
            bar,
            node.status
        );
        lines.push(Line::from(Span::raw(line_str)));
    }
    let tree_para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Files ")
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(tree_para, left_split[1]);

    let total_files = app.tree.len() as f32;
    let total_progress: f32 = app.tree.iter().map(|node| node.progress).sum();
    let overall = if total_files > 0.0 {
        total_progress / total_files
    } else {
        0.0
    };
    let bar_len: usize = 30;
    let filled = (overall * bar_len as f32).round() as usize;
    let empty = bar_len.saturating_sub(filled);
    let final_bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
    let bot_line = format!("Overall progress: {:.1}%  {}", overall * 100.0, final_bar);
    let bot_para = Paragraph::new(bot_line)
        .block(
            Block::default()
                .title(" Progress ")
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(ratatui::layout::Alignment::Left);
    f.render_widget(bot_para, left_split[2]);

    let logs_block = Block::default()
        .title(" Logs ")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner_logs_area = logs_block.inner(main_chunks[1]);
    f.render_widget(logs_block, main_chunks[1]);

    let mut log_lines = Vec::new();
    for entry in &app.logs.entries {
        log_lines.push(Line::from(Span::raw(entry)));
    }
    let logs_para = Paragraph::new(log_lines)
        .wrap(Wrap { trim: true })
        .scroll((app.logs_scroll, 0));
    f.render_widget(logs_para, inner_logs_area);
}

pub async fn indexing_task(app: Arc<Mutex<App>>) {
    {
        let mut guard = app.lock().await;
        guard.logs.add("Starting codebase indexing...".to_string());
        guard.indexing_start_time = Some(SystemTime::now());
        guard.tree = vec![TreeNode::new("src/main.rs".into())];
    }

    let api_key = {
        let guard = app.lock().await;
        guard.chatbot.api_key.clone()
    };

    let main_rs = "src/main.rs";
    {
        let mut guard = app.lock().await;
        guard.logs.add("Indexing main.rs...".to_string());
    }

    if let Ok(content) = std::fs::read_to_string(main_rs) {
        if let Ok(summary) = summarize_file(&content, "rust", &api_key).await {
            let mut guard = app.lock().await;
            guard
                .chatbot
                .index
                .insert(main_rs.to_string(), (summary, "rust".to_string()));
            guard.logs.add("Indexed main.rs successfully".to_string());
            if let Some(node) = guard.tree.get_mut(0) {
                node.progress = 1.0;
                node.status = "done".into();
            }
            guard.indexing_count += 1;
        }
    }

    {
        let mut guard = app.lock().await;
        guard.indexing_done = true;
        guard.logs.add("Indexing complete!".to_string());
        guard.screen = AppScreen::Chat;
    }
}
