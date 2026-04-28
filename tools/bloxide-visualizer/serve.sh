#!/usr/bin/env bash
set -euo pipefail

# Serve the Bloxide Visualizer in release mode on port 1420
# Usage: ./serve.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Serving Bloxide Visualizer..."
echo "  Mode:    release"
echo "  Port:    1420"
echo "  URL:     http://localhost:1420"
echo ""

dx serve --port 1420 --release --debug-symbols false
