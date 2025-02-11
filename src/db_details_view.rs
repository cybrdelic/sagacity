use crate::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use sqlx::Row;

pub async fn draw_db_details(f: &mut Frame<'_>, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(area);

    // note: sqlx uses a default migrations table "sqlx_migrations"
    let version: i64 = if let Some(db) = &app.db {
        match sqlx::query_scalar("select ifnull(max(version), 0) as version from sqlx_migrations")
            .fetch_one(&db.pool)
            .await
        {
            Ok(ver) => ver,
            Err(_) => 0,
        }
    } else {
        0
    };

    let info_line = format!("db: {} | version: {}", app.db_path, version);
    let info_para = Paragraph::new(info_line)
        .block(Block::default().title("db info").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(info_para, chunks[0]);

    let mut lines = vec![];
    if let Some(db) = &app.db {
        if let Ok(rows) = sqlx::query("select name, sql from sqlite_master where type = 'table'")
            .fetch_all(&db.pool)
            .await
        {
            for row in rows {
                let tname: String = row.try_get("name").unwrap_or_default();
                let tsql: String = row.try_get("sql").unwrap_or_default();
                lines.push(Line::from(Span::raw(format!("table: {}\n{}", tname, tsql))));
                lines.push(Line::from(""));
            }
        }
    } else {
        lines.push(Line::from("no db connection available"));
    }

    let schema_para = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title("schema layout")
                .borders(Borders::ALL),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(schema_para, chunks[1]);
}
