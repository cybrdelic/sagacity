--! sqlx up
create table projects (
    id integer primary key autoincrement,
    name text not null,
    description text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp
);

create table conversations (
    id integer primary key autoincrement,
    project_id integer,
    title text not null,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references projects(id)
);

create table messages (
    id integer primary key autoincrement,
    conversation_id integer not null,
    from_user boolean not null,
    content text not null,
    created_at datetime default current_timestamp,
    foreign key (conversation_id) references conversations(id)
);

--! sqlx down
drop table if exists messages;
drop table if exists conversations;
drop table if exists projects;
