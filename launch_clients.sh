#!/bin/zsh

# Launch multiple game clients tiled on the screen
# Usage: ./launch_clients.sh [num_clients] [lag_ms] [drop] [jitter]
# Default: 2 clients, 0ms lag, no drops, 5% jitter (drop and jitter are 0..1 fractions)

# Trap Ctrl-C and kill all child processes
trap 'echo "Killing all clients..."; kill 0; exit' INT TERM

NUM_CLIENTS=${1:-2}
LAG_MS=${2:-0}
DROP=${3:-0}
JITTER=${4:-0.05}

# Get screen dimensions
# Note: system_profiler reports both physical pixels and logical points
DISPLAY_INFO=$(system_profiler SPDisplaysDataType)
SCREEN_WIDTH_PHYSICAL=$(echo "$DISPLAY_INFO" | grep "Resolution:" | head -1 | awk '{print $2}')
SCREEN_HEIGHT_PHYSICAL=$(echo "$DISPLAY_INFO" | grep "Resolution:" | head -1 | awk '{print $4}')
SCREEN_WIDTH=$(echo "$DISPLAY_INFO" | grep "UI Looks like:" | head -1 | awk '{print $4}')
SCREEN_HEIGHT=$(echo "$DISPLAY_INFO" | grep "UI Looks like:" | head -1 | awk '{print $6}')

# At default scaling macOS omits the "UI Looks like:" line entirely. Retina
# panels then run at half the physical resolution, everything else at 1:1.
if [ -z "$SCREEN_WIDTH" ]; then
    if echo "$DISPLAY_INFO" | grep -q "Retina"; then
        SCREEN_WIDTH=$((SCREEN_WIDTH_PHYSICAL / 2))
        SCREEN_HEIGHT=$((SCREEN_HEIGHT_PHYSICAL / 2))
    else
        SCREEN_WIDTH=$SCREEN_WIDTH_PHYSICAL
        SCREEN_HEIGHT=$SCREEN_HEIGHT_PHYSICAL
    fi
fi

# Calculate the actual scaling factor (for display purposes)
SCALE_FACTOR=$(echo "scale=2; $SCREEN_WIDTH_PHYSICAL / $SCREEN_WIDTH" | bc)

echo "Physical resolution: ${SCREEN_WIDTH_PHYSICAL}x${SCREEN_HEIGHT_PHYSICAL}"
echo "Logical screen size: ${SCREEN_WIDTH}x${SCREEN_HEIGHT}"
echo "Scaling factor: ${SCALE_FACTOR}x"
echo "Launching $NUM_CLIENTS clients with ${LAG_MS}ms lag, drop ${DROP}, and jitter ${JITTER}..."

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
    # Y position: the top of the window frame, title bar included
    Y_LOGICAL=$((MENUBAR_HEIGHT + GAP + ROW * (WINDOW_HEIGHT + TITLEBAR_HEIGHT + GAP)))
    
    echo "Client $i: COL=$COL, ROW=$ROW, Logical=($X_LOGICAL, $Y_LOGICAL)"
    cargo run --bin client --release -- --window-x $X_LOGICAL --window-y $Y_LOGICAL --window-width $WINDOW_WIDTH --window-height $WINDOW_HEIGHT \
    --name "" --lag-ms $LAG_MS --drop $DROP --jitter $JITTER &
done

# Bring all client windows to the foreground
sleep 1
osascript -e 'tell application "System Events" to set frontmost of every process whose name contains "client" to true' 2>/dev/null

# Wait for all background jobs
echo "All clients launched!"
echo "Press Ctrl-C to kill all clients and exit."
wait
