use dotenv::dotenv;
use std::env;

fn main() {
    dotenv().ok();
    // If DATABASE_URL is not set, use our default SQLite connection URI.
    let db_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://myriad_db.sqlite".to_string());
    println!("cargo:rustc-env=DATABASE_URL={}", db_url);
}
