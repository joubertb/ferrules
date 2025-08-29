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

echo "🎯 Starting Ferrules API with text corrections..."

# Execute the main application
exec "$@"