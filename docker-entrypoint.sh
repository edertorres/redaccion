#!/bin/sh
set -e

PORT="${PORT:-8767}"
BIND="${BIND_ADDR:-0.0.0.0}"

# Ensure storage directories exist
mkdir -p /app/documents /app/templates /app/fonts

# Seed default starter templates if the templates volume is empty
if [ -d "/app/templates_default" ] && [ -z "$(ls -A /app/templates 2>/dev/null)" ]; then
    echo "Inicializando plantillas base en /app/templates..."
    cp -r /app/templates_default/* /app/templates/ 2>/dev/null || true
fi

echo "============================================================"
echo "Iniciando Servidor Web Editorial DesignCraft"
echo "Escuchando en: http://${BIND}:${PORT}"
echo "============================================================"

exec /usr/local/bin/designcraft-cli serve \
    --port "$PORT" \
    --bind "$BIND" \
    --dist /app/dist/web \
    --templates /app/templates \
    --docs /app/documents \
    --fonts /app/fonts
