--! sqlx up
create table chat_message (
    id integer primary key autoincrement,
    conversation_id integer not null,
    from_user boolean not null,
    content text not null,
    created_at datetime default current_timestamp,
    foreign key (conversation_id) references conversation(id)
);
--! sqlx down
drop table if exists chat_message;
