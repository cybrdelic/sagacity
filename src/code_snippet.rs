use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CodeSnippet {
    pub id: usize,
    pub content: String,
    pub language: String,
    pub line_start: usize,
    pub line_end: usize,
}

impl CodeSnippet {
    pub fn new(
        id: usize,
        content: String,
        language: String,
        line_start: usize,
        line_end: usize,
    ) -> Self {
        Self {
            id,
            content,
            language,
            line_start,
            line_end,
        }
    }

    pub fn detect_language(line: &str) -> String {
        let clean_line = line.trim().trim_start_matches("```");
        if clean_line.is_empty() {
            "text".to_string()
        } else {
            clean_line.to_string()
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnippetManager {
    pub snippets: Vec<CodeSnippet>,
    pub focused_snippet: Option<usize>,
    language_colors: HashMap<String, String>,
}

impl SnippetManager {
    pub fn new() -> Self {
        let mut language_colors = HashMap::new();
        language_colors.insert("rust".to_string(), "#dea584".to_string());
        language_colors.insert("python".to_string(), "#3572A5".to_string());
        language_colors.insert("javascript".to_string(), "#f1e05a".to_string());
        language_colors.insert("typescript".to_string(), "#2b7489".to_string());
        language_colors.insert("go".to_string(), "#00ADD8".to_string());

        Self {
            snippets: Vec::new(),
            focused_snippet: None,
            language_colors,
        }
    }

    pub fn add_snippet(&mut self, snippet: CodeSnippet) {
        self.snippets.push(snippet);
    }

    pub fn get_focused_snippet(&self) -> Option<&CodeSnippet> {
        self.focused_snippet.and_then(|idx| self.snippets.get(idx))
    }

    pub fn get_language_color(&self, language: &str) -> &str {
        self.language_colors
            .get(language)
            .map(|s| s.as_str())
            .unwrap_or("#FFFFFF")
    }

    pub fn handle_esc_number(&mut self, num: usize) -> Option<String> {
        if num > 0 && num <= self.snippets.len() {
            Some(self.snippets[num - 1].content.clone())
        } else {
            None
        }
    }

    pub fn focus_next(&mut self) {
        match self.focused_snippet {
            Some(current) if current + 1 < self.snippets.len() => {
                self.focused_snippet = Some(current + 1);
            }
            None if !self.snippets.is_empty() => {
                self.focused_snippet = Some(0);
            }
            _ => {}
        }
    }

    pub fn focus_previous(&mut self) {
        if let Some(current) = self.focused_snippet {
            if current > 0 {
                self.focused_snippet = Some(current - 1);
            }
        }
    }
}
