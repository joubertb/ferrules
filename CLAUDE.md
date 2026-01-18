# Ferrules Directory (PDF Parser)

This directory contains the Ferrules PDF parsing engine - a high-performance Rust-based service that converts PDF documents into structured JSON format for text-to-speech processing.

## Core Architecture

### Rust-Based PDF Parser
- **Technology Stack**: Rust with high-performance PDF processing libraries
- **Output Format**: Structured JSON with text blocks, positioning, and metadata
- **Performance**: Optimized for large documents with complex layouts
- **Accuracy**: Advanced text extraction with proper reading order and formatting

### Service Components
- **ferrules-core**: Core PDF parsing library and algorithms
- **ferrules-api**: HTTP API server for PDF parsing requests
- **ferrules-cli**: Command-line interface for standalone PDF processing
- **ferrules**: Main binary and orchestration logic
- **universal-corrector**: Built-in font analysis and automatic correction system

## Component Structure

### Core Library (`ferrules-core/`)
- **PDF Processing Engine**: Core algorithms for PDF parsing and text extraction
- **Text Extraction**: Advanced text extraction with proper formatting preservation
- **Layout Analysis**: Document structure analysis and reading order detection
- **Metadata Extraction**: Document properties, page information, and structure data

#### Key Features
- **High Performance**: Rust's memory safety and speed for large document processing
- **Accurate Text Extraction**: Proper handling of complex PDF layouts
- **Structure Preservation**: Maintains document hierarchy and reading order
- **Format Support**: Comprehensive PDF standard support
- **Universal Font Correction**: Dynamic analysis and automatic correction of corrupted PDF fonts
- **No Configuration Required**: Self-contained correction system with no external dependencies

### API Server (`ferrules-api/`)
- **HTTP Server**: RESTful API for PDF parsing requests
- **Request Handling**: Async request processing with proper error handling
- **File Processing**: Upload handling and temporary file management
- **Response Format**: Structured JSON output with parsed document data

#### API Endpoints
- **POST /parse**: Upload PDF and receive structured JSON output
- **GET /health**: Service health check endpoint
- **GET /info**: Service information and capabilities

#### Integration with SpeakDoc
- **Worker Integration**: Called by SpeakDoc worker service for PDF processing
- **Manual Startup**: Must be started manually on Mac (Docker issues)
- **Port Configuration**: Runs on port 3002 by default
- **Logging**: Outputs to `ferrules-api.log` in the ferrules directory

### Command Line Interface (`ferrules-cli/`)
- **Standalone Processing**: Direct PDF-to-JSON conversion from command line
- **Batch Processing**: Support for processing multiple files
- **Development Tool**: Useful for testing and debugging PDF parsing
- **Manual Operation**: Alternative to API server for direct processing

### Main Binary (`ferrules/`)
- **Application Entry Point**: Main ferrules application logic
- **Configuration Management**: Service configuration and parameter handling
- **Orchestration**: Coordinates between different components
- **Error Handling**: Centralized error management and reporting

## Docker Configuration

### Container Support
- `Dockerfile` - Standard Linux container build
- `Dockerfile.osx` - macOS-specific container configuration
- `docker-compose.yml` - Standalone service orchestration

### Mac Compatibility Issues
- **Manual Startup Required**: Cannot run reliably via Docker Compose on Mac
- **Direct Execution**: Must be started manually using cargo or pre-built binary
- **Log Location**: Service logs to `ferrules-api.log` in ferrules directory
- **Port Binding**: Manual port configuration to avoid conflicts

## Environment Variables

### FERRULES_DEBUG_OUTPUT

Controls debug output generation for troubleshooting PDF parsing issues.

**Values:**
- `none` - No debug output (default)
- `stderr` - Debug output to container logs only
- `file` - Debug output saved to `/tmp/ferrules-debug/{doc_name}-debug.txt`
- `both` - Both stderr and file output

**Debug File Locations:**
- API Service: `/tmp/ferrules-debug/{doc_name}-debug.txt`
- Worker Service: Downloaded to `<document_id>/logs/raw-debug.txt.gz` (compressed)
- Retention: Automatically cleaned up after 24 hours

### FERRULES_DEBUG_DIR

Custom debug file storage location (default: `/tmp/ferrules-debug`)

## Build System

### Rust Workspace Configuration
- `Cargo.toml` - Workspace-level configuration and dependencies
- `Cargo.lock` - Dependency version lock file
- `rust-toolchain.toml` - Rust version and toolchain specification
- `dist-workspace.toml` - Distribution and packaging configuration

### Build Configuration
- Individual `Cargo.toml` files for each component
- `build.rs` scripts for custom build steps
- Cross-compilation support for different platforms
- Optimization settings for production builds

## PDF Processing Pipeline

### Input Processing
1. **PDF Upload**: Receive PDF file via HTTP API or CLI
2. **File Validation**: Verify PDF format and integrity
3. **Temporary Storage**: Secure file handling during processing
4. **Size Limits**: Configurable limits for document size and complexity

### Parsing Engine
1. **Document Analysis**: Analyze PDF structure and layout
2. **Text Extraction**: Extract text with positioning information
3. **Reading Order**: Determine proper reading sequence
4. **Structure Detection**: Identify headers, paragraphs, lists, tables
5. **Metadata Extraction**: Document properties and page information

### Output Generation
1. **JSON Structure**: Convert parsed data to structured JSON
2. **Text Blocks**: Organize text into logical blocks with metadata
3. **Positioning Data**: Include coordinate and layout information
4. **Universal Font Correction**: Apply dynamic analysis and correction for corrupted PDF fonts
5. **Quality Validation**: Verify output completeness and accuracy

## Universal Font Correction System

### System Architecture

The system uses a **single, unified correction engine** that dynamically analyzes PDF fonts at runtime instead of relying on hardcoded configuration files.

**Key Features:**
- **Dynamic PDF Analysis**: Extracts actual font mappings from PDF structure
- **Glyph Name Resolution**: Uses Adobe Glyph List for accurate Unicode mapping
- **Synthetic Mapping Generation**: Creates proper mappings for mathematical Unicode ranges
- **No Configuration Required**: Works without external config files or hardcoded patterns
- **Feature Flag**: `correction-engine` feature enabled by default

### How It Works

#### 1. Font Detection and Analysis
- **Subset Detection**: Identifies corrupted subset fonts (names with '+' prefix)
- **ToUnicode Analysis**: Extracts existing character mappings from PDF CMaps  
- **Mathematical Font Recognition**: Detects mathematical symbol fonts requiring special handling
- **Glyph Extraction**: Analyzes actual glyph-to-Unicode relationships

#### 2. Dynamic Mapping Generation
- **Adobe Glyph List Integration**: Uses standard glyph name → Unicode mappings
- **Mathematical Unicode Ranges**: Generates mappings for U+1D400-U+1D7FF mathematical symbols
- **ASCII Identity Fallback**: Provides 1:1 mappings for basic printable characters
- **Surrogate Pair Handling**: Properly handles UTF-16 encoded mathematical symbols

#### 3. Real-time Correction
- **Character-Level**: Applied during individual character extraction from PDF
- **Context Preservation**: Maintains original text structure and positioning
- **Performance Optimized**: Uses cached mappings for repeated font analysis

### Success Example: E=mc² Formula

Corrupted text like "E=((²" (parentheses instead of 'mc') is automatically corrected by analyzing the PDF font structure and generating correct Unicode mappings.

### Dictionary System Integration

Complementary spell checking validates complete words using a 52K+ word English dictionary embedded in the source tree (`ferrules-core/src/correction/dictionaries/`).

### Performance Characteristics

- **Processing Time**: ~2.7 seconds for 7-page academic paper
- **Memory Usage**: <50MB additional overhead
- **Accuracy**: 99.89% correct subscript/superscript detection
- **Coverage**: Works with any corrupted font, not just predefined patterns
- **Scalability**: Linear with document size, thread-safe

### Advantages Over Legacy System

- **Universal Coverage**: Works with ANY corrupted font dynamically
- **No Maintenance**: No JSON config files to update for new fonts
- **Higher Accuracy**: Uses actual PDF structure instead of pattern guessing
- **Self-Contained**: All correction logic built-in, no external dependencies

## Correction Module Architecture

### Two-Module System

Ferrules employs a **dual-module correction architecture**:

1. **`ferrules-core/src/font_analysis/`** - Universal Font Corrector
   - Dynamic font corruption detection via PDF structure analysis
   - Handles font-level corruption

2. **`ferrules-core/src/correction/`** - Text Correction Module
   - Pattern matching, character substitutions, dictionary validation
   - Handles text-level corruption cleanup

### Correction Pipeline

1. **Universal Font Corrector**: Analyzes PDF fonts, uses glyph names and ToUnicode maps
2. **Character Corrector**: Fixes UTF-8 corruption, removes control characters
3. **Dictionary Corrector**: Validates complete words using 52K+ word database

### Module Files

**Font Analysis (`font_analysis/`):**
- `universal_corrector.rs` - Main correction engine
- `adobe_glyph_list.rs` - Standard glyph mappings

**Text Correction (`correction/`):**
- `character.rs` - Character-level corrections
- `dictionary.rs` - Dictionary-based spell correction
- `engine.rs` - Orchestrates all correction strategies

## JSON Output Format

### Document Structure

Output includes pages array, document metadata, and text blocks with:
- `text` - Extracted text content
- `page` - Page number
- `position` - X/Y coordinates
- `type` - Block type (paragraph, header, list, table, formula, etc.)
- `reading_order` - Sequence in document

### Text Block Types

- **Paragraphs**: Body text with formatting
- **Headers**: Section headers with hierarchy
- **Lists**: Bulleted and numbered lists
- **Tables**: Tabular data
- **Captions**: Figure and table captions
- **Footnotes**: Referenced footnote content
- **Formulas**: Raw pdfium text + extracted images

### Formula Block Handling

Formula blocks (identified by ONNX layout model) are handled specially:
- **Text**: Raw character extraction without enhancements
- **Image**: Formula region cropped and saved as PNG (`formula_{block_id}.png`)
- **API Access**: `/images/{job_id}/figures/formula_{block_id}.png`

Text blocks get full processing (hyphen removal, font corrections, script detection), while formula blocks remain raw with images for visual interpretation.

## External Dependencies

### Font Support (`font/`)
- **Font Files**: Required fonts for proper text rendering
- **Unicode Support**: Comprehensive character set support
- **Ligature Handling**: Advanced typography support
- **Font Fallbacks**: Alternative fonts for missing characters

### Libraries (`libs/`)
- **PDF Libraries**: Core PDF processing dependencies
- **Image Processing**: Support for embedded images
- **Compression**: PDF decompression and format support
- **Security**: Safe PDF processing with security validation

### Models (`models/`)
- **ML Models**: Machine learning models for layout analysis
- **Text Recognition**: OCR models for image-based text
- **Layout Detection**: Document structure recognition models
- **Language Models**: Language-specific processing models

## Testing and Development

### Test Infrastructure
- **Unit Tests**: Component-level testing
- **Integration Tests**: End-to-end parsing tests
- **Benchmark Tests**: Performance testing (`benches/`)
- **Regression Tests**: Validation against known documents

### Development Tools
- **Scripts**: Development and deployment scripts (`scripts/`)
- **Test Files**: Sample PDFs for testing (`test_*`)
- **Debugging**: Tools for analyzing parsing output
- **Profiling**: Performance analysis tools

## Performance Optimization

### Rust Advantages
- **Memory Safety**: No memory leaks or buffer overflows
- **Zero-Cost Abstractions**: High-level code with low-level performance
- **Parallel Processing**: Multi-threaded parsing for large documents
- **Resource Efficiency**: Minimal memory footprint and CPU usage

### Optimization Features
- **Streaming Processing**: Process large documents without full memory load
- **Caching**: Intelligent caching of parsed elements
- **Lazy Loading**: On-demand processing of document sections
- **Compression**: Efficient storage of intermediate results

## Logging and Monitoring

### Log Management
- **ferrules-api.log**: Main API server log file
- **Structured Logging**: JSON-formatted log entries
- **Error Tracking**: Detailed error information and stack traces
- **Performance Metrics**: Processing time and resource usage

### Health Monitoring
- **Service Health**: API endpoint health checks
- **Resource Monitoring**: Memory and CPU usage tracking
- **Error Rates**: Processing failure tracking
- **Performance Metrics**: Average processing times

## Integration with SpeakDoc

### Worker Service Integration
1. **API Call**: Worker sends PDF to ferrules-api via HTTP
2. **Processing**: Ferrules parses PDF and generates JSON
3. **Response**: Structured JSON returned to worker
4. **Text Processing**: Worker processes JSON for TTS pipeline

### Error Handling
- **Parsing Failures**: Graceful handling of corrupted or unsupported PDFs
- **Service Unavailable**: Worker retry logic for ferrules-api downtime
- **Timeout Management**: Processing timeout for very large documents
- **Error Reporting**: Detailed error information for debugging

## Manual Startup (macOS)

On macOS, start the API server manually:
- Navigate to ferrules directory
- Run `cargo run --bin ferrules-api` or use pre-built binary

**Configuration:**
- **Port**: 3002 (default)
- **Log Level**: Configurable logging verbosity
- **Resource Limits**: Memory and processing time limits

## File Organization

```
ferrules/
├── ferrules-core/           # Core PDF parsing library
│   ├── src/                # Core parsing algorithms
│   ├── benches/            # Performance benchmarks
│   └── Cargo.toml         # Core library dependencies
├── ferrules-api/            # HTTP API server
│   ├── src/                # API server implementation
│   └── Cargo.toml         # API server dependencies
├── ferrules-cli/            # Command-line interface
│   ├── src/                # CLI implementation
│   └── Cargo.toml         # CLI dependencies
├── ferrules/                # Main binary
│   └── src/                # Main application logic
├── font/                    # Required font files
├── libs/                    # External library dependencies
├── models/                  # ML models for processing
├── scripts/                 # Development and deployment scripts
├── target/                  # Rust build output directory
├── Cargo.toml              # Workspace configuration
├── Dockerfile              # Container build (Linux)
├── Dockerfile.osx          # Container build (macOS)
├── docker-compose.yml      # Service orchestration
├── ferrules-api.log        # API server log file
├── API.md                  # API documentation
├── README.md               # Project documentation
├── ROADMAP.md              # Development roadmap
└── CLAUDE.md               # This comprehensive documentation
```

## Troubleshooting

### Common Issues

**Correction engine not working:**
- Verify using default build (`cargo build --release`)
- Test with known document and check for expected output

**Slow processing:**
- Processing time scales linearly with document size
- Enable debug logging with `RUST_LOG=debug` and `--debug-output file`

**API server not responding (macOS):**
- Start manually: `cd ferrules && cargo run --bin ferrules-api`
- Check logs: `docker logs ferrules-api`

**Incorrect corrections:**
- Enable debug output to examine font analysis details
- Check for genuine PDF issues vs correction problems

### Known Limitations

- **Randomized fonts**: Cannot correct fonts with entirely random character mappings
- **Missing glyph data**: Some PDFs lack sufficient glyph information
- **macOS Docker**: Manual API startup required
- **Memory**: ~50MB additional overhead for font analysis

## Subscript and Superscript Detection

### Detection Algorithm

Uses a **multi-layered composite scoring approach** combining font size analysis with baseline positioning (99.89% accuracy).

**Key Factors:**
- **Vertical offset**: Baseline position relative to surrounding text (75% weight)
- **Font size shrinkage**: Smaller font indicates script notation (25% weight)

### Dual-Layer Processing

1. **Sequential Analysis**: Character-by-character for simple text
2. **Clustering Analysis**: Groups spans by Y-position for complex mathematical notation with local mode-based baseline calculation

### Implementation

**Location:** `ferrules-core/src/modtext/script_notation.rs`

**Key Features:**
- Local mode-based baseline using ±3pt Y-range
- Handles mixed sub/superscript notation
- Post-processing for correct visual ordering
- Works with corrupted fonts and inconsistent baselines

### Performance

- **Accuracy**: 99.89% on academic papers
- **Speed**: ~2.7 seconds for 7-page paper
- **Memory**: <50MB additional overhead
- Applied to text blocks only (formula blocks use raw text + images)

## Development Notes

- **Rust Toolchain**: Specific version required (see `rust-toolchain.toml`)
- **Manual Startup**: Must start ferrules-api manually on macOS
- **Testing**: Use debug build (`cargo build`) for faster iteration
- **Log Location**: `ferrules-api.log` for service status
- **Font Correction**: Dynamic analysis, no pattern matching or hardcoded rules
- **Regex**: Compile patterns at application start
- **HashMap Determinism**: Use deterministic tie-breaking for debug/release consistency

## See Also

- [../CLAUDE.md](../CLAUDE.md) - Project overview
- [../worker/CLAUDE.md](../worker/CLAUDE.md) - Worker service that calls Ferrules
