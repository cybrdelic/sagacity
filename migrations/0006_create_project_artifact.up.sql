create table project_artifact (
    id integer primary key autoincrement,
    project_id integer not null,
    artifact_type text not null,
    content text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
