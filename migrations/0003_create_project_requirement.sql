--! sqlx up
create table project_requirement (
    id integer primary key autoincrement,
    project_id integer not null,
    requirement text not null,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
--! sqlx down
drop table if exists project_requirement;
