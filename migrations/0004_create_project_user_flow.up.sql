create table project_user_flow (
    id integer primary key autoincrement,
    project_id integer not null,
    flow_name text not null,
    description text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
