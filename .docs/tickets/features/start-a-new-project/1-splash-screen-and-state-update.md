# feature: update splash screen & app state for "start a new project" option

this ticket introduces a new option on the splash screen to kick off the project setup process. it also updates the application state to support a new project setup mode.

## objectives
- add a "start a new project" option to the splash screen menu.
- update the app state enum with a new variant (e.g., AppScreen::ProjectSetup) to represent the project setup flow.
- modify key event handlers so that selecting the new option transitions the app into the project setup mode.

## tasks
1. **splash_screen.rs:**
   - add "start a new project" to the existing menu items.
   - update the input handler to trigger the new mode when selected.

2. **main.rs & app state:**
   - add a new enum variant (e.g., AppScreen::ProjectSetup).
   - update the state transition logic to handle the new project setup flow.

3. **key event logic:**
   - adjust the handle_key_event function to dispatch to the new project setup mode when the appropriate key is pressed.

## expected outcome
- users see a new "start a new project" option on the splash screen.
- selecting this option transitions the app from the splash screen into a dedicated project setup mode.
