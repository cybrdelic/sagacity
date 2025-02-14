#!/bin/bash
# determine the copy command based on os type
if [[ "$OSTYPE" == "darwin"* ]]; then
    copy_cmd="pbcopy"
else
    if ! command -v xclip &>/dev/null; then
        echo "xclip not installed. install it rn and try again." >&2
        exit 1
    fi
    copy_cmd="xclip -selection clipboard"
fi

# create a temporary file for concatenated content
tmp_file=$(mktemp /tmp/all_files_content.XXXXXX.txt)

# collect all .rs and .sql files, excluding target, build, and .git directories
mapfile -t files < <(find . -type f \( -name "*.rs" -o -name "*.sql" \) ! -path "./target/*" ! -path "./build/*" ! -path "./.git/*")
total=${#files[@]}

if ((total == 0)); then
    echo "no rust or sql files found. aborting."
    exit 1
fi

echo -e "\n\033[1;32m✨ processing ${total} rust and sql files...\033[0m\n"

count=0
for file in "${files[@]}"; do
    count=$((count + 1))
    # print a fancy progress message with color and count
    printf "\033[1;34m[%-3d/%-3d] processing: %s\033[0m\n" "$count" "$total" "$file"
    echo -e "\n\n=== $file ===\n" >>"$tmp_file"
    cat "$file" >>"$tmp_file"
    echo -e "\n" >>"$tmp_file"
done

echo -e "\n\033[1;32m🚀 copying concatenated content to clipboard...\033[0m"
cat "$tmp_file" | $copy_cmd

rm "$tmp_file"

echo -e "\n\033[1;32m🎉 done. all file contents are now on your clipboard.\033[0m\n"
