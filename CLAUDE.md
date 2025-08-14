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
4. **Quality Validation**: Verify output completeness and accuracy

## JSON Output Format

### Document Structure
```json
{
  "pages": [...],
  "metadata": {
    "title": "Document Title",
    "page_count": 10,
    "processing_info": {...}
  },
  "text_blocks": [
    {
      "text": "Extracted text content",
      "page": 1,
      "position": {"x": 100, "y": 200},
      "type": "paragraph|header|list|table",
      "reading_order": 1
    }
  ]
}
```

### Text Block Types
- **Paragraphs**: Regular body text with proper formatting
- **Headers**: Section headers with hierarchy levels
- **Lists**: Bulleted and numbered lists with structure
- **Tables**: Tabular data with row/column organization
- **Captions**: Figure and table captions
- **Footnotes**: Referenced footnote content

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

## Manual Startup Process

### Mac Development Setup
```bash
# Navigate to ferrules directory
cd ferrules

# Start the API server manually
cargo run --bin ferrules-api

# Or use pre-built binary if available
./target/release/ferrules-api
```

### Configuration
- **Port**: Default port 3002 (configurable)
- **Log Level**: Configurable logging verbosity
- **Resource Limits**: Memory and processing time limits
- **Temporary Storage**: Configurable temp directory for processing

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

## Development Notes

- **Rust Toolchain**: Specific Rust version required (see rust-toolchain.toml)
- **Manual Startup**: Must start ferrules-api manually on Mac
- **Docker Issues**: Container doesn't work reliably on macOS
- **Log Monitoring**: Check ferrules-api.log for service status
- **Performance**: Optimized for production use with large documents
- **Integration**: Critical component for SpeakDoc PDF processing pipeline