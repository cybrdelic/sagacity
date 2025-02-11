
# feature: add new 'start a new project' option in splash screen

this document outlines an expanded, llm-driven process for bootstrapping a new project. it covers context ingestion, artifact generation (with additional planning artifacts), and a phase for artifact modifications and clarifications.

---

## 1. context ingestion

in this phase, the system collects key data points from the user. beyond the basic questions, it probes further to build a richer context for artifact generation.

#### questions asked
1. **which stack/tools do we want to write this in?**
   *(default: rust/ratatui)*
2. **what do we want this project to do?**
3. **who is the target audience or primary user base?**
4. **what platforms or environments will this project run on?**
5. **are there any non-functional requirements (performance, security, scalability, compliance)?**
6. **what is the expected timeline or deadline for an mvp?**
7. **do you have any design or ux preferences?**

these additional questions help capture requirements, constraints, and stylistic preferences to better inform the planning artifacts.

---

## 2. artifact generation

based on the ingested context, the system auto-generates detailed planning artifacts. each artifact is produced using a dedicated detailed prompt tailored for llms.

### 2.1 tech stack and tools plan
**prompt:**
```
given the project context (tech stack: rust/ratatui, project goal: <insert user goal>, target platform: <insert platform>, non-functional requirements: <insert details>), generate a detailed tech stack and tools plan. include specifics such as:
- programming language version and rationale,
- ui toolkit details (ratatui) and why it's a good fit,
- build tools (e.g., cargo) and package management strategy,
- testing frameworks and development tools,
- additional libraries or plugins if applicable.
briefly justify each choice.
```

### 2.2 directory tree plan
**prompt:**
```
based on the project context and chosen tech stack, design a detailed directory tree structure. include all essential folders and files (for example, src, tests, docs, config, main.rs, lib.rs, Cargo.toml) and any extra directories (such as an 'api' folder if necessary). format your answer as an indented tree and provide a brief description of each folder/file's purpose.
```

### 2.3 mvp development plan
**prompt:**
```
using the project goal and timeline, produce a detailed mvp development plan. list out the core features and functionalities required for a minimal viable product, then break them into prioritized tasks and sub-tasks. include estimated phases or steps and note any dependencies or assumptions for each task.
```

### 2.4 requirements plan
**prompt:**
```
create a detailed requirements plan for the project. divide your output into two sections:
1. functional requirements: what the system should do,
2. non-functional requirements: performance, security, scalability, usability, etc.
for each requirement, provide clear, measurable criteria or acceptance conditions.
```

### 2.5 user flow plan
**prompt:**
```
develop a detailed user flow plan for the project. map out each step a user takes—from launching the application to achieving the primary goal—with decision points and alternative flows. structure your response in a clear, sequential format (using bullet points or an outlined flowchart) and include error handling or fallback scenarios where applicable.
```

### 2.6 additional artifacts (optional but recommended)
to further assist in planning and execution, consider generating these extra artifacts if the project context warrants:

#### 2.6.1 project timeline & milestones
**prompt:**
```
based on the mvp development plan and project deadline, generate a detailed project timeline. outline key milestones, deliverables, and dependencies. include estimates for task durations and identify critical paths.
```

#### 2.6.2 risk and mitigation plan
**prompt:**
```
assess potential risks associated with the project (technical, resource, market, etc.) and propose mitigation strategies for each risk. provide a prioritized list of risks along with possible contingency plans.
```

#### 2.6.3 resource allocation & cost estimation
**prompt:**
```
generate a plan outlining the required resources (human, technical, financial) for the project. include a high-level cost estimation and suggestions for resource allocation during the mvp phase.
```

#### 2.6.4 ui/ux design concept
**prompt:**
```
based on any provided design preferences, create a preliminary ui/ux design concept or wireframe plan. describe key interface elements, navigation flows, and visual style guidelines.
```

---

## 3. artifact modification and clarifications

in this final phase, the system presents the generated artifacts and invites the user to request modifications or clarifications. this ensures the artifacts align perfectly with the user's vision.

**system prompt:**
```
here are your generated artifacts:
- tech stack and tools plan
- directory tree plan
- mvp development plan
- requirements plan
- user flow plan
- (optional) project timeline & milestones
- (optional) risk and mitigation plan
- (optional) resource allocation & cost estimation
- (optional) ui/ux design concept

please list any modifications (e.g., "change directory tree: add an 'api' folder under src" or "update ui/ux design: prefer minimalist style with dark mode") or type 'approve' if all artifacts are correct.
```

**user response example:**
- "i want to update the directory tree to include an 'api' folder under src and add a 'config' folder."
- **system follow-up:** "updating directory tree plan: adding an 'api' folder and a 'config' folder. any other changes?"
- **user:** "that's good, approve."
- **system confirmation:** "artifacts approved. proceeding with project scaffolding."

---

## summary

this expanded process leverages detailed, llm-friendly prompts across multiple stages to fully capture project context and generate comprehensive planning artifacts. by adding extra context ingestion questions and optional artifacts—such as project timeline, risk analysis, resource planning, and ui/ux concepts—the system offers a robust framework for automating early-stage project planning, ensuring the final scaffold aligns with both functional and strategic goals.
