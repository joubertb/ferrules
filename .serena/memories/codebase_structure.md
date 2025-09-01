# Ferrules Codebase Structure

## Workspace Organization
Ferrules is organized as a Cargo workspace with three main crates:

```
ferrules/
├── ferrules-core/          # Core PDF parsing library
├── ferrules-api/           # HTTP API server  
├── ferrules-cli/           # Command-line interface
└── ferrules/               # Main binary (legacy)
```

## ferrules-core/ - Core Library
The heart of the PDF processing system:

### Key Modules
- **`src/lib.rs`** - Public API and core exports
- **`src/entities.rs`** - Core data structures (BBox, CharSpan, Line, etc.)
- **`src/blocks.rs`** - Document block processing and organization
- **`src/parse/`** - PDF parsing implementations
  - `native.rs` - Native PDF parsing with pdfium2
  - `merge.rs` - Merging and post-processing
- **`src/layout/`** - ML-based layout analysis
- **`src/correction/`** - Font corruption detection and correction
  - `font_analysis.rs` - Font corruption analysis
  - `pdf_preprocessor.rs` - PDF preprocessing for font fixes
  - `unicode_validator.rs` - Unicode validation
- **`src/modtext/`** - Text modification and processing
- **`src/ocr/`** - OCR integration (Apple Vision)
- **`src/render/`** - Output rendering (HTML, Markdown)
- **`src/utils.rs`** - Utility functions

### Critical Components
- **Font Correction System**: Advanced detection and correction of corrupted fonts in mathematical PDFs
- **Layout Analysis**: ML-powered document structure detection
- **Text Extraction**: High-fidelity text extraction preserving document structure

## ferrules-api/ - HTTP Server
REST API server built with Axum:
- Accepts PDF uploads via HTTP
- Returns structured JSON output
- Integrated tracing and monitoring
- Configurable hardware acceleration

## ferrules-cli/ - Command Line Tool
Standalone CLI for PDF processing:
- Direct PDF-to-JSON conversion
- Debug mode with visual output
- Configurable processing options
- Progress indicators

## Supporting Infrastructure
- **`libs/`** - External library dependencies
- **`font/`** - Required font files for text rendering
- **`models/`** - ML models for layout analysis
- **`scripts/`** - Development and deployment scripts
- **`target/`** - Rust build outputs (gitignored)

## Configuration Files
- **`Cargo.toml`** - Workspace configuration and shared dependencies
- **`rust-toolchain.toml`** - Specifies nightly Rust toolchain
- **`.rustfmt.toml`** - Code formatting configuration
- **`docker-compose.yml`** - Container orchestration
- **Multiple Dockerfiles** - For different deployment targets

## Build Artifacts
- **Debug**: `target/debug/` - Development builds
- **Release**: `target/release/` - Optimized production builds
- **Container Images**: Multi-platform Docker support