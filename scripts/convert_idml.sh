#!/usr/bin/env bash
# DesignCraft — Conversión en lote y recursiva de IDML a DesignCraft
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_BIN="$(which python3 || which python)"

if [ -z "$PYTHON_BIN" ]; then
    echo "Error: Python 3 no está instalado en el sistema." >&2
    exit 1
fi

chmod +x "$SCRIPT_DIR/convert_idml.py" 2>/dev/null || true
exec "$PYTHON_BIN" "$SCRIPT_DIR/convert_idml.py" "$@"
