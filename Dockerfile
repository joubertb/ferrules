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

ARG ONNXRUNTIME_VERSION=1.22.0
# SHA-256 of the NuGet packages for ONNXRUNTIME_VERSION; update all three together.
#   GPU: Microsoft.ML.OnnxRuntime.Gpu.Linux   CPU: Microsoft.ML.OnnxRuntime
ARG ONNXRUNTIME_GPU_SHA256=d3dabd8235b7a04a67838a3b93437420f1a709528a8d8611e08bedbb08206d1d
ARG ONNXRUNTIME_CPU_SHA256=d571e63a2329baacb713f441e65ad75284de354db6e1ac435fe4bebbb417986a

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    clang \
    libclang-dev \
    curl \
    xz-utils \
    unzip \
    pkg-config \
    libssl-dev \
    zlib1g-dev \
    libtinfo-dev \
    libxml2-dev \
    && rm -rf /var/lib/apt/lists/*

# Install rustup only; the toolchain version comes from rust-toolchain.toml
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
ENV PATH="/root/.cargo/bin:${PATH}"

# Copy source code and essential files
COPY . .

# Install the toolchain pinned in rust-toolchain.toml
RUN rustup toolchain install

# Build with ort-dynamic: ORT is loaded at runtime, enabling GPU/CPU selection
RUN cargo build --release -p ferrules-api --features ferrules-core/ort-dynamic

# Download both ONNX Runtime distributions:
#   - GPU build: includes CUDA/TensorRT EPs (used when --cuda is enabled)
#   - CPU-only build: avoids loading libonnxruntime_providers_cuda.so on hosts
#     where the NVIDIA driver is missing or broken (its capability probe at
#     session init can null-deref and segfault the process).
# At runtime, docker-init.sh selects which one to load via ORT_DYLIB_PATH.
# From NuGet: github.com/microsoft/onnxruntime releases went 404 (2026-11-18).
RUN fetch_ort() { \
        pkg="$1"; sha="$2"; dest="$3"; \
        echo "Downloading ${pkg} ${ONNXRUNTIME_VERSION}..." && \
        curl -fsSL -o /tmp/ort.nupkg \
            "https://api.nuget.org/v3-flatcontainer/${pkg}/${ONNXRUNTIME_VERSION}/${pkg}.${ONNXRUNTIME_VERSION}.nupkg" && \
        echo "${sha}  /tmp/ort.nupkg" | sha256sum -c - && \
        mkdir -p "${dest}" && \
        unzip -j /tmp/ort.nupkg 'runtimes/linux-x64/native/*.so' -d "${dest}" && \
        rm /tmp/ort.nupkg; \
    } && \
    fetch_ort microsoft.ml.onnxruntime.gpu.linux "${ONNXRUNTIME_GPU_SHA256}" /app/onnx_libs/gpu && \
    fetch_ort microsoft.ml.onnxruntime "${ONNXRUNTIME_CPU_SHA256}" /app/onnx_libs/cpu && \
    echo "ONNX Runtime GPU libraries:" && ls -la /app/onnx_libs/gpu/ && \
    echo "ONNX Runtime CPU libraries:" && ls -la /app/onnx_libs/cpu/

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

# Copy binary and ONNX Runtime libraries (both CPU-only and GPU variants).
# docker-init.sh picks one at startup via ORT_DYLIB_PATH based on whether
# nvidia-smi reports a working GPU. The CUDA EP shared object is therefore
# never mapped into the process on CPU-only hosts.
COPY --from=builder /app/target/release/ferrules-api /app/ferrules-api
COPY --from=builder /app/onnx_libs/gpu/ /usr/local/lib/ort-gpu/
COPY --from=builder /app/onnx_libs/cpu/ /usr/local/lib/ort-cpu/

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
