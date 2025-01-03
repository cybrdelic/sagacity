// src/ui/directory_tree.rs

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub struct DirectoryTree {
    pub root_path: PathBuf,
    pub selected_path: PathBuf,      // Currently selected path
    pub expanded: HashSet<PathBuf>,  // Set of expanded directories
    pub visible_nodes: Vec<PathBuf>, // Flattened list of visible nodes
    pub selected_index: usize,       // Index in the visible_nodes vector
}

impl DirectoryTree {
    pub fn new(root_path: PathBuf) -> Self {
        let mut tree = DirectoryTree {
            root_path: root_path.clone(),
            selected_path: root_path.clone(),
            expanded: HashSet::new(),
            visible_nodes: Vec::new(),
            selected_index: 0,
        };
        tree.update_visible_nodes();
        tree
    }

    pub fn toggle_expand(&mut self, path: &Path) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.to_path_buf());
        }
        self.update_visible_nodes();
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.selected_path = self.visible_nodes[self.selected_index].clone();
        }
    }

    pub fn move_selection_down(&mut self) {
        if self.selected_index + 1 < self.visible_nodes.len() {
            self.selected_index += 1;
            self.selected_path = self.visible_nodes[self.selected_index].clone();
        }
    }

    pub fn move_selection_left(&mut self) {
        // Collapse the current directory
        let selected_path_clone = self.selected_path.clone();
        self.toggle_expand(&selected_path_clone);
    }

    pub fn move_selection_right(&mut self) {
        // Expand the current directory
        let selected_path_clone = self.selected_path.clone();
        self.toggle_expand(&selected_path_clone);
    }

    pub fn select_current(&self) -> Option<PathBuf> {
        Some(self.selected_path.clone())
    }

    pub fn update_visible_nodes(&mut self) {
        self.visible_nodes = Vec::new();
        let root_path_clone = self.root_path.clone();
        self.traverse(&root_path_clone, 0);
        // Ensure selected_index is within bounds
        if self.selected_index >= self.visible_nodes.len() && !self.visible_nodes.is_empty() {
            self.selected_index = self.visible_nodes.len() - 1;
            self.selected_path = self.visible_nodes[self.selected_index].clone();
        }
    }

    fn traverse(&mut self, path: &Path, _depth: usize) {
        self.visible_nodes.push(path.to_path_buf());

        if self.expanded.contains(path) {
            if let Ok(entries) = fs::read_dir(path) {
                let mut dirs: Vec<PathBuf> = entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|p| p.is_dir())
                    .collect();
                dirs.sort(); // Optional: sort directories alphabetically

                for dir in dirs {
                    self.traverse(&dir, 0);
                }
            }
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .visible_nodes
            .iter()
            .map(|path| {
                let display = path.display().to_string();
                let style = if path == &self.selected_path {
                    Style::default()
                        .bg(Color::LightMagenta)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(display).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Select Codebase"),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::LightMagenta)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        f.render_widget(list, area);
    }
}
