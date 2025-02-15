use crate::chat_view::summarize_file;
use crate::models::TreeNode;
use crate::{chat_message::ChatMessage, App, AppScreen};
use futures::stream::{self, StreamExt};
use ignore::WalkBuilder;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use std::{sync::Arc, time::SystemTime};
use tokio::sync::Mutex; // For stream combinators

/// Draws the indexing UI with a status header, file tree panel, overall progress,
/// and a logs panel styled similarly to the chat view.
pub fn draw_indexing(f: &mut Frame, app: &mut App) {
    let size = f.area();
    // Split the screen horizontally: left for indexing status and files, right for logs.
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
        .split(size);

    // Left panel: vertical split for header, file tree, and overall progress.
    let left_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(5), // Status header
                Constraint::Min(10),   // File tree
                Constraint::Length(5), // Overall progress
            ]
            .as_ref(),
        )
        .split(main_chunks[0]);

    // ---------- Status Header ----------
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin_char = spinner_frames[app.spinner_idx % spinner_frames.len()];
    let elapsed = app
        .indexing_start_time
        .map(|start| start.elapsed().unwrap_or_default())
        .unwrap_or_default();
    let status_text = format!(
        "{} {}  | Files Indexed: {}  | Elapsed: {}s",
        spin_char,
        if app.indexing_done {
            "Complete!"
        } else {
            "Indexing..."
        },
        app.indexing_count,
        elapsed.as_secs()
    );
    let header_para = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .block(
            Block::default()
                .title(" Indexing Status ")
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(header_para, left_split[0]);

    // ---------- File Tree Panel ----------
    let mut file_lines = Vec::new();
    for (i, node) in app.tree.iter().enumerate() {
        let bar_len: usize = 20;
        let filled = (node.progress * bar_len as f32).round() as usize;
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(bar_len - filled));
        let status_color = match node.status.as_str() {
            "done" => Color::Green,
            "pending" => Color::Yellow,
            _ => Color::Red,
        };
        let line = format!(
            "{:>2}. {}  {} ({:>3}%)",
            i + 1,
            node.filename,
            bar,
            (node.progress * 100.0) as u8
        );
        file_lines.push(Line::from(Span::styled(
            line,
            Style::default().fg(status_color),
        )));
    }
    let file_tree = Paragraph::new(file_lines)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title(" Files to Index ")
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(file_tree, left_split[1]);

    // ---------- Overall Progress Panel ----------
    let total_files = app.tree.len() as f32;
    let total_progress: f32 = app.tree.iter().map(|node| node.progress).sum();
    let overall = if total_files > 0.0 {
        total_progress / total_files
    } else {
        0.0
    };
    let progress_bar_len: usize = 30;
    let filled = (overall * progress_bar_len as f32).round() as usize;
    let overall_bar = format!(
        "[{}{}]",
        "█".repeat(filled),
        "░".repeat(progress_bar_len - filled)
    );
    let overall_text = format!(
        "Overall Progress: {:>5.1}% {}",
        overall * 100.0,
        overall_bar
    );
    let overall_para = Paragraph::new(overall_text)
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .block(
            Block::default()
                .title(" Progress ")
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(overall_para, left_split[2]);

    // ---------- Logs Panel (Right Side) ----------
    let logs_block = Block::default()
        .title(" Logs (Press Esc to cancel indexing) ")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner_logs_area = logs_block.inner(main_chunks[1]);
    f.render_widget(logs_block, main_chunks[1]);

    let log_lines: Vec<Line> = app
        .logs
        .entries
        .iter()
        .map(|entry| Line::from(Span::raw(entry)))
        .collect();
    let logs_para = Paragraph::new(log_lines)
        .wrap(Wrap { trim: true })
        .scroll((app.logs_scroll, 0))
        .style(Style::default().fg(Color::Gray));
    f.render_widget(logs_para, inner_logs_area);
}

/// Asynchronously indexes files from specified directories.
/// Uses the ignore crate to skip over unwanted directories (like .git, target, and node_modules)
/// and only processes files with .rs or .md extensions.
/// This version reads files asynchronously and processes them concurrently,
/// updating each file’s progress incrementally.
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

    // Define the directories you want to index (e.g., "src" and "docs").
    let directories = vec!["src", "docs"];
    let mut files_to_index = Vec::new();

    for dir in directories {
        // Build a walker that respects .gitignore files and filters out unwanted directories.
        let walker = WalkBuilder::new(dir)
            .hidden(true)
            .filter_entry(|entry| {
                let path = entry.path();
                let path_str = path.to_string_lossy();
                // Ignore common directories.
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
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path_str = entry.path().to_string_lossy().to_string();
                    // Only index .rs and .md files.
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

    // Process files concurrently using a futures stream.
    let concurrency_limit = 16;
    // Clone the app for use inside the async closures.
    let app_clone = app.clone();
    let file_results = stream::iter(files_to_index)
        .map(|file_path| {
            let api_key = api_key.clone();
            let app_inner = app_clone.clone();
            async move {
                // Update progress: starting file read.
                update_progress(&app_inner, &file_path, 0.3, "reading").await;
                match tokio::fs::read_to_string(&file_path).await {
                    Ok(content) => {
                        // Update progress: file read complete.
                        update_progress(&app_inner, &file_path, 0.6, "read").await;

                        // <<< ADDED >>>
                        {
                            let mut guard = app_inner.lock().await;
                            guard.logs.add(format!(
                                "Sending summarize_file request to Claude for {}",
                                file_path
                            ));
                        }

                        let language = if file_path.ends_with(".rs") {
                            "rust"
                        } else if file_path.ends_with(".md") {
                            "markdown"
                        } else {
                            "text"
                        };

                        match summarize_file(&content, language, &api_key).await {
                            Ok(summary) => {
                                // <<< ADDED >>>
                                {
                                    let mut guard = app_inner.lock().await;
                                    guard.logs.add(format!(
                                        "Claude responded successfully for {} ({} bytes in summary)",
                                        file_path,
                                        summary.len()
                                    ));
                                }
                                // Update progress: summarization complete.
                                update_progress(&app_inner, &file_path, 1.0, "done").await;
                                Some((file_path, summary, language.to_string()))
                            }
                            Err(e) => {
                                update_progress(&app_inner, &file_path, 1.0, "failed").await;
                                // <<< ADDED >>>
                                {
                                    let mut guard = app_inner.lock().await;
                                    guard.logs.add(format!(
                                        "Claude summarization failed for {}: {}",
                                        file_path, e
                                    ));
                                }
                                None
                            }
                        }
                    }
                    Err(_) => None,
                }
            }
        })
        .buffer_unordered(concurrency_limit)
        .collect::<Vec<_>>()
        .await;

    {
        let mut guard = app.lock().await;
        for result in file_results {
            if let Some((file_path, summary, language)) = result {
                guard
                    .chatbot
                    .index
                    .insert(file_path.clone(), (summary, language));
                if let Some(node) = guard
                    .tree
                    .iter_mut()
                    .find(|node| node.filename == file_path)
                {
                    node.progress = 1.0;
                    node.status = "done".into();
                }
                guard.indexing_count += 1;
                guard
                    .logs
                    .add(format!("Indexed {} successfully", file_path));
            }
        }
        guard.indexing_done = true;
        guard.logs.add("Indexing complete!".to_string());
        guard.screen = AppScreen::Chat;
    }
}

/// Updates the progress and status for a specific file in the app's tree.
async fn update_progress(app: &Arc<Mutex<App>>, file_path: &str, progress: f32, status: &str) {
    let mut guard = app.lock().await;
    if let Some(node) = guard
        .tree
        .iter_mut()
        .find(|node| node.filename == file_path)
    {
        node.progress = progress;
        node.status = status.to_string();
    }
}
