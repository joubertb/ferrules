# Ferrules Essential Commands

## Build Commands
```bash
# Development build
cargo build

# Optimized release build
cargo build --release

# Build specific crate
cargo build -p ferrules-core
cargo build -p ferrules-api
cargo build -p ferrules-cli
```

## Testing Commands
```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p ferrules-core

# Run tests with output
cargo test -- --nocapture

# Test specific module
cargo test font_analysis
```

## Code Quality Commands
```bash
# Format code (uses .rustfmt.toml config)
cargo fmt

# Check formatting without changing files
cargo fmt --check

# Lint code with clippy
cargo clippy

# Clippy with all features and strict mode
cargo clippy --all-features -- -D warnings
```

## Running the Application
```bash
# CLI tool - process PDF
cargo run --bin ferrules-cli -- document.pdf

# CLI with debug mode
cargo run --bin ferrules-cli -- document.pdf --debug

# API server (for integration testing)
cargo run --bin ferrules-api

# Using release builds
./target/release/ferrules-cli document.pdf
./target/release/ferrules-api
```

## Development Workflow
```bash
# Clean build artifacts
cargo clean

# Check code without building
cargo check

# Build documentation
cargo doc --open

# Update dependencies
cargo update
```

## Docker Commands
```bash
# Build container
docker build -t ferrules .

# Build GPU variant
docker build -f Dockerfile.gpu -t ferrules-gpu .

# Run API server in container
docker run -p 3002:3002 ferrules

# Use docker-compose
docker-compose up ferrules
```

## macOS Development Notes
- **Manual API Startup**: Ferrules API must be started manually on macOS (Docker compatibility issues)
- **Hardware Acceleration**: Uses CoreML and Apple Neural Engine when available
- **Font Tools**: `pdffonts` command available for font analysis debugging

## Quick Testing
```bash
# Test with mathbert.pdf (common test document)
cargo run --bin ferrules-cli mathbert.pdf --output-dir test-results

# Enable debug logging
RUST_LOG=debug cargo run --bin ferrules-cli mathbert.pdf

# Test font correction system
cargo run --bin ferrules-cli mathbert.pdf --debug
```