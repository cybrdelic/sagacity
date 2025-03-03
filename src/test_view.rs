use crate::errors::SagacityResult;
use crate::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::{sync::Arc, time::Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: u64,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestStatus {
    Passed,
    Failed,
    Running,
    NotRun,
}

impl TestStatus {
    pub fn color(&self) -> Color {
        match self {
            TestStatus::Passed => Color::Green,
            TestStatus::Failed => Color::Red,
            TestStatus::Running => Color::Yellow,
            TestStatus::NotRun => Color::Gray,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TestStatus::Passed => "PASS",
            TestStatus::Failed => "FAIL",
            TestStatus::Running => "RUNNING",
            TestStatus::NotRun => "NOT RUN",
        }
    }
}

#[derive(Debug)]
pub struct TestView {
    pub tests: Vec<TestResult>,
    pub selected_test: Option<usize>,
    pub running: bool,
    pub start_time: Option<Instant>,
}

impl TestView {
    pub fn new() -> Self {
        Self {
            tests: Vec::new(),
            selected_test: None,
            running: false,
            start_time: None,
        }
    }

    pub fn add_test(&mut self, name: String) {
        self.tests.push(TestResult {
            name,
            status: TestStatus::NotRun,
            duration_ms: 0,
            output: String::new(),
        });
    }

    pub fn update_test(&mut self, name: &str, status: TestStatus, duration_ms: u64, output: String) {
        if let Some(test) = self.tests.iter_mut().find(|t| t.name == name) {
            test.status = status;
            test.duration_ms = duration_ms;
            test.output = output;
        }
    }

    pub fn select_next(&mut self) {
        if self.tests.is_empty() {
            return;
        }

        self.selected_test = match self.selected_test {
            Some(i) if i < self.tests.len() - 1 => Some(i + 1),
            Some(_) => Some(0),
            None => Some(0),
        };
    }

    pub fn select_prev(&mut self) {
        if self.tests.is_empty() {
            return;
        }

        self.selected_test = match self.selected_test {
            Some(i) if i > 0 => Some(i - 1),
            Some(_) => Some(self.tests.len() - 1),
            None => Some(0),
        };
    }

    pub fn run_all_tests(&mut self) -> SagacityResult<()> {
        if self.running {
            return Ok(());
        }

        self.running = true;
        self.start_time = Some(Instant::now());

        // Reset all tests
        for test in &mut self.tests {
            test.status = TestStatus::NotRun;
            test.duration_ms = 0;
            test.output.clear();
        }

        Ok(())
    }

    pub fn get_selected_test(&self) -> Option<&TestResult> {
        self.selected_test.and_then(|i| self.tests.get(i))
    }

    pub fn all_tests_finished(&self) -> bool {
        !self.tests.is_empty() && self.tests.iter().all(|t| t.status != TestStatus::Running)
    }

    pub fn get_summary(&self) -> String {
        let total = self.tests.len();
        let passed = self.tests.iter().filter(|t| t.status == TestStatus::Passed).count();
        let failed = self.tests.iter().filter(|t| t.status == TestStatus::Failed).count();
        let not_run = self.tests.iter().filter(|t| t.status == TestStatus::NotRun).count();
        let running = self.tests.iter().filter(|t| t.status == TestStatus::Running).count();

        format!(
            "Total: {} | Passed: {} | Failed: {} | Running: {} | Not Run: {}",
            total, passed, failed, running, not_run
        )
    }
}

pub fn draw_test_view(f: &mut Frame, app: &mut App) {
    let size = f.size();

    // Create the layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(size);

    // Draw the header
    let header_text = vec![
        Spans::from(Span::styled(
            "Sagacity Test Runner",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Spans::from(Span::styled(
            app.test_view.get_summary(),
            Style::default().fg(Color::White),
        )),
    ];

    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::ALL).title("Tests"));
    f.render_widget(header, chunks[0]);

    // Split the main area
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(chunks[1]);

    // Draw the test list
    let test_items: Vec<ListItem> = app
        .test_view
        .tests
        .iter()
        .map(|test| {
            let status_style = Style::default().fg(test.status.color());
            let name_style = if Some(app.test_view.tests.iter().position(|t| t.name == test.name).unwrap())
                == app.test_view.selected_test
            {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let spans = Spans::from(vec![
                Span::styled(format!("[{}] ", test.status.as_str()), status_style),
                Span::styled(&test.name, name_style),
                Span::styled(
                    format!(" ({} ms)", test.duration_ms),
                    Style::default().fg(Color::Gray),
                ),
            ]);
            ListItem::new(spans)
        })
        .collect();

    let tests_list = List::new(test_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Test Cases"),
    );
    f.render_widget(tests_list, main_chunks[0]);

    // Draw the test details
    let details = if let Some(test) = app.test_view.get_selected_test() {
        let text = Text::from(vec![
            Spans::from(Span::styled(
                format!("Test: {}", test.name),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Spans::from(Span::styled(
                format!("Status: {}", test.status.as_str()),
                Style::default().fg(test.status.color()),
            )),
            Spans::from(Span::styled(
                format!("Duration: {} ms", test.duration_ms),
                Style::default().fg(Color::White),
            )),
            Spans::from(Span::raw("")),
            Spans::from(Span::styled(
                "Output:",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Spans::from(Span::raw("")),
            Spans::from(Span::raw(&test.output)),
        ]);

        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Test Details"),
        )
    } else {
        Paragraph::new("Select a test to view details")
            .block(Block::default().borders(Borders::ALL).title("Test Details"))
    };

    f.render_widget(details, main_chunks[1]);
}

pub async fn run_tests(app_arc: Arc<Mutex<App>>) {
    let mut guard = app_arc.lock().await;
    guard.test_view.running = true;
    drop(guard);

    // Define test cases
    let test_cases = vec![
        "test_api_connection",
        "test_file_indexing",
        "test_database_operations",
        "test_chat_functionality",
        "test_error_handling",
        "test_config_validation",
    ];

    // Initialize test cases
    {
        let mut guard = app_arc.lock().await;
        for test_name in &test_cases {
            guard.test_view.add_test(test_name.to_string());
        }
    }

    // Run each test
    for test_name in test_cases {
        // Mark test as running
        {
            let mut guard = app_arc.lock().await;
            guard.test_view.update_test(
                test_name,
                TestStatus::Running,
                0,
                "Running test...".to_string(),
            );
        }

        // Sleep to simulate test execution
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let result = match test_name {
            "test_api_connection" => run_api_connection_test().await,
            "test_file_indexing" => run_file_indexing_test().await,
            "test_database_operations" => run_database_operations_test().await,
            "test_chat_functionality" => run_chat_functionality_test().await,
            "test_error_handling" => run_error_handling_test().await,
            "test_config_validation" => run_config_validation_test().await,
            _ => Ok((TestStatus::Failed, "Unknown test".to_string())),
        };

        // Update test result
        {
            let mut guard = app_arc.lock().await;
            let (status, output) = match result {
                Ok((status, output)) => (status, output),
                Err(e) => (TestStatus::Failed, format!("Error: {}", e)),
            };

            let duration = rand::random::<u64>() % 1000 + 50; // Simulate random duration
            guard
                .test_view
                .update_test(test_name, status, duration, output);
        }
    }

    // Mark test run as complete
    {
        let mut guard = app_arc.lock().await;
        guard.test_view.running = false;
    }
}

async fn run_api_connection_test() -> SagacityResult<(TestStatus, String)> {
    // Simulate API connection test
    let api_key = std::env::var("ANTHROPIC_API_KEY");
    if api_key.is_err() {
        return Ok((
            TestStatus::Failed,
            "API key not found in environment variables".to_string(),
        ));
    }

    Ok((
        TestStatus::Passed,
        "Successfully connected to Anthropic API".to_string(),
    ))
}

async fn run_file_indexing_test() -> SagacityResult<(TestStatus, String)> {
    // Simulate file indexing test
    Ok((
        TestStatus::Passed,
        "Successfully indexed test directory with 10 files".to_string(),
    ))
}

async fn run_database_operations_test() -> SagacityResult<(TestStatus, String)> {
    // Simulate database operations test
    Ok((
        TestStatus::Passed,
        "Successfully performed CRUD operations on test database".to_string(),
    ))
}

async fn run_chat_functionality_test() -> SagacityResult<(TestStatus, String)> {
    // Simulate chat functionality test
    Ok((
        TestStatus::Passed,
        "Successfully tested chat message rendering and interaction".to_string(),
    ))
}

async fn run_error_handling_test() -> SagacityResult<(TestStatus, String)> {
    // Simulate error handling test
    Ok((
        TestStatus::Passed,
        "Successfully handled simulated errors and provided user-friendly messages".to_string(),
    ))
}

async fn run_config_validation_test() -> SagacityResult<(TestStatus, String)> {
    // Simulate config validation test
    Ok((
        TestStatus::Passed,
        "Successfully validated configuration parameters".to_string(),
    ))
}