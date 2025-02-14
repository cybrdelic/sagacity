create table code_snippet (
    id integer primary key autoincrement,
    chat_message_id integer,
    content text not null,
    language text,
    line_start integer,
    line_end integer,
    created_at datetime default current_timestamp,
    foreign key (chat_message_id) references chat_message(id)
);
