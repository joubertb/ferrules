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

# Copy source code and essential files
COPY ferrules-core/ ./ferrules-core/
COPY ferrules-api/ ./ferrules-api/
COPY ferrules-cli/ ./ferrules-cli/
COPY Cargo.toml Cargo.lock ./

COPY ferrules-core/src/correction/dictionaries/ ./dictionaries/

# Copy models directory
COPY models/ ./models/

# Build application (dictionary corrections only)
RUN cargo build --release -p ferrules-api

# Runtime stage
FROM debian:${DEBIAN_VERSION}-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates curl \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Create app directory structure
RUN mkdir -p /app/dictionaries /app/models /app/scripts

# Copy the binary and libs from builder
COPY --from=builder /app/target/release/libonnxruntime*.so /usr/local/lib/
COPY --from=builder /app/target/release/ferrules-api /app/ferrules-api
# Copy dictionaries directly from builder (no intermediate compression layer)
COPY --from=builder /app/dictionaries/ /app/dictionaries/

# Copy models
COPY --from=builder /app/models/ /app/models/

# Copy scripts for container initialization
COPY scripts/docker-init.sh /app/scripts/
RUN chmod +x /app/scripts/docker-init.sh

# Verify all required files are present
RUN echo "Verifying dictionary files..." && \
    test -f /app/dictionaries/en_US.aff && \
    test -f /app/dictionaries/en_US.dic && \
    test -f /app/dictionaries/basic_english.dic && \
    echo "✓ Dictionary files verified" && \
    echo "Verifying model files..." && \
    test -f /app/models/yolov8s-doclaynet.onnx && \
    echo "✓ Model files verified" && \
    echo "Verifying binaries..." && \
    test -f /app/ferrules-api && \
    echo "✓ Binary files verified" && \
    echo "File verification complete!"

RUN ldconfig

ENV FERRULES_ENABLE_CORRECTIONS=true
ENV FERRULES_LOG_LEVEL=info

ENTRYPOINT ["/app/scripts/docker-init.sh", "/app/ferrules-api"]
