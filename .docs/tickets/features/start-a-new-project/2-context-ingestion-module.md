# feature: implement context ingestion module for new project setup

this ticket creates a dedicated module to collect detailed project context from the user. it ensures that all necessary data—ranging from tech stack and project goals to target audience and design preferences—is captured to drive artifact generation.

## objectives
- gather a rich set of project details using targeted questions.
- support multiline input and optional additional notes.
- store user responses in a dedicated context state for downstream use.

## tasks
1. **create context_ingestion.rs:**
   - implement functions to present the following questions:
     1. which stack/tools do we want to write this in? (default: rust/ratatui)
     2. what do we want this project to do?
     3. who is the target audience or primary user base?
     4. what platforms or environments will this project run on?
     5. are there any non-functional requirements (performance, security, scalability, compliance)?
     6. what is the expected timeline or deadline for an mvp?
     7. do you have any design or ux preferences?
   - allow for detailed, multiline responses and validations.

2. **create question_input.rs:**
   - manage the presentation of individual questions.
   - capture and validate user inputs, logging any required clarifications.

3. **integration:**
   - store the gathered responses in a new project setup state module (to be used later by artifact generation).

## expected outcome
- a robust context ingestion module that collects all essential project data.
- user responses are stored and ready for generating planning artifacts.
