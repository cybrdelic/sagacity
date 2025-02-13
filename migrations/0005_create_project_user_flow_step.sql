--! sqlx up
create table project_user_flow_step (
    id integer primary key autoincrement,
    user_flow_id integer not null,
    step_number integer not null,
    description text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (user_flow_id) references project_user_flow(id)
);
--! sqlx down
drop table if exists project_user_flow_step;
