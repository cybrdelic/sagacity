--! sqlx up
create table file (
    id integer primary key autoincrement,
    directory_id integer,
    name text not null,
    content text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (directory_id) references directory(id)
);
--! sqlx down
drop table if exists file;
