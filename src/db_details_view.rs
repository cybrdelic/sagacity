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
    // divide into 3 vertical chunks: db info, migrations, schema layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(7),
                Constraint::Min(0),
            ]
            .as_ref(),
        )
        .split(area);

    // db info block
    let version: i64 = if let Some(db) = &app.db {
        sqlx::query_scalar("select ifnull(max(version), 0) from _sqlx_migrations")
            .fetch_one(&db.pool)
            .await
            .unwrap_or(0)
    } else {
        0
    };
    let info_line = format!("db: {} | version: {}", app.db_path, version);
    let info_para = Paragraph::new(info_line)
        .block(Block::default().title("db info").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(info_para, chunks[0]);

    // migrations block - list applied migrations
    let mut mig_lines = vec![];
    if let Some(db) = &app.db {
        if let Ok(rows) = sqlx::query("select version, description, installed_on, success from _sqlx_migrations order by version")
            .fetch_all(&db.pool)
            .await {
            for row in rows {
                let ver: i64 = row.try_get("version").unwrap_or(0);
                let desc: String = row.try_get("description").unwrap_or_else(|_| "".to_string());
                let installed_on: String = row.try_get("installed_on").unwrap_or_else(|_| "".to_string());
                let success: bool = row.try_get("success").unwrap_or(false);
                mig_lines.push(Line::from(Span::raw(
                    format!("v{} - {} | {} | {}", ver, desc, installed_on, if success { "ok" } else { "fail" })
                )));
            }
        } else {
            mig_lines.push(Line::from(Span::raw("failed to query migrations")));
        }
    } else {
        mig_lines.push(Line::from(Span::raw("no db connection available")));
    }
    let mig_para = Paragraph::new(mig_lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().title("migrations").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(mig_para, chunks[1]);

    // schema layout block - list tables & their sql
    let mut schema_lines = vec![];
    if let Some(db) = &app.db {
        if let Ok(rows) = sqlx::query(
            "select name, sql from sqlite_master where type = 'table' and name not like 'sqlite_%'",
        )
        .fetch_all(&db.pool)
        .await
        {
            for row in rows {
                let tname: String = row.try_get("name").unwrap_or_default();
                let tsql: String = row.try_get("sql").unwrap_or_default();
                schema_lines.push(Line::from(Span::raw(format!("table: {}\n{}", tname, tsql))));
                schema_lines.push(Line::from(""));
            }
        }
    } else {
        schema_lines.push(Line::from("no db connection available"));
    }
    let schema_para = Paragraph::new(schema_lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title("schema layout")
                .borders(Borders::ALL),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(schema_para, chunks[2]);
}
