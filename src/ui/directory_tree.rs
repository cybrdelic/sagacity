// src/ui/directory_tree.rs

use ratatui::{
    layout::Rect,
    prelude::Backend,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use std::path::{Path, PathBuf};
use std::{collections::HashSet, fs};

pub struct DirectoryTree {
    pub root: PathBuf,
    pub expanded: HashSet<PathBuf>,
}

impl DirectoryTree {
    pub fn new(root_path: PathBuf) -> Self {
        DirectoryTree {
            root: root_path,
            expanded: HashSet::new(),
        }
    }

    pub fn toggle_expand(&mut self, path: &Path) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.to_path_buf());
        }
    }

    pub fn build_tree(&self, path: &Path) -> Vec<String> {
        let mut nodes = vec![format!("{}", path.display())];

        if self.expanded.contains(path) {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.filter_map(Result::ok) {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        nodes.push("".to_string()); // Placeholder for expanded node
                        nodes.extend(self.build_tree(&entry_path));
                    } else {
                        nodes.push(format!("{}", entry_path.display()));
                    }
                }
            }
        }

        nodes
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let tree_items = self.build_tree(&PathBuf::from("/"));
        let tree = List::new(
            tree_items
                .iter()
                .map(|i| ListItem::new(i.clone()))
                .collect::<Vec<_>>(),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Select Codebase"),
        )
        .highlight_symbol(">> ");
        f.render_widget(tree, area);
    }
}
