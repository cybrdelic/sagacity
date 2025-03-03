# Sagacity Development Guide

## Build Commands

```bash
# Build in debug mode
cargo build

# Run the application
cargo run

# Run with tests
cargo run -- --run-tests

# Build in release mode
cargo build --release

# Run linting
cargo clippy

# Run tests
cargo test

# Generate documentation
cargo doc --open
```

## Code Style Preferences

- Use 4 spaces for indentation
- Prefer `snake_case` for variable and function names
- Use `CamelCase` for types, traits, and enums
- Keep line length to around 100 characters
- Use descriptive variable names
- Add comments for complex logic
- Prefer early returns

## Project Structure

- `src/main.rs` - Application entry point
- `src/api.rs` - API interaction with Claude
- `src/config.rs` - Configuration management
- `src/errors.rs` - Error types and handling
- `src/db.rs` - Database operations
- `src/models.rs` - Data models
- `src/*_view.rs` - UI views for different screens

## Key Dependencies

- `ratatui` - Terminal UI framework
- `crossterm` - Terminal manipulation
- `tokio` - Async runtime
- `sqlx` - Database access
- `reqwest` - HTTP client
- `serde` - Serialization/deserialization
- `anyhow` & `thiserror` - Error handling

## Database Schema

The application uses SQLite for storage with tables for:
- Projects
- Files
- Conversations
- Chat messages
- Code snippets