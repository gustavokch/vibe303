# GUI Visual Iteration Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `iterating-gui-visually` workflow skill to automatically capture and validate `nih_plug_egui` standalone screenshots on macOS.

**Architecture:** A native macOS helper script `capture_standalone.sh` handles the AppleScript and `screencapture` orchestration, abstracting UI flakiness from the agent. The `SKILL.md` instructs the agent to read a design spec, run the capture script, ingest the image via `view_file`, analyze the differences, and ask the user for permission to execute a repair plan.

**Tech Stack:** macOS `screencapture`, `osascript`, Bash, Antigravity Customization System.

## Global Constraints
- Target directory: `/Users/gus/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/`
- Skill must follow Antigravity customization YAML frontmatter specs (`name`, `description`).

---

### Task 1: Scaffolding & Helper Script

**Files:**
- Create: `/Users/gus/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/scripts/capture_standalone.sh`

**Interfaces:**
- Produces: An executable bash script that outputs an image path.

- [ ] **Step 1: Create directories**
```bash
mkdir -p ~/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/scripts
```

- [ ] **Step 2: Write the helper script**
```bash
cat << 'EOF' > ~/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/scripts/capture_standalone.sh
#!/bin/bash
set -e

echo "Building standalone plugin..."
cargo build -p plugin303 --features standalone

echo "Launching standalone plugin in background..."
target/debug/plugin303 &
PID=$!

echo "Waiting for window to appear..."
# Wait up to 10 seconds for the window
for i in {1..20}; do
    # Suppress stderr to avoid spamming the log if the window isn't ready yet
    WINDOW_ID=$(osascript -e 'tell application "System Events" to get id of window 1 of (every process whose unix id is '"$PID"')' 2>/dev/null || echo "")
    if [ -n "$WINDOW_ID" ]; then
        break
    fi
    sleep 0.5
done

if [ -z "$WINDOW_ID" ]; then
    echo "Error: Could not find standalone window."
    kill $PID
    exit 1
fi

OUT_FILE="/tmp/vibe303_gui_capture.png"
echo "Capturing window $WINDOW_ID to $OUT_FILE..."
screencapture -l $WINDOW_ID $OUT_FILE

echo "Killing standalone process..."
kill $PID

echo "Screenshot saved to: $OUT_FILE"
EOF
chmod +x ~/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/scripts/capture_standalone.sh
```

- [ ] **Step 3: Test the script**
```bash
~/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/scripts/capture_standalone.sh
```
Expected: Compiles, launches, captures to `/tmp/vibe303_gui_capture.png`, and exits cleanly.

### Task 2: Write Minimal SKILL.md

**Files:**
- Create: `/Users/gus/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/SKILL.md`

**Interfaces:**
- Consumes: The `capture_standalone.sh` script created in Task 1.

- [ ] **Step 1: Write SKILL.md**
```bash
cat << 'EOF' > ~/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/SKILL.md
---
name: iterating-gui-visually
description: "Use when refining GUI layouts, when visual alignment with the design spec is required, or when testing nih_plug UI changes."
---

# Iterating GUI Visually

## Overview
A closed-loop workflow for visually validating and iterating on `nih_plug_egui` standalone interfaces. This ensures UI edits are tested against the actual rendered pixels and compared to the design spec before committing changes.

## When to Use
- You need to align the `plugin303` GUI with the design spec.
- You've been asked to "check what the UI looks like".
- You're iterating on layout, margins, colors, or visual hierarchy.

## The Workflow Loop

1. **Read Spec:** Read the design specification to understand the target aesthetics and layout.
2. **Capture:** Execute the capture script to take a screenshot of the current standalone GUI:
   `~/.gemini/config/plugins/superpowers/skills/iterating-gui-visually/scripts/capture_standalone.sh`
3. **Ingest Image:** Use your `view_file` tool to read the resulting `/tmp/vibe303_gui_capture.png` into your context.
4. **Analyze:** Visually compare the screenshot against the specification requirements. Look for misalignment, wrong colors, missing elements, and poor spacing.
5. **Plan & Pause:** Present your visual analysis and a proposed code-edit plan to the user. **STOP and wait for user approval.** Do NOT autonomously modify Rust code without review.
6. **Execute & Repeat:** Once approved, execute the code changes. Then repeat from Step 2 to verify the fix.

## Red Flags - STOP
- Guessing UI changes blindly without capturing a screenshot.
- Modifying UI code autonomously without waiting for the user to review your visual analysis and plan.
EOF
```

- [ ] **Step 2: Commit the plan to the repo**
```bash
git add docs/superpowers/plans/2026-08-14-gui-visual-iteration-skill.md
git commit -m "docs: add implementation plan for iterating-gui-visually skill"
```
