# Sagacity MVP Polish Requirements

## 1. User Experience

### Command History Navigation
**Current State**: Chat input has no command history navigation.  
**Improvement**: Implement arrow up/down for command history navigation.  
**Priority**: Medium  
**Approach**: Add command history storage in App struct and modify handle_chat_input to support navigating through previous commands with arrow keys.

### Copy Functionality
**Current State**: Copy to clipboard works but lacks visual feedback.  
**Improvement**: Add clear visual confirmation when text is copied.  
**Priority**: High  
**Approach**: Enhance status_indicator with temporary toast-style notification in a different color that fades after a few seconds.

### Input Field Improvements
**Current State**: Basic text input with limited editing capabilities.  
**Improvement**: Support cursor movement, selection, and better editing capabilities.  
**Priority**: Medium  
**Approach**: Implement a more robust text input widget with cursor positioning and text selection.

### Keyboard Shortcuts
**Current State**: Limited keyboard shortcuts (only Esc, Enter, etc.).  
**Improvement**: Add comprehensive keyboard shortcuts with help overlay.  
**Priority**: Medium  
**Approach**: Create a shortcut system and a help screen accessible via '?' key.

### Message Navigation
**Current State**: Rudimentary message navigation through up/down keys.  
**Improvement**: Cleaner, more intuitive navigation between messages and code blocks.  
**Priority**: High  
**Approach**: Add visual indicators for focus, implement tab navigation between code blocks.

## 2. Error Handling

### API Request Error Handling
**Current State**: Basic error handling for API calls, but error messages are not always user-friendly.  
**Improvement**: Improve API error handling with specific error messages and recovery options.  
**Priority**: High  
**Approach**: Create an error classification system to provide appropriate user-facing messages and recovery actions.

### File Access Error Handling
**Current State**: Minimal error handling for file operations during indexing.  
**Improvement**: Add robust error handling for file operations with clear user feedback.  
**Priority**: Medium  
**Approach**: Add structured error types and context-aware error messages for file operations.

### Config Error Handling
**Current State**: Limited validation of configuration settings.  
**Improvement**: Add comprehensive configuration validation with helpful error messages.  
**Priority**: Medium  
**Approach**: Implement a validation system for configuration with detailed error reporting.

### Token Limit Handling
**Current State**: No handling for API token limits or large context issues.  
**Improvement**: Add proactive handling of token limits with user warnings/options.  
**Priority**: High  
**Approach**: Implement token counting with claude-tokenizer and add smart context truncation strategies.

## 3. Performance

### File Indexing Optimization
**Current State**: Basic concurrent indexing with fixed concurrency limit.  
**Improvement**: Implement adaptive concurrency based on system resources.  
**Priority**: Medium  
**Approach**: Use sysinfo crate to detect available cores/memory and adjust concurrency dynamically.

### Response Caching
**Current State**: No caching of API responses.  
**Improvement**: Implement caching of API responses to reduce duplicate queries.  
**Priority**: Medium  
**Approach**: Add an LRU cache for API responses with configurable TTL.

### Rendering Performance
**Current State**: Inefficient re-rendering of the entire UI on every tick.  
**Improvement**: Implement selective rendering updates.  
**Priority**: Low  
**Approach**: Track UI dirty states and only redraw components that need updating.

### Indexing Resume Capability
**Current State**: Indexing starts from scratch each time.  
**Improvement**: Add ability to resume interrupted indexing operations.  
**Priority**: Low  
**Approach**: Store indexing progress in a persistence layer and implement resume logic.

## 4. Documentation

### User Documentation
**Current State**: Basic README with limited usage instructions.  
**Improvement**: Create comprehensive user documentation with examples.  
**Priority**: High  
**Approach**: Develop a complete user guide with command references, examples, and troubleshooting.

### API Documentation
**Current State**: Limited documentation in code.  
**Improvement**: Add comprehensive API documentation with examples.  
**Priority**: Medium  
**Approach**: Add rustdoc comments to all public interfaces and generate documentation.

### Code Comments
**Current State**: Inconsistent code comments.  
**Improvement**: Add consistent, high-quality code comments throughout the codebase.  
**Priority**: Medium  
**Approach**: Establish comment standards and systematically improve comments in all modules.

### Architecture Documentation
**Current State**: No formal architecture documentation.  
**Improvement**: Create architecture documentation explaining the system design.  
**Priority**: Medium  
**Approach**: Create architecture diagrams and documentation explaining component relationships.

## 5. Testing

### Unit Testing
**Current State**: Little to no unit testing.  
**Improvement**: Implement comprehensive unit test coverage.  
**Priority**: High  
**Approach**: Add unit tests for all core modules, starting with critical functionality.

### Integration Testing
**Current State**: No integration testing.  
**Improvement**: Add integration tests for key workflows.  
**Priority**: Medium  
**Approach**: Develop integration tests simulating user interactions with the system.

### API Mock Testing
**Current State**: No mock testing for API interactions.  
**Improvement**: Implement mock testing for API calls.  
**Priority**: High  
**Approach**: Use wiremock or similar to test API interaction patterns.

### Test Coverage Reporting
**Current State**: No test coverage metrics.  
**Improvement**: Add test coverage reporting to CI/CD.  
**Priority**: Low  
**Approach**: Integrate tools like tarpaulin for test coverage reporting.

## 6. Code Quality

### Error Propagation
**Current State**: Inconsistent error handling patterns.  
**Improvement**: Standardize error handling across the codebase.  
**Priority**: High  
**Approach**: Refactor to use anyhow/thiserror consistently throughout the code.

### Code Organization
**Current State**: Some modules with mixed responsibilities.  
**Improvement**: Refactor for cleaner separation of concerns.  
**Priority**: Medium  
**Approach**: Split larger modules into focused components with clear responsibilities.

### Configuration Management
**Current State**: Hard-coded values and environment variables.  
**Improvement**: Implement a structured configuration system.  
**Priority**: Medium  
**Approach**: Use config crate with layered configuration from files, environment, and CLI.

### Dependency Management
**Current State**: Many dependencies with potentially redundant functionality.  
**Improvement**: Review and optimize dependencies.  
**Priority**: Low  
**Approach**: Audit dependencies for overlap and unused features, optimize feature flags.

## 7. Feature Completeness

### Command History Persistence
**Current State**: No persistence of command history.  
**Improvement**: Implement persistent command history across sessions.  
**Priority**: Low  
**Approach**: Store command history in SQLite and load on startup.

### Chat History Export/Import
**Current State**: Conversations are not persisted.  
**Improvement**: Add export/import of chat histories.  
**Priority**: Medium  
**Approach**: Implement JSON export/import of conversation histories with metadata.

### Configurable Model Selection
**Current State**: Hardcoded Claude model.  
**Improvement**: Allow selection of different AI models.  
**Priority**: Medium  
**Approach**: Add model configuration option in settings and UI.

### Project Management
**Current State**: Database schema exists for projects, but no UI.  
**Improvement**: Implement project management UI.  
**Priority**: High  
**Approach**: Create screens for project creation, selection, and management.

### Context Management
**Current State**: Limited ability to manage which files are in context.  
**Improvement**: Add explicit context management UI.  
**Priority**: High  
**Approach**: Create a context management screen to select which files should be included in queries.