create table contextualization_index (
    id integer primary key autoincrement,
    file_id integer,
    index_data text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (file_id) references file(id)
);
