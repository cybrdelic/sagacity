# Sagacity: Enhanced Codebase Explorer

Sagacity is a powerful command-line tool that helps developers explore and understand their codebase with the assistance of the Claude AI language model from Anthropic.
It provides a user-friendly interface for searching, navigating, and interacting with code files, as well as an interactive chat mode for asking questions about the co
debase and receiving contextual responses from the AI.

## Features

### Codebase Indexing

Sagacity scans your codebase and indexes all Rust (`*.rs`), TOML (`*.toml`), and Markdown (`*.md`) files. For each file, it generates a concise summary using the Claude
AI language model, providing a high-level overview of the file's purpose and key functionalities.

### Search Mode

The search mode allows you to find relevant files based on keywords. When you enter a search query, Sagacity displays a list of matching files along with their summarie
s. You can then select a file to view its contents with syntax highlighting, making it easier to navigate and understand the codebase.

### Chat Mode

The chat mode is one of the core features of Sagacity. It enables you to engage in interactive conversations with the Claude AI language model, asking questions about y
our codebase. The AI responds with contextual information based on the relevant file contents. This mode is particularly useful for understanding complex code snippets,
finding specific functionality, or getting clarification on any aspect of your codebase.

### Conversation History

Sagacity keeps track of your conversation history with the AI. You can save and load conversations for future reference, allowing you to pick up where you left off or r
evisit previous discussions.

### Command Palette

The command palette provides a user-friendly way to access various actions within Sagacity. You can easily switch between search mode, chat mode, print the index, or qu
it the application.

### Autocompletion

When typing commands or file paths, Sagacity offers autocompletion functionality, making it easier to navigate and interact with the application.

## Prerequisites

- Rust installed (https://www.rust-lang.org/tools/install)
- An Anthropic API key (stored in your `~/.zshrc` file as `export ANTHROPIC_API_KEY="your_key_here"`)

## Installation

1. Clone the repository:

```bash
git clone https://github.com/your-repo/sagacity.git
```

2. Navigate to the project directory:

```bash
cd sagacity
```

3. Build the project:

```bash
cargo build --release
```

## Usage

Run the application:

```bash
./target/release/sagacity
```

Follow the on-screen instructions to explore your codebase, search for files, chat with the AI, and more.

When you start the application, you'll be prompted to choose an action from the command palette. Select the desired action (Search, Chat, Print Index, or Quit) and foll
ow the prompts.

### Search Mode

In the search mode, you can enter keywords to search for relevant files. Sagacity will display a list of matching files along with their summaries. You can then select
a file to view its contents with syntax highlighting.

### Chat Mode

The chat mode allows you to engage in interactive conversations with the Claude AI language model. When you enter a query, Sagacity will find relevant file contents and
provide them as context to the AI, enabling more accurate and contextual responses.

You can use various commands in the chat mode:

- `/exit`: End the chat session.
- `/clear`: Clear the conversation history.
- `/help`: Display a list of available commands and their descriptions.
- `/save`: Save the current conversation to a file (`conversation_history.json`).
- `/load`: Load a previously saved conversation from the `conversation_history.json` file.

### Other Commands

- **Print Index**: This command displays the full index of indexed files and their summaries.

## Contributing

Contributions are welcome! If you have any suggestions, bug reports, or feature requests, please open an issue or submit a pull request.

## License

This project is licensed under the [MIT License](LICENSE).
