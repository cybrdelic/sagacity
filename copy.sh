#!/bin/bash

# Check if we're on macOS or Linux
if [[ "$OSTYPE" == "darwin"* ]]; then
    COPY_CMD="pbcopy"
else
    COPY_CMD="xclip -selection clipboard"
fi

# Find all .rs files, excluding the target and build directories
find . -type f -name "*.rs" ! -path "./target/*" ! -path "./build/*" ! -path "./.git/*" | while read file; do
    # Append the file contents to a temporary file
    cat "$file" >> /tmp/all_rust_files_content.txt
    echo -e "\n\n" >> /tmp/all_rust_files_content.txt  # Add spacing between files
done

# Copy the contents of the temporary file to the clipboard
cat /tmp/all_rust_files_content.txt | $COPY_CMD

# Clean up the temporary file
rm /tmp/all_rust_files_content.txt

echo "All Rust file contents have been copied to the clipboard."
