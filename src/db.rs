use log::LevelFilter;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    ConnectOptions, Pool, Row, Sqlite,
};
use std::path::Path;
use std::str::FromStr;

// this macro collects migrations from the ./migrations folder at compile time
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

        if std::env::var("DATABASE_URL").is_err() {
            std::env::set_var("DATABASE_URL", &connection_str);
        }

        let options = SqliteConnectOptions::from_str(&connection_str)?
            .create_if_missing(true)
            .log_statements(LevelFilter::Debug)
            .clone();

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        println!(
            "found {} migrations in ./migrations:",
            MIGRATOR.migrations.len()
        );
        for migration in MIGRATOR.migrations.iter() {
            println!(" - v{}: {}", migration.version, migration.description);
        }

        println!("running migrations...");
        MIGRATOR.run(&pool).await?;
        println!("migrations applied successfully.");

        // dump schema
        println!("dumping schema:");
        let rows = sqlx::query("select name, sql from sqlite_master")
            .fetch_all(&pool)
            .await?;
        for row in rows {
            let name: String = row.try_get("name")?;
            let sql: String = row.try_get("sql")?;
            println!("--- table: {} ---", name);
            println!("{}", sql);
        }

        // query current applied version for confirmation
        let applied_version: i64 =
            sqlx::query_scalar("select ifnull(max(version), 0) from _sqlx_migrations")
                .fetch_one(&pool)
                .await?;
        println!("current db version: {}", applied_version);

        Ok(Db { pool })
    }
}
