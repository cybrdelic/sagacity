#!/bin/bash
set -e

# create migrations directory if it doesn't exist
mkdir -p migrations

# 0001_create_project.sql
cat <<'EOF' >migrations/0001_create_project.sql
--! sqlx up
create table project (
    id integer primary key autoincrement,
    name text not null,
    description text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp
);
--! sqlx down
drop table if exists project;
EOF

# 0002_create_project_overview.sql
cat <<'EOF' >migrations/0002_create_project_overview.sql
--! sqlx up
create table project_overview (
    id integer primary key autoincrement,
    project_id integer not null,
    overview text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
--! sqlx down
drop table if exists project_overview;
EOF

# 0003_create_project_requirement.sql
cat <<'EOF' >migrations/0003_create_project_requirement.sql
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
EOF

# 0004_create_project_user_flow.sql
cat <<'EOF' >migrations/0004_create_project_user_flow.sql
--! sqlx up
create table project_user_flow (
    id integer primary key autoincrement,
    project_id integer not null,
    flow_name text not null,
    description text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
--! sqlx down
drop table if exists project_user_flow;
EOF

# 0005_create_project_user_flow_step.sql
cat <<'EOF' >migrations/0005_create_project_user_flow_step.sql
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
EOF

# 0006_create_project_artifact.sql
cat <<'EOF' >migrations/0006_create_project_artifact.sql
--! sqlx up
create table project_artifact (
    id integer primary key autoincrement,
    project_id integer not null,
    artifact_type text not null,
    content text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
--! sqlx down
drop table if exists project_artifact;
EOF

# 0007_create_conversation.sql
cat <<'EOF' >migrations/0007_create_conversation.sql
--! sqlx up
create table conversation (
    id integer primary key autoincrement,
    project_id integer,
    title text not null,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (project_id) references project(id)
);
--! sqlx down
drop table if exists conversation;
EOF

# 0008_create_chat_message.sql
cat <<'EOF' >migrations/0008_create_chat_message.sql
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
EOF

# 0009_create_code_snippet.sql
cat <<'EOF' >migrations/0009_create_code_snippet.sql
--! sqlx up
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
--! sqlx down
drop table if exists code_snippet;
EOF

# 0010_create_paragraph.sql
cat <<'EOF' >migrations/0010_create_paragraph.sql
--! sqlx up
create table paragraph (
    id integer primary key autoincrement,
    chat_message_id integer,
    content text not null,
    created_at datetime default current_timestamp,
    foreign key (chat_message_id) references chat_message(id)
);
--! sqlx down
drop table if exists paragraph;
EOF

# 0011_create_directory_tree.sql
cat <<'EOF' >migrations/0011_create_directory_tree.sql
--! sqlx up
create table directory_tree (
    id integer primary key autoincrement,
    name text not null,
    root_directory_id integer,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (root_directory_id) references directory(id)
);
--! sqlx down
drop table if exists directory_tree;
EOF

# 0012_create_directory.sql
cat <<'EOF' >migrations/0012_create_directory.sql
--! sqlx up
create table directory (
    id integer primary key autoincrement,
    name text not null,
    parent_directory_id integer,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (parent_directory_id) references directory(id)
);
--! sqlx down
drop table if exists directory;
EOF

# 0013_create_file.sql
cat <<'EOF' >migrations/0013_create_file.sql
--! sqlx up
create table file (
    id integer primary key autoincrement,
    directory_id integer,
    name text not null,
    content text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (directory_id) references directory(id)
);
--! sqlx down
drop table if exists file;
EOF

# 0014_create_contextualization_index.sql
cat <<'EOF' >migrations/0014_create_contextualization_index.sql
--! sqlx up
create table contextualization_index (
    id integer primary key autoincrement,
    file_id integer,
    index_data text,
    created_at datetime default current_timestamp,
    updated_at datetime default current_timestamp,
    foreign key (file_id) references file(id)
);
--! sqlx down
drop table if exists contextualization_index;
EOF

echo "All migration files created in the migrations/ directory."
