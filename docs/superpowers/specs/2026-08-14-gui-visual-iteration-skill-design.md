# GUI Visual Iteration Skill Design

**Date:** 2026-08-14
**Status:** Pending Review
**Topic:** Workflow skill for visually iterating on the `nih_plug_egui` frontend

## 1. Overview & Goals
This project introduces a new agent workflow skill to create a closed, visual iteration loop for the `vibe303` plugin GUI. Agents will use this skill to launch the standalone GUI, automatically capture a screenshot, analyze the UI against the design spec, and present an implementation plan to resolve visual deviations.

## 2. Skill Architecture
The skill will be implemented as an Antigravity custom skill within the global config plugin directory.

**Path:** `~/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/`

**SDO (Skill Discovery Optimization):**
*   **Name:** `iterating-gui-visually`
*   **Description:** "Use when refining GUI layouts, when visual alignment with the design spec is required, or when testing nih_plug UI changes."

## 3. Helper Script (`capture_standalone.sh`)
To isolate the agent from OS-level window management flakiness, a bash utility script will handle capturing screenshots on macOS.

*   **Location:** `skills/iterating-gui-visually/scripts/capture_standalone.sh`
*   **Process:**
    1.  Compiles the project using `cargo build -p plugin303 --features standalone`.
    2.  Launches the built binary in the background.
    3.  Executes an inline AppleScript to wait for the application window to appear and retrieves its macOS Window ID.
    4.  Runs `screencapture -l <WINDOW_ID> /tmp/vibe303_gui_capture.png` to take an isolated window screenshot.
    5.  Terminates the background process.
    6.  Prints the absolute path to the captured image.

## 4. Agent Workflow (`SKILL.md`)
When invoked, the skill enforces the following structured procedure for the agent:

1.  **Read Spec:** Ingest the GUI design spec (e.g., `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`).
2.  **Capture:** Execute `scripts/capture_standalone.sh`.
3.  **Ingest Image:** Use the `view_file` tool to load the output `.png` screenshot.
4.  **Analyze:** Compare the screenshot against the specification, focusing on layout, missing elements, color tokens, and overall visual polish.
5.  **Plan & Pause:** Present the analysis and a proposed plan for code edits to the user. The agent MUST NOT modify code until the plan is approved.

## 5. Development Plan
As dictated by `writing-skills`, this skill will be developed using TDD:
1.  **RED:** Run a subagent pressure scenario *without* the skill to observe baseline behavior.
2.  **GREEN:** Implement the helper script and the minimal `SKILL.md` to guide the agent through the correct capture and analysis loop.
3.  **REFACTOR:** Test the scenario with the skill active, adjusting instructions to close any observed loopholes (e.g., making sure the agent pauses for review instead of autonomously modifying code).
