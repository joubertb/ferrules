#!/bin/bash
set -e

echo "🚀 Initializing Ferrules container..."

# Configuration validation
CONFIG_PATH="${FERRULES_CONFIG_PATH:-/app/configs/font_corrections.json}"

echo "📋 Validating font correction configuration..."
if [[ -f "$CONFIG_PATH" ]]; then
    echo "✅ Font corrections config found: $CONFIG_PATH"
    
    # Validate JSON syntax
    if /app/font-analyzer validate-config --config "$CONFIG_PATH" 2>/dev/null; then
        echo "✅ Configuration file is valid"
    else
        echo "⚠️  Configuration file validation failed, but continuing..."
    fi
else
    echo "⚠️  Font corrections config not found: $CONFIG_PATH"
    echo "📝 Creating default configuration..."
    
    mkdir -p "$(dirname "$CONFIG_PATH")"
    cat > "$CONFIG_PATH" << 'EOF'
{
  "version": "1.0.0",
  "description": "Default font correction configuration",
  "last_updated": "2025-08-27T00:00:00Z",
  "font_corrections": {
    "FYEQFE+NimbusRomNo9L-Regu": {
      "description": "Nimbus Roman Regular subset font",
      "corrections": {
        "(": "h",
        ")": "i"
      },
      "confidence": 0.90,
      "enabled": true,
      "is_subset": true,
      "has_tounicode": false
    },
    "CMSY10": {
      "description": "Computer Modern Symbol font",
      "corrections": {
        "{": "ff",
        "}": "ffi"
      },
      "confidence": 0.85,
      "enabled": true,
      "is_subset": false,
      "has_tounicode": false
    }
  },
  "pattern_corrections": {
    "whic(": "which",
    "t(e": "the",
    "g)ven": "given"
  },
  "settings": {
    "enable_font_corrections": true,
    "enable_pattern_corrections": true,
    "confidence_threshold": 0.7,
    "max_corrections_per_word": 3,
    "enable_diagnostic_logging": false
  }
}
EOF
    echo "✅ Default configuration created"
fi

# Set log level
export RUST_LOG="${FERRULES_LOG_LEVEL:-info}"

echo "🔧 Configuration:"
echo "  - Config path: $CONFIG_PATH"
echo "  - Enable corrections: ${FERRULES_ENABLE_CORRECTIONS:-true}"
echo "  - Log level: $RUST_LOG"

# Health check function
health_check() {
    echo "🏥 Performing container health check..."
    
    # Check if configuration files are readable
    if [[ ! -r "$CONFIG_PATH" ]]; then
        echo "❌ Configuration file not readable: $CONFIG_PATH"
        return 1
    fi
    
    # Check if CLI tools are available
    if [[ ! -x "/app/font-analyzer" ]]; then
        echo "❌ Font analyzer tool not available"
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

echo "🎯 Starting Ferrules API with font corrections..."
echo "📡 CLI tools available:"
echo "  - /app/font-analyzer analyze --pdf <file> --output <report>"
echo "  - /app/font-analyzer generate --report <file> --output <corrections>"
echo "  - /app/font-analyzer validate --pdf <file> --config <config>"

# Execute the main application
exec "$@"