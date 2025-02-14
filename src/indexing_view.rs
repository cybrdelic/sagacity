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
use ignore::WalkBuilder;

pub async fn indexing_task(app: Arc<Mutex<App>>) {
    {
        let mut guard = app.lock().await;
        guard.logs.add("Starting codebase indexing...".to_string());
        guard.indexing_start_time = Some(SystemTime::now());
        // Clear any previous tree nodes.
        guard.tree.clear();
    }

    let api_key = {
        let guard = app.lock().await;
        guard.chatbot.api_key.clone()
    };

    // Define the directories you want to index (e.g. "src" and "docs")
    let directories = vec!["src", "docs"];
    let mut files_to_index = Vec::new();

    for dir in directories {
        // Build a walker that respects .gitignore files and also filters out unwanted directories.
        let walker = WalkBuilder::new(dir)
            // Optionally ignore hidden files.
            .hidden(true)
            // Filter out paths that contain unwanted directory names.
            .filter_entry(|entry| {
                let path = entry.path();
                let path_str = path.to_string_lossy();
                // Ignore common directories
                if path_str.contains("/.git/")
                    || path_str.contains("/target/")
                    || path_str.contains("/node_modules/")
                {
                    return false;
                }
                true
            })
            .build();

        for result in walker {
            if let Ok(entry) = result {
                // Only process files.
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path_str = entry.path().to_string_lossy().to_string();
                    // Filter to index only specific file types.
                    if path_str.ends_with(".rs") || path_str.ends_with(".md") {
                        files_to_index.push(path_str);
                    }
                }
            }
        }
    }

    {
        let mut guard = app.lock().await;
        // Initialize the tree with a node for each file to be indexed.
        guard.tree = files_to_index
            .iter()
            .map(|f| TreeNode::new(f.clone()))
            .collect();
    }

    // Process each file.
    for file_path in files_to_index {
        {
            let mut guard = app.lock().await;
            guard.logs.add(format!("Indexing {}...", file_path));
        }
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            // Determine the language based on the file extension.
            let language = if file_path.ends_with(".rs") {
                "rust"
            } else if file_path.ends_with(".md") {
                "markdown"
            } else {
                "text"
            };

            if let Ok(summary) = summarize_file(&content, language, &api_key).await {
                let mut guard = app.lock().await;
                guard
                    .chatbot
                    .index
                    .insert(file_path.clone(), (summary, language.to_string()));
                guard
                    .logs
                    .add(format!("Indexed {} successfully", file_path));
                if let Some(node) = guard
                    .tree
                    .iter_mut()
                    .find(|node| node.filename == file_path)
                {
                    node.progress = 1.0;
                    node.status = "done".into();
                }
                guard.indexing_count += 1;
            } else {
                let mut guard = app.lock().await;
                guard.logs.add(format!("Failed to index {}", file_path));
            }
        } else {
            let mut guard = app.lock().await;
            guard.logs.add(format!("Could not read {}", file_path));
        }
    }

    {
        let mut guard = app.lock().await;
        guard.indexing_done = true;
        guard.logs.add("Indexing complete!".to_string());
        guard.screen = AppScreen::Chat;
    }
}
