# Unified Dockerfile for ferrules-api (CPU and GPU)
#
# CPU build (default):
#   docker build -f ferrules/Dockerfile ferrules/
#
# GPU build:
#   docker build -f ferrules/Dockerfile ferrules/ \
#     --build-arg BASE_BUILD=nvidia/cuda:12.3.2-cudnn9-devel-ubuntu22.04 \
#     --build-arg BASE_RUNTIME=nvidia/cuda:12.3.2-cudnn9-runtime-ubuntu22.04 \
#     --build-arg GPU=1

ARG BASE_BUILD=ubuntu:22.04
ARG BASE_RUNTIME=ubuntu:22.04

# ============================================================
# Builder stage
# ============================================================
FROM ${BASE_BUILD} AS builder

ARG RUST_VERSION=nightly-2025-08-15
ARG GPU=0
ARG ONNXRUNTIME_VERSION=1.22.0

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    clang \
    libclang-dev \
    curl \
    xz-utils \
    pkg-config \
    libssl-dev \
    zlib1g-dev \
    libtinfo-dev \
    libxml2-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain ${RUST_VERSION}
ENV PATH="/root/.cargo/bin:${PATH}"

# Copy source code and essential files
COPY . .

# Build the application
RUN cargo build --release -p ferrules-api

# Collect ONNX Runtime libraries into a single directory.
# CPU: download the CPU-only ONNX Runtime (smaller, no CUDA deps).
# GPU: copy the CUDA-enabled ONNX Runtime that ort fetched during cargo build.
RUN mkdir -p /app/onnx_libs && \
    if [ "${GPU}" = "0" ]; then \
        echo "Downloading ONNX Runtime CPU ${ONNXRUNTIME_VERSION}..." && \
        curl -L https://github.com/microsoft/onnxruntime/releases/download/v${ONNXRUNTIME_VERSION}/onnxruntime-linux-x64-${ONNXRUNTIME_VERSION}.tgz | \
        tar xzf - -C /app/onnx_libs --strip-components=2 --wildcards "*/lib/libonnxruntime.so*"; \
    else \
        echo "Copying CUDA-enabled ONNX Runtime from build output..." && \
        cp /app/target/release/*onnxruntime*.so* /app/onnx_libs/; \
    fi && \
    echo "ONNX Runtime libraries:" && ls -la /app/onnx_libs/

# ============================================================
# Runtime stage
# ============================================================
FROM ${BASE_RUNTIME}

ARG GPU=0

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app directory structure
RUN mkdir -p /app/models /app/scripts

# Set CUDA paths (only meaningful for GPU builds, harmless on CPU)
ENV CUDA_HOME=/usr/local/cuda
ENV PATH="${CUDA_HOME}/bin:${PATH}"
ENV LD_LIBRARY_PATH="${CUDA_HOME}/lib64:${LD_LIBRARY_PATH}"

# Copy binary and ONNX Runtime libraries
COPY --from=builder /app/target/release/ferrules-api /app/ferrules-api
COPY --from=builder /app/onnx_libs/ /usr/local/lib/

# Copy runtime data
COPY --from=builder /app/models/ /app/models/
COPY --from=builder /app/font/ /app/font/
COPY --from=builder /app/libs/ /app/libs/

# Copy init script
COPY --from=builder /app/scripts/docker-init.sh /app/scripts/
RUN chmod +x /app/scripts/docker-init.sh

# Verify all required files are present
RUN echo "Verifying model files..." && \
    test -f /app/models/yolov8s-doclaynet.onnx && \
    echo "Model files verified" && \
    echo "Verifying binaries..." && \
    test -f /app/ferrules-api && \
    echo "Binary files verified" && \
    echo "File verification complete!"

RUN ldconfig
WORKDIR /app

ENV FERRULES_ENABLE_CORRECTIONS=true
ENV FERRULES_LOG_LEVEL=info

ENTRYPOINT ["/app/scripts/docker-init.sh", "/app/ferrules-api"]
