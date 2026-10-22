# ==============================================================================
# DesignCraft Web Editorial — Multi-stage Production Dockerfile
# Diseñado para despliegue optimizado en Coolify / Docker
# ==============================================================================

# ------------------------------------------------------------------------------
# Fase 1: Compilación (Rust + WebAssembly Trunk + CLI Server)
# ------------------------------------------------------------------------------
FROM rust:bookworm AS builder

ENV DEBIAN_FRONTEND=noninteractive

# Dependencias para compilación nativa
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ca-certificates \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Instalar target WebAssembly
RUN rustup target add wasm32-unknown-unknown

# Instalar binario oficial de Trunk (evita compilación lenta de 5-10m)
ARG TRUNK_VERSION=0.21.14
RUN curl -fsSL https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xzf - -C /usr/local/bin && chmod +x /usr/local/bin/trunk

WORKDIR /app

# Copiar manifiestos y código fuente
COPY Cargo.toml Cargo.lock rustfmt.toml clippy.toml ./
COPY crates/ ./crates/
COPY apps/ ./apps/
COPY assets/ ./assets/
COPY xtask/ ./xtask/

# 1. Compilar el frontend web en WebAssembly
WORKDIR /app/apps/designcraft-web
RUN trunk build --release

# 2. Compilar el micro-servidor backend en modo release
WORKDIR /app
RUN cargo build --release -p designcraft-cli

# ------------------------------------------------------------------------------
# Fase 2: Imagen de Ejecución Ligera y Segura
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

ENV DEBIAN_FRONTEND=noninteractive \
    RUST_LOG=info \
    PORT=8767 \
    BIND_ADDR=0.0.0.0

# Paquetes de runtime mínimos y utilidades de salud
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    fontconfig \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copiar binario compilado del servidor
COPY --from=builder /app/target/release/designcraft-cli /usr/local/bin/designcraft-cli

# Copiar bundle estático WebAssembly
COPY --from=builder /app/dist/web /app/dist/web

# Copiar fuentes y perfiles ICC requeridos para el motor editorial y separación CMYK
COPY fonts /app/fonts
COPY ISOnewspaper26v4.icc /app/
COPY ISOnewspaper26v4_gr.icc /app/

# Copiar plantillas base (se respaldan en templates_default para inicializar volúmenes vacíos)
COPY templates/columna_opinion.* /app/templates_default/
COPY templates/editorial_reportaje.* /app/templates_default/
COPY templates/general.* /app/templates_default/
COPY templates/revista_muestra.* /app/templates_default/

COPY templates/columna_opinion.* /app/templates/
COPY templates/editorial_reportaje.* /app/templates/
COPY templates/general.* /app/templates/
COPY templates/revista_muestra.* /app/templates/

# Directorios de documentos y persistencia
RUN mkdir -p /app/documents /app/templates/cloud

# Script de inicio
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Declarar volúmenes persistentes para salvaguardar documentos y plantillas
VOLUME ["/app/documents", "/app/templates"]

# Puerto de servicio
EXPOSE 8767

# Verificación de salud integrada en serve.rs (/api/health)
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -f http://127.0.0.1:${PORT:-8767}/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
