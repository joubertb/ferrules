# Build arguments
ARG RUST_VERSION=latest
ARG DEBIAN_VERSION=bookworm

# Use cargo-chef for better caching
FROM lukemathwalker/cargo-chef:${RUST_VERSION}-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build dependencies
FROM chef AS builder
RUN apt-get update -y && apt-get install -y clang && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - cached if they don't change
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --release -p ferrules-api
RUN cargo build --release --bin font-analyzer

# Runtime stage
FROM debian:${DEBIAN_VERSION}-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates curl \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary and libs from builder
COPY --from=builder /app/target/release/libonnxruntime*.so /usr/local/lib/
COPY --from=builder /app/target/release/ferrules-api /app/ferrules-api
COPY --from=builder /app/target/release/font-analyzer /app/font-analyzer

# Copy configuration files (with default configs)
COPY --from=builder /app/configs /app/configs
RUN mkdir -p /app/configs

# Copy scripts for container initialization
COPY scripts/docker-init.sh /app/scripts/
RUN chmod +x /app/scripts/docker-init.sh

RUN ldconfig

# Initialize correction engine on startup
ENV FERRULES_CONFIG_PATH=/app/configs/font_corrections.json
ENV FERRULES_ENABLE_CORRECTIONS=true
ENV FERRULES_LOG_LEVEL=info

ENTRYPOINT ["/app/scripts/docker-init.sh", "/app/ferrules-api"]
