
# integrating a sqlite db and adding a "db details" screen

## overview
the goal is to have a local sqlite db to store data (like projects, convos, etc.) and a tui view that shows db path, schema version, and table definitions. we also want to see the queries that our code executes at runtime. here's a straightforward approach using sqlx. let's do it. we also add a back button to the db details view, as well as displaying instructions on how to connect to sqlx with the db details in the db details view.

## done
that’s basically it. now you have a myopic local sqlite db with real-time statement logging, plus a new tui screen enumerating the schema. if you want param logs, you might do `.log_statements(level::trace)` or do manual logging whenever you run queries. idk, do what you like. but afaict, that’s the gist. cheers.
```
