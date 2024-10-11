# Sagacity

Sagacity is a Rust-based command-line tool that leverages AI to assist developers in exploring and understanding their codebase through natural language queries. It automatically indexes the codebase and generates concise summaries for each file, allowing users to ask questions and receive relevant information based on the code context.

## Features

- **Codebase Indexing**: Sagacity scans and indexes the codebase, generating AI-generated summaries for each file, making it easier to navigate and understand the codebase.
- **Natural Language Queries**: Users can ask questions about the codebase in natural language, and Sagacity will provide relevant information based on the indexed files and summaries.
- **Contextual Responses**: Sagacity takes into account the conversation history and relevant code context to provide accurate and contextual responses.
- **Interactive CLI**: The project includes an interactive command-line interface (CLI) for seamless interaction and navigation through the codebase.
- **File Browsing**: Users can browse and view summaries for individual files, making it easier to understand the purpose and functionality of different components.
- **Response Management**: Responses from the AI can be easily copied to the clipboard or saved to files for future reference.
- **Conversation History**: Sagacity maintains a conversation history, allowing users to review previous queries and responses.

## Installation

You can install Sagacity using Cargo, the Rust package manager:

```
cargo install sagacity
```

## Setup

Sagacity requires an API key from [Anthropic](https://www.anthropic.com/) to leverage their AI models. You can obtain an API key by creating an account on the Anthropic website and following their instructions.

Once you have an API key, you need to add it to your `.zshrc` file (or the appropriate shell configuration file for your system). Add the following line, replacing `<YOUR_API_KEY>` with your actual API key:

```
export ANTHROPIC_API_KEY="<YOUR_API_KEY>"
```

Sagacity will automatically read the API key from this environment variable.

## Usage

After installation, you can run Sagacity from the command line:

```
sagacity
```

This will start the interactive CLI, where you can navigate through different options using the arrow keys and Enter.

### Main Menu

The main menu provides the following options:

- **Chat with AI**: Engage in a natural language conversation with the AI about your codebase.
- **Browse Index**: Browse and view summaries for individual files in the codebase.
- **Help**: Display the available commands and usage instructions.
- **Quit**: Exit the CLI.

### Chat Mode

In the chat mode, you can ask questions about your codebase in natural language. The AI will provide relevant information based on the indexed files and your queries. You can also use the following commands during the chat session:

- `/exit`: Return to the main menu.
- `/clear`: Clear the conversation history.
- `/help`: Display the chat commands help.
- `/save`: Save the current conversation.
- `/load`: Load a previously saved conversation.

### Response Management

After receiving a response from the AI, you can choose to copy the response to the clipboard or save it to a file for future reference.

## Contributing

Contributions to Sagacity are welcome! If you encounter any issues or have suggestions for improvements, please open an issue or submit a pull request on the project's GitHub repository.

## License

Sagacity is licensed under the [MIT License](LICENSE).
