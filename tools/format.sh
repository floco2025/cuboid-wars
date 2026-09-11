#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

cargo fmt
prettier --write --print-width 80 --object-wrap collapse \
    '{client,common,config,server,tools}/**/*.json' \
    '!config/server/maps/*/layout.json' '!config/client/client_local.json'
ruff format --line-length 120 --no-cache .
