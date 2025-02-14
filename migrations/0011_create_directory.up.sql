create table directory (
    id integer primary key autoincrement,
    name text not null,
    parent_directory_id integer,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (parent_directory_id) references directory(id)
);
