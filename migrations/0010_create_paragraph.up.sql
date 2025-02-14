create table paragraph (
    id integer primary key autoincrement,
    chat_message_id integer,
    content text not null,
    created_at datetime default current_timestamp,
    foreign key (chat_message_id) references chat_message(id)
);
