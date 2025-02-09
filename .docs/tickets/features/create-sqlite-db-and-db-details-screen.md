
# integrating a sqlite db with sqlx and adding a "db details" screen

## overview
the goal is to have a local sqlite db to store data (like projects, convos, etc.) and a tui view that shows db path, schema version, and table definitions. we also want to see the queries that our code executes at runtime. here's a straightforward approach using sqlx. let's do it.

## steps

1. **add dependencies**
   in your `cargo.toml` (root level), toss in something like:
   ```toml
   [dependencies]
   sqlx = { version = "0.6", features = ["sqlite", "runtime-tokio-rustls", "macros"] }
   ```
   bc we want async usage of sqlite with macros for migrations if we want them.

2. **create db.rs**
   define a new file, e.g. `src/db.rs`:

   ```rust
   use sqlx::{
       sqlite::{SqliteConnectOptions, SqlitePoolOptions},
       Pool, Sqlite,
   };
   use std::str::FromStr;
   use tracing::Level;

   pub struct Db {
       pub pool: Pool<Sqlite>,
   }

   impl Db {
       pub async fn init(path: &str) -> anyhow::Result<Self> {
           // create connection opts w/ logging
           let connect_opts = SqliteConnectOptions::from_str(path)?
               .create_if_missing(true)
               .log_statements(Level::DEBUG);

           // build a pool
           let pool = SqlitePoolOptions::new()
               .max_connections(5)
               .connect_with(connect_opts)
               .await?;

           // if you want to run migrations automatically, do:
           // sqlx::migrate!().run(&pool).await?;

           // or define your own table creation, e.g.:
           // sqlx::query("create table if not exists projects (...);").execute(&pool).await?;

           ok(db { pool })
       }
   }
   ```
   note that `.log_statements(Level::DEBUG)` means you'll see the sql statements in your logs rn. if you want param logging, you'd do `.log_statements(level::trace)`, but you might need to configure tracing more deeply.

3. **inject db into app**
   in `src/indexing_view.rs` (or wherever your main `app` struct is declared), we can store a `db` handle. e.g.:

   ```rust
   // inside the app struct
   use crate::db::db; // wherever your db struct is
   ...

   #[derive(Debug)]
   pub struct app {
       ...
       pub db: option<db>, // store db handle
       pub db_path: string,
   }

   impl app {
       fn new() -> self {
           // set up everything
           self {
               ...
               db: none,
               db_path: "myriad_db.sqlite".to_string(),
           }
       }
   }
   ```
   next, in `main()`, after `app = app::new()`, do something like:
   ```rust
   {
       let mut guard = app.lock().await;
       let db_instance = db::init(&guard.db_path).await?;
       guard.db = some(db_instance);
       guard.logs.add("db initialized successfully");
   }
   ```

4. **create the db details screen**
   define a new file, `src/db_details_view.rs`, that queries `sqlite_master` for table info and shows it:

   ```rust
   use ratatui::{
       backend::backend,
       layout::{constraint, direction, layout, rect},
       style::{color, style},
       text::{line, span},
       widgets::{block, borders, paragraph, wrap},
       frame,
   };
   use crate::app;
   use crate::db::db;
   use sqlx::row::row;

   pub async fn draw_db_details<B: backend>(f: &mut frame<B>, app: &mut app) {
       let size = f.size();
       let vertical_split = layout::default()
           .direction(direction::vertical)
           .constraints([constraint::length(3), constraint::min(0)].as_ref())
           .split(size);

       let version = if let some(db_handle) = &app.db {
           // if we stored migrations in a table, we'd do something like:
           // let row: (i64,) = sqlx::query_as("select ifnull(max(version),0) from schema_migrations")
           //     .fetch_one(&db_handle.pool).await.unwrap_or((0,));
           // row.0
           0
       } else {
           0
       };

       let info_line = format!("db: {}, version: {}", app.db_path, version);
       let info_para = paragraph::new(info_line)
           .block(block::default().title("db info").borders(borders::all))
           .style(style::default().fg(color::white));
       f.render_widget(info_para, vertical_split[0]);

       let mut lines = vec![];

       if let some(db_handle) = &app.db {
           if let ok(rows) = sqlx::query!("select name, sql from sqlite_master where type='table'")
               .fetch_all(&db_handle.pool).await
           {
               for t in rows {
                   let tname = t.name.unwrap_or_default();
                   let tsql = t.sql.unwrap_or_default();
                   lines.push(line::from(span::raw(format!("table: {}\n{}", tname, tsql))));
                   lines.push(line::from(""));
               }
           }
       } else {
           lines.push(line::from("no db connection rn."));
       }

       let schema_para = paragraph::new(lines)
           .wrap(wrap { trim: true })
           .block(block::default().title("schema layout").borders(borders::all))
           .style(style::default().fg(color::white));
       f.render_widget(schema_para, vertical_split[1]);
   }
   ```

5. **wire up a route**
   your `appscreen` enum can get a variant:
   ```rust
   #[derive(debug, clone, copy, partialeq, eq)]
   pub enum appscreen {
       splash,
       indexing,
       chat,
       dbdetails,  // new
   }
   ```
   in `draw_ui(f, app)`, do:
   ```rust
   match app.screen {
       appscreen::splash => app.splash_screen.draw(f, f.area()),
       appscreen::indexing => draw_indexing(f, app),
       appscreen::chat => draw_chat(f, app),
       appscreen::dbdetails => {
           // call your async screen function in a blocking manner or do something cunning
           // but you prob want:
           let db_future = draw_db_details(f, app);
           let rt = tokio::runtime::handle::current();
           rt.block_on(db_future);
       }
   }
   ```
   or if you prefer a purely async approach, you can rearrange your code so that drawing is done in an async context. up to you.

6. **add a command to open db details**
   in `handle_command(app: &mut app)`, if you see `:db`, do:
   ```rust
   case "db" => {
       app.logs.add("opening db details screen rn");
       app.screen = appscreen::dbdetails;
   }
   ```
   then next time the ui draws, it’ll call `draw_db_details` and show your schema, version, etc.

## usage
- run the app, do `:db` in the chat, or however you trigger it. you should see the schema screen. also, if you run with `tracing_subscriber` or some such, you’ll see lines like `[sqlx::query]` in your logs bc `.log_statements(level::debug)`.
- if you want to store real stuff, add actual migrations or queries in `db::init` or do `sqlx::migrate!()`.

## done
that’s basically it. now you have a myopic local sqlite db with real-time statement logging, plus a new tui screen enumerating the schema. if you want param logs, you might do `.log_statements(level::trace)` or do manual logging whenever you run queries. idk, do what you like. but afaict, that’s the gist. cheers.
```
