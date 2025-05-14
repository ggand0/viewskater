#!/bin/bash
#
# viewskater_wrapper.sh
#
# Purpose:
# This wrapper script is intended for debugging macOS file association
# events (e.g., when opening a file with ViewSkater from Finder) and
# for capturing early startup logs or errors from the main application binary.
#
# Usage:
# Revise Info.plist's CFBundleExecutable to point to this script.
# e.g.,
# <key>CFBundleExecutable</key>
# <string>viewskater_wrapper.sh</string>
#
# How it works:
# - Sets up a log file at $HOME/Library/Logs/ViewSkater/open_events.log.
# - Logs invocation arguments and environment details.
# - Executes the main "viewskater" binary located in the same directory
#   as this script.
# - Redirects stderr of the main binary to the log file.
#
BASEDIR=$(dirname "$0")
LOG_FILE="$HOME/Library/Logs/ViewSkater/open_events.log"
mkdir -p "$(dirname "$LOG_FILE")"

# Log launch information with more detail
echo "$(date): ViewSkater wrapper launched with args: $@" >> "$LOG_FILE"
echo "$(date): Current directory: $(pwd)" >> "$LOG_FILE"
echo "$(date): Executable path: $BASEDIR/viewskater" >> "$LOG_FILE"

# Add direct console output
echo "ViewSkater wrapper starting..."
echo "Arguments: $@"
echo "Current directory: $(pwd)"
echo "Executable path: $BASEDIR/viewskater"

# Check if the executable exists
if [ ! -f "$BASEDIR/viewskater" ]; then
    echo "ERROR - Executable not found at $BASEDIR/viewskater"
    echo "$(date): ERROR - Executable not found at $BASEDIR/viewskater" >> "$LOG_FILE"
    exit 1
fi

# Execute with error logging
echo "Launching ViewSkater executable..."
exec "$BASEDIR/viewskater" "$@" 2>> "$LOG_FILE"