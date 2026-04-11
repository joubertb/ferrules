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

# Auto-detect GPU: if nvidia-smi is available and working, enable CUDA
GPU_ARGS=""
if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null; then
    GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)
    echo "🎮 GPU detected: ${GPU_NAME:-unknown}, enabling CUDA acceleration"
    GPU_ARGS="--cuda"
else
    echo "💻 No GPU detected, using CPU execution"
fi

echo "🎯 Starting Ferrules API with text corrections..."

# Execute the main application with GPU args prepended
exec "$1" $GPU_ARGS "${@:2}"