#!/bin/zsh

# Launch multiple game clients tiled on the screen
# Usage: ./launch_clients.sh [num_clients]
# Default: 2 clients

# Trap Ctrl-C and kill all child processes
trap 'echo "Killing all clients..."; kill 0; exit' INT TERM

NUM_CLIENTS=${1:-2}

# Get screen dimensions
# Note: system_profiler reports physical pixels, but macOS windowing uses logical points
# For Retina displays, divide by 2
SCREEN_RES=$(system_profiler SPDisplaysDataType | grep Resolution | head -1)
SCREEN_WIDTH_PHYSICAL=$(echo $SCREEN_RES | awk '{print $2}')
SCREEN_HEIGHT_PHYSICAL=$(echo $SCREEN_RES | awk '{print $4}')

# Assume 2x scaling for Retina displays (adjust if needed)
SCREEN_WIDTH=$((SCREEN_WIDTH_PHYSICAL / 2))
SCREEN_HEIGHT=$((SCREEN_HEIGHT_PHYSICAL / 2))

echo "Physical resolution: ${SCREEN_WIDTH_PHYSICAL}x${SCREEN_HEIGHT_PHYSICAL}"
echo "Logical screen size: ${SCREEN_WIDTH}x${SCREEN_HEIGHT}"
echo "Launching $NUM_CLIENTS clients..."

# Window dimensions
WINDOW_WIDTH=1000
WINDOW_HEIGHT=600
GAP=20
MENUBAR_HEIGHT=25  # macOS menu bar at top of screen
TITLEBAR_HEIGHT=30  # Window title bar height

# Calculate how many columns we can fit
COLS=2

# Launch clients
for i in $(seq 0 $((NUM_CLIENTS - 1))); do
    COL=$((i % COLS))
    ROW=$((i / COLS))
    
    # Position windows from right side of screen, side by side
    # Calculate in logical coordinates with consistent gap
    X_LOGICAL=$((SCREEN_WIDTH - (COL + 1) * WINDOW_WIDTH - GAP - COL * GAP))
    # Y position: The Y coordinate is for window content, but we want gap above the title bar
    # So we need: menu bar + gap + title bar for the first row
    if [ $ROW -eq 0 ]; then
        Y_LOGICAL=$((MENUBAR_HEIGHT + GAP + TITLEBAR_HEIGHT))
    else
        Y_LOGICAL=$((MENUBAR_HEIGHT + GAP + TITLEBAR_HEIGHT + ROW * (WINDOW_HEIGHT + TITLEBAR_HEIGHT + GAP)))
    fi
    
    # Convert to physical coordinates for window positioning (2x for Retina)
    X=$((X_LOGICAL * 2))
    Y=$((Y_LOGICAL * 2))
    
    echo "Client $i: COL=$COL, ROW=$ROW, Logical=($X_LOGICAL, $Y_LOGICAL), Physical=($X, $Y)"
    cargo run --bin client -- --window-x $X --window-y $Y --window-width $WINDOW_WIDTH --window-height $WINDOW_HEIGHT &
    
    # Small delay between launches
    sleep 0.5
done

echo "All clients launched!"
echo "Press Ctrl-C to kill all clients and exit."

# Bring all client windows to the foreground
#sleep 1
osascript -e 'tell application "System Events" to set frontmost of every process whose name contains "client" to true' 2>/dev/null

# Wait for all background jobs
wait
