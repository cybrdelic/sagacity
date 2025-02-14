create table project (
    id integer primary key autoincrement,
    name text not null,
    description text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp
);

