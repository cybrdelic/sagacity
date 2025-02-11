# feature: integrate project setup state management into main app

this ticket adds robust state management for the new project setup flow and integrates it into the existing application event loop.

## objectives
- create a dedicated state module (project_setup_state.rs) to store context responses, generated artifacts, and modification data.
- integrate this new state with the main app's state management.
- ensure seamless transitions between splash, project setup, and other app modes.

## tasks
1. **create project_setup_state.rs:**
   - define data structures for capturing project context, artifacts, and user modifications.
   - implement functions for state updates and retrieval.

2. **integration with main event loop:**
   - update main.rs and related modules to include and manage the new project setup state.
   - adjust state transitions and error handling for the new flow.

## expected outcome
- a robust, centralized state management system that supports the entire project setup process.
- smooth integration into the main app, enabling seamless mode transitions.
