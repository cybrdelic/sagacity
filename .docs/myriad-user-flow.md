# User Flow

## Main Menu

The **Main Menu** is the central navigation hub, offering options to start new chats, continue existing conversations, or exit the application.

### 1. Start New Chat

Initiate a new conversation with different codebases or repositories.

#### a. Chat With CWD
- **Action:** Index the Current Working Directory (CWD).
- **Flow:**
  1. Select `Chat With CWD`.
  2. Trigger indexing screen.
  3. Upon completion, navigate to the chat screen.

#### b. Chat With GitHub Repo
- **Action:** Engage with a specific GitHub repository.
- **Flow:**
  1. Select `Chat With GitHub Repo`.
  2. Enter or choose a repository URL.
  3. Clone and index the repository.
  4. Navigate to the chat screen.

#### c. Chat With Multiple Codebases
- **Action:** Converse across multiple codebases simultaneously.
- **Flow:**
  1. Select `Chat With Multiple Codebases`.
  2. Choose multiple repositories/codebases.
  3. Clone and index each selected codebase.
  4. Merge indexes and navigate to the chat screen.

### 2. Continue Existing Conversation

Resume previous interactions seamlessly.

#### a. Conversation Select List
- **Features:**
  - **FZF-style** search and filtering.
  - Display conversation previews with title, project name, and last message.
- **Key Events:**
  1. **UP/DOWN Arrows:** Navigate conversations.
  2. **ENTER:**
     - Select conversation.
     - If related to CWD, check and update index if needed.
- **Flow:**
  1. Select a conversation from the list.
  2. Handle indexing if required.
  3. Open the chat screen.

### 3. Exit

Gracefully terminate the application with necessary confirmations.
