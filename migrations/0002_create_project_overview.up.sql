create table project_overview (
    id integer primary key autoincrement,
    project_id integer not null,
    overview text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
