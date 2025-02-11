use log::LevelFilter;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    ConnectOptions, Pool, Sqlite,
};
use std::str::FromStr;

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

        // run migrations from ./migrations folder
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Db { pool })
    }
}
