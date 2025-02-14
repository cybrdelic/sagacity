// src/db.rs
use log::info;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    ConnectOptions, Pool, Row, Sqlite,
};
use std::str::FromStr;

// This macro collects migrations from the ./migrations folder at compile time.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug)]
pub struct Db {
    pub pool: Pool<Sqlite>,
}

impl Db {
    pub async fn init(path: &str) -> anyhow::Result<Self> {
        let connection_str = if path.starts_with("sqlite://") {
            path.to_string()
        } else {
            format!("sqlite://{}", path)
        };

        // Ensure DATABASE_URL is set for sqlx.
        if std::env::var("DATABASE_URL").is_err() {
            std::env::set_var("DATABASE_URL", &connection_str);
        }

        let options = SqliteConnectOptions::from_str(&connection_str)?
            .create_if_missing(true)
            .log_statements(log::LevelFilter::Debug)
            .clone();

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        // Log migration details instead of printing to stdout.
        info!(
            "Found {} migrations in ./migrations",
            MIGRATOR.migrations.len()
        );
        for migration in MIGRATOR.migrations.iter() {
            info!(
                "Migration v{}: {}",
                migration.version, migration.description
            );
        }

        info!("Running migrations...");
        MIGRATOR.run(&pool).await?;
        info!("Migrations applied successfully.");

        // Log the database schema for reference.
        info!("Dumping schema:");
        let rows = sqlx::query("SELECT name, sql FROM sqlite_master")
            .fetch_all(&pool)
            .await?;
        for row in rows {
            let name: String = row.try_get("name")?;
            let sql: String = row.try_get("sql")?;
            info!("--- table: {} ---", name);
            info!("{}", sql);
        }

        // Query the current applied version.
        let applied_version: i64 =
            sqlx::query_scalar("SELECT ifnull(max(version), 0) FROM _sqlx_migrations")
                .fetch_one(&pool)
                .await?;
        info!("Current DB version: {}", applied_version);

        Ok(Db { pool })
    }
}
