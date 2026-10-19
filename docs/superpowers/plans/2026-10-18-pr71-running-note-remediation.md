# PR #71 remediation: running_note QA state

**Goal:** Close the two review findings on PR #71 (`feat(gui-qa): running_note state and a single QaState parse`).

**Architecture:** No behavior change. The Rust `QaState` enum and `qa_playhead_pin` are unchanged. The fixes are a Python regression guard and a doc comment.

**Tech Stack:** Python 3.12 (`unittest`, Pillow via `uv run --no-project --with pillow`), Rust (`cargo test -p plugin303 --release`).

**Spec reference:** `.agents/AGENTS.md` testing strategy; PR #71 review comment.

---

## Task 1: Cover `running_note` in the crop test

**Target files:** Modify `tests/test_crop_gui_sections.py`.

**Consumes:** `crop_gui_sections.process_all_states(qa_dir)` iterates `running_note` stem and writes `{stem}_crop_*.png` for each present `{stem}.png`.

**Step 1: Write the test change.** Add `running_note` to the mock stem list in `test_generate_all_crops`, and add `running_note_crop_header.png` (the crop naming pattern used for `latest_*` above) to the expected files.

**Step 2: Run to confirm pass.** `uv run --no-project --with pytest --with pillow --with numpy python -m pytest tests/test_crop_gui_sections.py -q`.

**Step 3: Mutation check (no commit).** Remove `"running_note"` from `scripts/crop_gui_sections.py` `process_all_states`, re-run; the test must fail. Restore the file.

**Step 4: Commit.** `git add tests/test_crop_gui_sections.py && git commit -m "test(gui-qa): cover running_note in crop section generation"`

## Task 2: Name every state-list copy in the QaState contract

**Target files:** Modify `plugin303/src/params.rs` (doc comment on `QaState`, lines ~414-418).

**Step 1: Edit the doc comment** to say the capture script, `scripts/crop_gui_sections.py`, and the `tools/gui_render` `STATES` table all hand-list state names and must stay in sync with `QaState::parse`.

**Step 2: Gate.** Run the Rust gate once after all edits (doc-only change; no test needed).

**Step 3: Commit.** `git add plugin303/src/params.rs && git commit -m "docs(qa): name every copy of the QA state list"`
