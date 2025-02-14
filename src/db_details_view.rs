use crate::App;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use sqlx::Row;

pub async fn draw_db_details(f: &mut Frame<'_>, app: &App) {
    // Outer container to prevent content from touching the screen edge.
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Database Details ",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::LightBlue));
    let outer_area = f.area();
    f.render_widget(outer_block.clone(), outer_area);
    let inner_area = outer_block.inner(outer_area);

    // Divide inner_area vertically into four sections:
    // 0: Back button (top)
    // 1: Details grid
    // 2: Tables grid
    // 3: Markdown instructions (scrollable)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),  // Back Button
                Constraint::Length(5),  // Details Grid
                Constraint::Length(10), // Tables Grid
                Constraint::Min(8),     // Markdown Instructions
            ]
            .as_ref(),
        )
        .split(inner_area);

    // --- Section 0: Back Button ---
    let back_button = Paragraph::new("Press Esc to go back")
        .style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Navigation"));
    f.render_widget(back_button, chunks[0]);

    // --- Section 1: Details Grid ---
    // Fetch database version.
    let version: i64 = if let Some(db) = &app.db {
        sqlx::query_scalar("select ifnull(max(version), 0) from _sqlx_migrations")
            .fetch_one(&db.pool)
            .await
            .unwrap_or(0)
    } else {
        0
    };
    // Fetch table count.
    let table_count: i64 = if let Some(db) = &app.db {
        sqlx::query_scalar(
            "select count(*) from sqlite_master where type='table' and name not like 'sqlite_%'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap_or(0)
    } else {
        0
    };
    // Fetch migrations count.
    let migration_count: i64 = if let Some(db) = &app.db {
        sqlx::query_scalar("select count(*) from _sqlx_migrations")
            .fetch_one(&db.pool)
            .await
            .unwrap_or(0)
    } else {
        0
    };

    // Split details grid into two columns.
    let details_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[1]);

    let details_left = Paragraph::new(format!(
        "URL: sqlite://{}\nVersion: {}",
        app.db_path, version
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Connection Details"),
    )
    .style(Style::default().fg(Color::White))
    .wrap(Wrap { trim: true });
    f.render_widget(details_left, details_chunks[0]);

    let details_right = Paragraph::new(format!(
        "Tables: {}\nMigrations: {}",
        table_count, migration_count
    ))
    .block(Block::default().borders(Borders::ALL).title("Metadata"))
    .style(Style::default().fg(Color::White))
    .wrap(Wrap { trim: true });
    f.render_widget(details_right, details_chunks[1]);

    // --- Section 2: Tables Grid ---
    let mut table_names = Vec::new();
    let mut table_schemas = Vec::new();
    if let Some(db) = &app.db {
        if let Ok(rows) = sqlx::query(
            "select name, sql from sqlite_master where type='table' and name not like 'sqlite_%'",
        )
        .fetch_all(&db.pool)
        .await
        {
            for row in rows {
                let tname: String = row.try_get("name").unwrap_or_default();
                let tsql: String = row.try_get("sql").unwrap_or_default();
                table_names.push(tname);
                table_schemas.push(tsql);
            }
        }
    }
    let table_grid_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(chunks[2]);

    let table_names_text = table_names.join("\n");
    let table_schemas_text = table_schemas.join("\n\n");
    let table_names_para = Paragraph::new(table_names_text)
        .block(Block::default().borders(Borders::ALL).title("Table Names"))
        .style(Style::default().fg(Color::LightYellow))
        .wrap(Wrap { trim: true });
    let table_schemas_para = Paragraph::new(table_schemas_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Table Schemas"),
        )
        .style(Style::default().fg(Color::LightCyan))
        .wrap(Wrap { trim: true });
    f.render_widget(table_names_para, table_grid_chunks[0]);
    f.render_widget(table_schemas_para, table_grid_chunks[1]);

    // --- Section 3: Markdown Rendered SQLx CLI Instructions (Scrollable) ---
    let markdown_instructions = r#"
# SQLx CLI Connection Instructions

To inspect and manage your database via SQLx CLI, follow these steps:

1. **Set the Environment Variable:**

   ```bash
   export DATABASE_URL="sqlite://<path-to-your-db>"
   ```

2. **Run the Migration Info Command:**

   ```bash
   sqlx migrate info
   ```

This command will display your migration history and current schema details.

For further help, please refer to the [SQLx CLI Documentation](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli).
"#;
    let markdown_para = Paragraph::new(markdown_instructions)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("SQLx CLI Instructions"),
        )
        .style(Style::default().fg(Color::Green))
        .wrap(Wrap { trim: true })
        .scroll((app.db_markdown_scroll, 0)); // 'db_markdown_scroll' should be maintained in your App struct.
    f.render_widget(markdown_para, chunks[3]);
}
