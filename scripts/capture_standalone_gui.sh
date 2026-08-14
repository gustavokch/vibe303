#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$REPO_ROOT/target/gui-qa"
OUTPUT_FILE="$OUTPUT_DIR/latest.png"
LOG_FILE="$OUTPUT_DIR/standalone.log"

mkdir -p "$OUTPUT_DIR"

echo "Building standalone binary..."
cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p plugin303 --bin standalone

echo "Launching standalone GUI..."
"$REPO_ROOT/target/debug/standalone" > "$LOG_FILE" 2>&1 &
APP_PID=$!

cleanup() {
    if kill -0 "$APP_PID" 2>/dev/null; then
        echo "Terminating standalone process (PID: $APP_PID)..."
        kill -TERM "$APP_PID" 2>/dev/null || true
        sleep 0.5
        kill -KILL "$APP_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "Waiting for standalone window..."
WIN_ID=""
for i in {1..30}; do
    WIN_ID=$(osascript -e '
        tell application "System Events"
            set proc to first process whose unix id is '"$APP_PID"'
            if exists proc then
                set winList to windows of proc
                if (count of winList) > 0 then
                    return id of first window of proc
                end if
            end if
        end tell
    ' 2>/dev/null || true)

    if [[ -n "$WIN_ID" && "$WIN_ID" != "missing value" ]]; then
        break
    fi
    sleep 0.3
done

if [[ -z "$WIN_ID" || "$WIN_ID" == "missing value" ]]; then
    WIN_ID=$(python3 -c "
import Quartz
wl = Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID)
for w in wl:
    if w.get('kCGWindowOwnerPID') == $APP_PID:
        print(w.get('kCGWindowNumber'))
        break
" 2>/dev/null || true)
fi

if [[ -z "$WIN_ID" || "$WIN_ID" == "missing value" ]]; then
    echo "ERROR: Standalone window did not appear within timeout." >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

echo "Capturing window ID: $WIN_ID to $OUTPUT_FILE..."
screencapture -l "$WIN_ID" -o "$OUTPUT_FILE"

if [[ ! -s "$OUTPUT_FILE" ]]; then
    echo "ERROR: Screenshot output file is empty or missing." >&2
    exit 1
fi

echo "Screenshot captured successfully: $OUTPUT_FILE"
