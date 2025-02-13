#!/bin/bash
# debug_sqlite_context.sh
# this script outputs context information to help troubleshoot your sqlite disk i/o error

echo "=== current working directory ==="
pwd
echo ""

echo "=== disk free space ==="
df -h .
echo ""

echo "=== directory listing (detailed) ==="
ls -la .
echo ""

echo "=== migration files in ./migrations ==="
if [ -d "./migrations" ]; then
    for f in ./migrations/*.sql; do
        echo "---- $f ----"
        cat "$f"
        echo ""
    done
else
    echo "migrations directory not found."
fi

echo "=== checking db file 'myriad_db.sqlite' ==="
if [ -f "myriad_db.sqlite" ]; then
    echo "DB file exists. Permissions:"
    ls -la myriad_db.sqlite
    echo ""
    echo "=== db schema (using sqlite3) ==="
    sqlite3 myriad_db.sqlite ".schema"
    echo ""
    echo "=== db tables ==="
    sqlite3 myriad_db.sqlite ".tables"
    echo ""
else
    echo "DB file 'myriad_db.sqlite' does not exist."
fi
