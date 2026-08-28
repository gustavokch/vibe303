#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$REPO_ROOT/target/gui-qa"
LOG_FILE="$OUTPUT_DIR/standalone.log"

mkdir -p "$OUTPUT_DIR"

echo "Building standalone binary (release)..."
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" -p plugin303 --bin standalone

capture_single_state() {
    local state_name="$1"
    local output_file="$2"
    echo "Capturing state '$state_name' to '$output_file'..."

    VIBE303_QA_STATE="$state_name" "$REPO_ROOT/target/release/standalone" -b dummy > "$LOG_FILE" 2>&1 &
    local app_pid=$!
    if [[ "$state_name" != "default" ]]; then
        sleep 5
    else
        echo "First run: sleeping 20s..."
    	sleep 20
    fi

    local win_id=""
    for i in {1..30}; do
        win_id=$(osascript -e '
            tell application "System Events"
                set proc to first process whose unix id is '"$app_pid"'
                if exists proc then
                    set winList to windows of proc
                    if (count of winList) > 0 then
                        return id of first window of proc
                    end if
                end if
            end tell
        ' 2>/dev/null || true)

        if [[ -n "$win_id" && "$win_id" != "missing value" ]]; then
            break
        fi
        sleep 0.3
    done

    if [[ -z "$win_id" || "$win_id" == "missing value" ]]; then
        win_id=$(PROCESS_PID=$app_pid swift - << 'EOF' 2>/dev/null || true
import Cocoa
import CoreGraphics
import Foundation

let envPid = Int32(ProcessInfo.processInfo.environment["PROCESS_PID"] ?? "0") ?? 0
if let windowList = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] {
    for w in windowList {
        let pid = w[kCGWindowOwnerPID as String] as? pid_t ?? 0
        let id = w[kCGWindowNumber as String] as? CGWindowID ?? 0
        let bounds = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        let owner = w[kCGWindowOwnerName as String] as? String ?? ""
        let width = bounds["Width"] as? Int ?? 0
        let height = bounds["Height"] as? Int ?? 0
        if (pid == envPid || owner == "standalone") && width >= 1200 && height >= 700 {
            print(id)
            break
        }
    }
}
EOF
        )
    fi
    sleep 3
    if [[ -n "$win_id" && "$win_id" != "missing value" ]]; then
        osascript -e '
            tell application "System Events"
                set proc to first process whose unix id is '"$app_pid"'
                if exists proc then
                    set frontmost of proc to true
                end if
            end tell
        ' 2>/dev/null || true
        sleep 1
        screencapture -l "$win_id" -o "$output_file"
    else
        echo "Window ID not directly queryable; capturing screen to $output_file..."
        sleep 1
        screencapture -x "$output_file"
    fi

    if kill -0 "$app_pid" 2>/dev/null; then
        kill -TERM "$app_pid" 2>/dev/null || true
        sleep 0.3
        kill -KILL "$app_pid" 2>/dev/null || true
    fi

    if [[ ! -s "$output_file" ]]; then
        echo "ERROR: Screenshot output file '$output_file' is empty or missing." >&2
        exit 1
    fi
}

# Capture all required GUI states
capture_single_state "default" "$OUTPUT_DIR/latest.png"
capture_single_state "fun" "$OUTPUT_DIR/fun.png"
capture_single_state "running" "$OUTPUT_DIR/running.png"

echo "Extracting dynamic section crops, margins, and diff composites..."
python3 "$SCRIPT_DIR/crop_gui_sections.py" "$OUTPUT_DIR"

REQUIRED_CROPS=(
    "latest.png"
    "fun.png"
    "running.png"
    "crop_header.png"
    "crop_classic.png"
    "crop_souped.png"
    "crop_clipper_tweaks.png"
    "crop_sequencer.png"
    "crop_right_margin.png"
    "crop_bottom_margin.png"
    "crop_state_diff.png"
)

for crop_file in "${REQUIRED_CROPS[@]}"; do
    if [[ ! -s "$OUTPUT_DIR/$crop_file" ]]; then
        echo "ERROR: Required crop '$crop_file' missing or empty in $OUTPUT_DIR" >&2
        exit 1
    fi
done

echo "All GUI snapshots and crops captured successfully in $OUTPUT_DIR:"
ls -la "$OUTPUT_DIR"/*.png
