#!/bin/bash
set -e

echo "🚀 Initializing Ferrules container..."

# Text correction system enabled
echo "📚 Text correction system enabled"

# Set environment variables for cache configuration
export FERRULES_CORRECTION_CACHE_SIZE="${FERRULES_CORRECTION_CACHE_SIZE:-10000}"
export FERRULES_CORRECTION_CACHE_TTL_SECONDS="${FERRULES_CORRECTION_CACHE_TTL_SECONDS:-3600}"

# Set log level
export RUST_LOG="${FERRULES_LOG_LEVEL:-info}"

echo "🔧 Configuration:"
echo "  - Text corrections: enabled"
echo "  - Cache size: $FERRULES_CORRECTION_CACHE_SIZE"
echo "  - Cache TTL: ${FERRULES_CORRECTION_CACHE_TTL_SECONDS}s"
echo "  - Log level: $RUST_LOG"

# Health check function
health_check() {
    echo "🏥 Performing container health check..."
    
    # Dictionary files are embedded in binary - no external files needed
    
    # Check if main binary exists
    if [[ ! -x "/app/ferrules-api" ]]; then
        echo "❌ Ferrules API binary not available"
        return 1
    fi
    
    echo "✅ Health check passed"
    return 0
}

# Run health check
if ! health_check; then
    echo "💥 Health check failed, exiting..."
    exit 1
fi

# Auto-detect GPU and pick the matching ONNX Runtime distribution.
#
# Both CPU-only and GPU builds of libonnxruntime.so are shipped in the
# image. We select between them at startup with ORT_DYLIB_PATH (honored by
# the `ort` Rust crate's load-dynamic mode) plus LD_LIBRARY_PATH so the
# dynamic linker resolves any sibling .so dependencies from the same dir.
#
# This avoids loading libonnxruntime_providers_cuda.so on hosts where the
# NVIDIA driver is missing or broken — the GPU build's capability probe
# null-derefs there and segfaults the process.
GPU_ARGS=""
if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null; then
    GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)
    echo "🎮 GPU detected: ${GPU_NAME:-unknown}, enabling CUDA acceleration"
    ORT_LIB_DIR="/usr/local/lib/ort-gpu"
    GPU_ARGS="--cuda"
else
    echo "💻 No GPU detected, using CPU-only ONNX Runtime"
    ORT_LIB_DIR="/usr/local/lib/ort-cpu"
fi

if [[ ! -f "${ORT_LIB_DIR}/libonnxruntime.so" ]]; then
    echo "❌ libonnxruntime.so not found in ${ORT_LIB_DIR}"
    ls -la "${ORT_LIB_DIR}" || true
    exit 1
fi

export ORT_DYLIB_PATH="${ORT_LIB_DIR}/libonnxruntime.so"
export LD_LIBRARY_PATH="${ORT_LIB_DIR}:${LD_LIBRARY_PATH}"
echo "  - ORT_DYLIB_PATH: ${ORT_DYLIB_PATH}"

echo "🎯 Starting Ferrules API with text corrections..."

# Execute the main application with GPU args prepended
exec "$1" $GPU_ARGS "${@:2}"