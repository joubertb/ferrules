# Unified Dockerfile for ferrules-api
#
# Builds a single image that supports both CPU and GPU execution.
# GPU acceleration is enabled at runtime via the --cuda flag
# (added by docker-compose-gpu.yml). Without --cuda, ORT uses CPU only.
#
# Build:
#   docker build -f ferrules/Dockerfile ferrules/

ARG BASE_BUILD=nvidia/cuda:12.8.1-cudnn-devel-ubuntu22.04
ARG BASE_RUNTIME=nvidia/cuda:12.8.1-cudnn-runtime-ubuntu22.04

# ============================================================
# Builder stage
# ============================================================
FROM ${BASE_BUILD} AS builder

ARG RUST_VERSION=nightly-2025-08-15
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

# Build with ort-dynamic: ORT is loaded at runtime, enabling GPU/CPU selection
RUN cargo build --release -p ferrules-api --features ferrules-core/ort-dynamic

# Download GPU-enabled ONNX Runtime (includes CUDA/TensorRT EPs + CPU fallback)
RUN mkdir -p /app/onnx_libs && \
    echo "Downloading ONNX Runtime GPU ${ONNXRUNTIME_VERSION}..." && \
    curl -L https://github.com/microsoft/onnxruntime/releases/download/v${ONNXRUNTIME_VERSION}/onnxruntime-linux-x64-gpu-${ONNXRUNTIME_VERSION}.tgz | \
    tar xzf - -C /app/onnx_libs --strip-components=2 --wildcards "*/lib/libonnxruntime*.so*" && \
    echo "ONNX Runtime libraries:" && ls -la /app/onnx_libs/

# ============================================================
# Runtime stage
# ============================================================
FROM ${BASE_RUNTIME}

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app directory structure
RUN mkdir -p /app/models /app/scripts

# Set CUDA paths (harmless on CPU-only execution)
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
