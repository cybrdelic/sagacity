# feature: implement artifact generation for new project setup

this ticket focuses on automatically generating detailed planning artifacts from the ingested context via llm-driven prompts. it covers core artifacts and optional ones for a comprehensive project blueprint.

## objectives
- generate the following artifacts using dedicated prompts:
  - tech stack and tools plan
  - directory tree plan
  - mvp development plan
  - requirements plan
  - user flow plan
- optionally, generate additional artifacts such as:
  - project timeline & milestones
  - risk and mitigation plan
  - resource allocation & cost estimation
  - ui/ux design concept

## tasks

1. **create artifact_generation_prompts.rs:**
   - define detailed llm-friendly prompts for each artifact as follows:

   ### tech stack and tools plan prompt
   ```
   given the project context (tech stack: rust/ratatui, project goal: <insert user goal>, target platform: <insert platform>, non-functional requirements: <insert details>), generate a detailed tech stack and tools plan. include specifics such as:
   - programming language version and rationale,
   - ui toolkit details (ratatui) and why it's a good fit,
   - build tools (e.g., cargo) and package management strategy,
   - testing frameworks and development tools,
   - additional libraries or plugins if applicable.
   briefly justify each choice.
   ```

   ### directory tree plan prompt
   ```
   based on the project context and chosen tech stack, design a detailed directory tree structure. include all essential folders and files (for example, src, tests, docs, config, main.rs, lib.rs, Cargo.toml) and any extra directories (such as an 'api' folder if necessary). format your answer as an indented tree and provide a brief description of each folder/file's purpose.
   ```

   ### mvp development plan prompt
   ```
   using the project goal and timeline, produce a detailed mvp development plan. list out the core features and functionalities required for a minimal viable product, then break them into prioritized tasks and sub-tasks. include estimated phases or steps and note any dependencies or assumptions for each task.
   ```

   ### requirements plan prompt
   ```
   create a detailed requirements plan for the project. divide your output into two sections:
   1. functional requirements: what the system should do,
   2. non-functional requirements: performance, security, scalability, usability, etc.
   for each requirement, provide clear, measurable criteria or acceptance conditions.
   ```

   ### user flow plan prompt
   ```
   develop a detailed user flow plan for the project. map out each step a user takes—from launching the application to achieving the primary goal—with decision points and alternative flows. structure your response in a clear, sequential format (using bullet points or an outlined flowchart) and include error handling or fallback scenarios where applicable.
   ```

   ### additional artifacts prompts (optional)

   #### project timeline & milestones prompt
   ```
   based on the mvp development plan and project deadline, generate a detailed project timeline. outline key milestones, deliverables, and dependencies. include estimates for task durations and identify critical paths.
   ```

   #### risk and mitigation plan prompt
   ```
   assess potential risks associated with the project (technical, resource, market, etc.) and propose mitigation strategies for each risk. provide a prioritized list of risks along with possible contingency plans.
   ```

   #### resource allocation & cost estimation prompt
   ```
   generate a plan outlining the required resources (human, technical, financial) for the project. include a high-level cost estimation and suggestions for resource allocation during the mvp phase.
   ```

   #### ui/ux design concept prompt
   ```
   based on any provided design preferences, create a preliminary ui/ux design concept or wireframe plan. describe key interface elements, navigation flows, and visual style guidelines.
   ```

2. **create artifact_generation.rs:**
   - implement functions to:
     - call the llm with each of the above prompts, dynamically inserting the user-provided project context.
     - process and store the responses as generated artifacts.
     - integrate error handling and logging (similar to the indexing_task) to manage api failures or timeouts.

## expected outcome
- a suite of detailed planning artifacts generated automatically from user context.
- each artifact is stored and ready for review and modification in subsequent phases.
