use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub content: String,
    pub from_user: bool,
    pub chunks: Vec<MessageChunk>,
    pub focused_chunk: Option<usize>,
    pub highlight_mode: bool,
    language_colors: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct MessageChunk {
    pub id: usize,
    pub content: ChunkType,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChunkType {
    Code(CodeSnippet),
    Text(String),
    Steps(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
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

impl ChatMessage {
    pub fn new(content: String, from_user: bool) -> Self {
        let mut msg = Self {
            content: content.clone(),
            from_user,
            chunks: Vec::new(),
            focused_chunk: None,
            highlight_mode: false,
            language_colors: Self::default_language_colors(),
        };
        msg.parse_chunks();
        msg
    }

    fn default_language_colors() -> HashMap<String, String> {
        let mut colors = HashMap::new();
        colors.insert("rust".to_string(), "#dea584".to_string());
        colors.insert("python".to_string(), "#3572A5".to_string());
        colors.insert("javascript".to_string(), "#f1e05a".to_string());
        colors.insert("typescript".to_string(), "#2b7489".to_string());
        colors.insert("go".to_string(), "#00ADD8".to_string());
        colors
    }

    pub fn code_blocks(&self) -> impl Iterator<Item = &CodeSnippet> {
        self.chunks.iter().filter_map(|chunk| {
            if let ChunkType::Code(snippet) = &chunk.content {
                Some(snippet)
            } else {
                None
            }
        })
    }

    pub fn handle_esc_number(&self, number: usize) -> Option<String> {
        let code_blocks: Vec<_> = self.code_blocks().collect();
        if number > 0 && number <= code_blocks.len() {
            Some(code_blocks[number - 1].content.clone())
        } else {
            None
        }
    }

    fn parse_chunks(&mut self) {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut in_code_block = false;
        let mut in_steps = false;
        let mut current_steps = Vec::new();
        let mut line_number = 0;
        let mut chunk_start = 0;
        let mut current_language = String::new();

        for line in self.content.lines() {
            line_number += 1;
            if line.trim().starts_with("```") {
                if !current_chunk.is_empty() && !in_code_block {
                    chunks.push(MessageChunk {
                        id: chunks.len(),
                        content: ChunkType::Text(current_chunk.trim().to_string()),
                        start_line: chunk_start,
                        end_line: line_number - 1,
                    });
                    current_chunk.clear();
                }
                if in_code_block {
                    chunks.push(MessageChunk {
                        id: chunks.len(),
                        content: ChunkType::Code(CodeSnippet::new(
                            chunks.len(),
                            current_chunk.trim().to_string(),
                            current_language.clone(),
                            chunk_start,
                            line_number,
                        )),
                        start_line: chunk_start,
                        end_line: line_number,
                    });
                    current_chunk.clear();
                    current_language.clear();
                    in_code_block = false;
                } else {
                    current_language = CodeSnippet::detect_language(line);
                    in_code_block = true;
                    chunk_start = line_number;
                }
                continue;
            }
            if line.trim().starts_with("1.") && !in_code_block {
                if !current_chunk.is_empty() {
                    chunks.push(MessageChunk {
                        id: chunks.len(),
                        content: ChunkType::Text(current_chunk.trim().to_string()),
                        start_line: chunk_start,
                        end_line: line_number - 1,
                    });
                    current_chunk.clear();
                }
                in_steps = true;
                chunk_start = line_number;
                current_steps.push(line.trim()[2..].trim().to_string());
                continue;
            }
            if in_steps {
                if line.trim().starts_with(char::is_numeric) {
                    current_steps.push(line.trim()[2..].trim().to_string());
                } else {
                    chunks.push(MessageChunk {
                        id: chunks.len(),
                        content: ChunkType::Steps(current_steps.clone()),
                        start_line: chunk_start,
                        end_line: line_number - 1,
                    });
                    current_steps.clear();
                    in_steps = false;
                    chunk_start = line_number;
                    current_chunk.push_str(line);
                    current_chunk.push('\n');
                }
            } else if in_code_block {
                current_chunk.push_str(line);
                current_chunk.push('\n');
            } else {
                current_chunk.push_str(line);
                current_chunk.push('\n');
            }
        }
        if !current_chunk.is_empty() {
            chunks.push(MessageChunk {
                id: chunks.len(),
                content: ChunkType::Text(current_chunk.trim().to_string()),
                start_line: chunk_start,
                end_line: line_number,
            });
        }
        if in_steps && !current_steps.is_empty() {
            chunks.push(MessageChunk {
                id: chunks.len(),
                content: ChunkType::Steps(current_steps),
                start_line: chunk_start,
                end_line: line_number,
            });
        }
        self.chunks = chunks;
    }

    pub fn render(&self, area: Rect) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let style = self.get_base_style();
        self.render_header(&mut lines, style);
        for (idx, chunk) in self.chunks.iter().enumerate() {
            let is_focused = self.focused_chunk == Some(idx);
            self.render_chunk(&mut lines, chunk, is_focused, style, area);
        }
        self.render_footer(&mut lines, style);
        lines
    }

    fn render_header(&self, lines: &mut Vec<Line<'static>>, style: Style) {
        let indent = if self.from_user { "  " } else { "" };
        lines.push(Line::from(vec![
            Span::styled(indent.to_string(), style),
            Span::styled("┌─".to_string(), style),
        ]));
    }

    fn render_footer(&self, lines: &mut Vec<Line<'static>>, style: Style) {
        let indent = if self.from_user { "  " } else { "" };
        let mut footer_spans = vec![
            Span::styled(indent.to_string(), style),
            Span::styled("╰─".to_string(), style),
        ];
        
        // Show navigation hints when a message is focused
        if self.focused_chunk.is_some() {
            let hint_style = Style::default().fg(Color::DarkGray);
            footer_spans.extend(vec![
                Span::styled(" [", hint_style),
                Span::styled("←", Style::default().fg(Color::Yellow)),
                Span::styled("/", hint_style),
                Span::styled("→", Style::default().fg(Color::Yellow)),
                Span::styled(" to navigate chunks]", hint_style),
            ]);
        }
        
        // Show copy instructions in highlight mode
        if self.highlight_mode {
            let code_blocks_count = self.code_blocks().count();
            if code_blocks_count > 0 {
                let copy_style = Style::default().fg(Color::DarkGray);
                footer_spans.extend(vec![
                    Span::styled(" [ESC+", copy_style),
                    Span::styled(
                        format!("1-{}", code_blocks_count),
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("] to copy", copy_style),
                ]);
            }
        }
        
        lines.push(Line::from(footer_spans));
    }

    fn render_chunk(
        &self,
        lines: &mut Vec<Line<'static>>,
        chunk: &MessageChunk,
        is_focused: bool,
        base_style: Style,
        area: Rect,
    ) {
        let indent = if self.from_user { "  " } else { "" };
        
        // Enhanced focus indicator
        let (line_prefix, line_color) = if is_focused {
            ("│> ", Color::Yellow)  // Show a highlighted arrow for focused chunks
        } else {
            ("│ ", base_style.fg.unwrap_or(Color::Reset))
        };
        
        match &chunk.content {
            ChunkType::Code(snippet) => {
                let code_style = if is_focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    base_style
                };
                
                let header_style = if is_focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    base_style
                };
                
                // Code block header
                lines.push(Line::from(vec![
                    Span::styled(indent.to_string(), base_style),
                    Span::styled(line_prefix, Style::default().fg(line_color)),
                    Span::styled("```", header_style),
                    Span::styled(snippet.language.clone(), header_style.add_modifier(Modifier::UNDERLINED)),
                ]));
                
                // Code content
                for code_line in snippet.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled(indent.to_string(), base_style),
                        Span::styled(if is_focused {"│| "} else {"│ "}, Style::default().fg(line_color)),
                        Span::styled(code_line.to_string(), code_style),
                    ]));
                }
                
                // Code block footer
                lines.push(Line::from(vec![
                    Span::styled(indent.to_string(), base_style),
                    Span::styled(line_prefix, Style::default().fg(line_color)),
                    Span::styled("```", header_style),
                ]));
            }
            ChunkType::Text(text) => {
                let text_style = if is_focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    base_style
                };
                
                let wrap_width = (area.width as usize).saturating_sub(4);
                let wrapped = textwrap::wrap(text, wrap_width);
                let is_empty = wrapped.is_empty();
                
                // Add a focus marker at the top of focused text chunks
                if is_focused && !is_empty {
                    lines.push(Line::from(vec![
                        Span::styled(indent.to_string(), base_style),
                        Span::styled("╭─── Text ───", Style::default().fg(Color::Yellow)),
                    ]));
                }
                
                for line in wrapped {
                    lines.push(Line::from(vec![
                        Span::styled(indent.to_string(), base_style),
                        Span::styled(line_prefix, Style::default().fg(line_color)),
                        Span::styled(line.to_string(), text_style),
                    ]));
                }
                
                // Add a focus marker at the bottom of focused text chunks
                if is_focused && !is_empty {
                    lines.push(Line::from(vec![
                        Span::styled(indent.to_string(), base_style),
                        Span::styled("╰────────────", Style::default().fg(Color::Yellow)),
                    ]));
                }
            }
            ChunkType::Steps(steps) => {
                let step_style = if is_focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    base_style
                };
                
                // Add a focus marker at the top of focused step chunks
                if is_focused && !steps.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(indent.to_string(), base_style),
                        Span::styled("╭─── Steps ───", Style::default().fg(Color::Yellow)),
                    ]));
                }
                
                for (i, step) in steps.iter().enumerate() {
                    lines.push(Line::from(vec![
                        Span::styled(indent.to_string(), base_style),
                        Span::styled(line_prefix, Style::default().fg(line_color)),
                        Span::styled(format!("{}. ", i + 1), step_style),
                        Span::styled(step.clone(), step_style),
                    ]));
                }
                
                // Add a focus marker at the bottom of focused step chunks
                if is_focused && !steps.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(indent.to_string(), base_style),
                        Span::styled("╰─────────────", Style::default().fg(Color::Yellow)),
                    ]));
                }
            }
        }
    }

    fn get_base_style(&self) -> Style {
        Style::default().fg(if self.from_user {
            Color::Rgb(255, 223, 128)
        } else {
            Color::Rgb(144, 238, 144)
        })
    }

    pub fn focus_next(&mut self) {
        match self.focused_chunk {
            Some(current) if current + 1 < self.chunks.len() => {
                self.focused_chunk = Some(current + 1)
            }
            None if !self.chunks.is_empty() => self.focused_chunk = Some(0),
            _ => self.focused_chunk = None,
        }
    }

    pub fn focus_previous(&mut self) {
        match self.focused_chunk {
            Some(current) if current > 0 => self.focused_chunk = Some(current - 1),
            _ => self.focused_chunk = None,
        }
    }

    pub fn get_focused_content(&self) -> Option<String> {
        self.focused_chunk.and_then(|idx| {
            self.chunks.get(idx).map(|chunk| match &chunk.content {
                ChunkType::Code(snippet) => snippet.content.clone(),
                ChunkType::Text(text) => text.clone(),
                ChunkType::Steps(steps) => steps.join("\n"),
            })
        })
    }
}
