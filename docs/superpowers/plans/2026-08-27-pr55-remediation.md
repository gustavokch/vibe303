# PR #55 Review & Remediation Plan

## Goal
Remediate code review nits identified in PR #55 diff review:
1. Capture background `app_pid=$!` immediately in `scripts/capture_standalone_gui.sh`.
2. Remove dead `count=2` assignment in `scripts/capture_standalone_gui.sh`.
3. Verify test suite and push updates to `fix/gui-visual-remediation-r6`.

## Architecture & Tech Stack
- Bash scripting (`scripts/capture_standalone_gui.sh`)
- Cargo / Rust test suite (`plugin303`)

## Tasks

### Task 1: Clean up PID capture and unused assignment in capture script
- **Modify:** `scripts/capture_standalone_gui.sh`
- **Step 1:** Move `local app_pid=$!` to line directly following background invocation of standalone.
- **Step 2:** Remove unused `count=2` inside the window polling loop.
- **Step 3:** Run `bash -n scripts/capture_standalone_gui.sh` to verify syntax.
- **Step 4:** Commit fix: `git commit -m "fix(ci): capture standalone app_pid immediately and clean loop"`

### Task 2: Verify full test suite and push to PR branch
- **Step 1:** Run `cargo test --workspace`.
- **Step 2:** Push to `origin fix/gui-visual-remediation-r6`.
