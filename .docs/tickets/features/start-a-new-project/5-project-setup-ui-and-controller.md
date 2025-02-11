# feature: implement project setup ui and controller for new project flow

this ticket builds the overall user interface and controller logic for the new project setup process. it orchestrates the entire flow from context ingestion through artifact generation and modification.

## objectives
- create a dedicated project setup screen (project_setup_screen.rs) to manage the user interaction.
- implement a controller (project_setup_controller.rs) to coordinate between context ingestion, artifact generation, and modifications.
- handle new key events and input routing specific to the project setup mode.

## tasks
1. **create project_setup_screen.rs:**
   - design the ui for the project setup mode.
   - implement display logic for context questions, generated artifacts, and modification prompts.

2. **create project_setup_controller.rs:**
   - manage the overall flow and state transitions between the different phases.
   - handle new key events specific to project setup (refer to project_setup_events.rs).

3. **update key events:**
   - create project_setup_events.rs to define and process new key commands relevant to project setup.

## expected outcome
- a cohesive project setup interface that guides the user through all phases.
- smooth state transitions and responsive key event handling specific to the new mode.
