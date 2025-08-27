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
- **font-analyzer**: CLI tool for analyzing PDF fonts and generating correction suggestions

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
- **Character Corruption Detection**: Advanced font analysis and correction system for corrupted PDF text
- **Hot-Reloadable Corrections**: External JSON configuration for font-specific character corrections

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
4. **Character Correction**: Apply font-specific corrections for corrupted text
5. **Quality Validation**: Verify output completeness and accuracy

## Font Correction System

### Overview
Ferrules includes an advanced font correction system to address character encoding corruption commonly found in PDF text extraction. This system automatically detects and corrects systematic character corruption caused by missing ToUnicode mappings in subset fonts.

### Root Cause of Corruption
PDF subset fonts often have corrupted or missing character mapping tables:
- **Subset Fonts**: Font names with '+' prefix (e.g., "FYEQFE+NimbusRomNo9L-Regu") indicate subsetted fonts
- **Missing ToUnicode Maps**: 64+ fonts in typical academic PDFs lack proper Unicode mapping
- **Character Code Misalignment**: Systematic corruption like `(` → `h`, `)` → `i`, `{` → `ff`

### System Architecture

#### Configuration-Driven Corrections (`configs/font_corrections.json`)
```json
{
  "font_corrections": {
    "FYEQFE+NimbusRomNo9L-Regu": {
      "corrections": { "(": "h", ")": "i", "[": "fi", "]": "fl" },
      "confidence": 0.95,
      "enabled": true
    },
    "CMSY10": {
      "corrections": { "{": "ff", "}": "ffi" },
      "confidence": 0.80,
      "enabled": true
    }
  },
  "pattern_corrections": {
    "whic(": "which", "t(e": "the", "g)ven": "given"
  }
}
```

#### Hot-Reloadable Correction Engine
- **External Configuration**: JSON files can be updated without recompilation
- **Runtime Reloading**: Configuration changes detected and applied automatically
- **Font-Specific Rules**: Targeted corrections based on exact font names
- **Pattern Matching**: Context-aware corrections for common corruption patterns
- **Confidence Scoring**: Validate corrections with statistical confidence

### Font Analysis and Diagnostic Tools

#### Font Analyzer CLI (`font-analyzer`)
```bash
# Analyze PDF for font corruption
ferrules font-analyzer analyze --pdf document.pdf --output analysis.json

# Generate correction suggestions from analysis
ferrules font-analyzer generate --report analysis.json --output corrections.json

# Validate corrections against PDF
ferrules font-analyzer validate --pdf document.pdf --config corrections.json

# Add new font corrections
ferrules font-analyzer add --font "FontName" --corrections "(:h,):i" --config corrections.json
```

#### Diagnostic Features
- **Automatic Font Detection**: Identifies problematic fonts with missing ToUnicode maps
- **Corruption Pattern Analysis**: Discovers systematic character replacements
- **Confidence Assessment**: Statistical analysis of correction accuracy
- **Recommendation Engine**: Suggests improvements and new correction rules

### Integration with Text Extraction

#### Correction Pipeline
1. **Font Analysis**: lopdf examines PDF font dictionaries for corruption indicators
2. **Character-Level Corrections**: Font-specific replacements applied during extraction
3. **Pattern-Level Corrections**: Context-aware fixes for multi-character corruptions
4. **Validation**: Confidence scoring ensures corrections improve rather than degrade text

#### Performance Optimization
- **Lazy Loading**: Corrections loaded on-demand for better startup performance
- **Caching**: Frequently used correction tables cached in memory
- **Parallel Processing**: Font analysis and corrections applied concurrently

### Docker Integration

#### Volume Configuration
```yaml
services:
  ferrules:
    volumes:
      - ./configs:/app/configs:ro  # Mount corrections for runtime updates
      - ./reports:/app/reports     # Analysis outputs
    environment:
      - FERRULES_CONFIG_PATH=/app/configs/font_corrections.json
      - FERRULES_ENABLE_CORRECTIONS=true
```

#### Container Features
- **Persistent Configuration**: Config changes survive container restarts
- **CLI Tool Access**: Font analyzer available inside container
- **Health Monitoring**: Configuration validation on startup
- **Log Management**: Structured logging for correction application

### Usage Examples

#### Analyzing New Documents
```bash
# Step 1: Analyze PDF for corruption patterns
docker exec ferrules-api font-analyzer analyze --pdf /app/uploads/document.pdf

# Step 2: Generate corrections from analysis
docker exec ferrules-api font-analyzer generate --report analysis.json --output new-corrections.json

# Step 3: Test corrections
docker exec ferrules-api font-analyzer validate --pdf document.pdf --config new-corrections.json

# Step 4: Merge with existing config (manual or scripted)
```

#### Production Workflow
1. **Development**: Use font-analyzer to discover corruption patterns in new document types
2. **Testing**: Validate correction accuracy against known documents
3. **Deployment**: Update mounted config volumes without container rebuild
4. **Monitoring**: Track correction statistics through application logs

### Known Font Issues

#### Common Problematic Fonts
- **Computer Modern (CM\*\*)**: Mathematical fonts with ligature corruption
- **Nimbus Roman (subset)**: Parentheses-to-letter corruption
- **Times/Arial variants**: Encoding table corruption in older PDFs
- **Mathematical Fonts**: CMSY, CMMI, CMEX families with symbol corruption

#### Validation Results
Based on analysis of academic papers (e.g., mathbert.pdf):
- **100% corruption rate** detected in subset fonts without ToUnicode
- **90%+ accuracy** achieved with font-specific correction tables
- **Real-time correction** with minimal performance impact

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

## Troubleshooting Font Correction System

### Common Issues and Solutions

#### Configuration Problems

**Issue**: Corrections not applied despite valid config file
```bash
# Check if corrections are enabled
docker logs ferrules-api | grep "FERRULES_ENABLE_CORRECTIONS"

# Verify config file syntax
docker exec ferrules-api font-analyzer validate-config --config /app/configs/font_corrections.json

# Test config reload
docker exec ferrules-api pkill -USR1 ferrules-api  # Trigger config reload
```

**Issue**: Font analyzer CLI not found in container
```bash
# Check if tool is available
docker exec ferrules-api ls -la /app/font-analyzer

# Rebuild container if missing
docker-compose build --no-cache ferrules
```

#### Correction Accuracy Problems

**Issue**: Corrections making text worse instead of better
```bash
# Analyze correction confidence scores
docker exec ferrules-api font-analyzer validate --pdf problem.pdf --config corrections.json --verbose

# Review diagnostic output
tail -f ferrules-api.log | grep "correction_confidence"

# Disable low-confidence corrections
# Edit configs/font_corrections.json: set "confidence": 0.95 for problematic fonts
```

**Issue**: Missing corrections for new font types
```bash
# Generate analysis for new document
docker exec ferrules-api font-analyzer analyze --pdf new-doc.pdf --output /app/reports/new-analysis.json

# Generate suggested corrections
docker exec ferrules-api font-analyzer generate --report /app/reports/new-analysis.json --output /app/reports/suggested.json

# Merge with existing config (manual process)
```

#### Performance Issues

**Issue**: Slow correction processing on large documents
```bash
# Check correction cache status
grep "correction_cache" ferrules-api.log

# Disable pattern corrections temporarily
# Set "enable_pattern_corrections": false in config

# Monitor memory usage
docker stats ferrules-api
```

**Issue**: High memory usage with correction engine
```bash
# Check for memory leaks in correction engine
docker exec ferrules-api ps aux | grep ferrules

# Restart correction engine
docker restart ferrules-api

# Review correction table sizes
du -sh /app/configs/font_corrections.json
```

#### Container and Volume Issues

**Issue**: Config changes not taking effect
```bash
# Verify volume mount
docker inspect ferrules-api | grep -A 10 "Mounts"

# Check file permissions
docker exec ferrules-api ls -la /app/configs/

# Manual config reload
docker exec ferrules-api curl -X POST http://localhost:3002/reload-config
```

**Issue**: Reports directory not accessible
```bash
# Create reports directory if missing
mkdir -p ./reports
chmod 755 ./reports

# Verify volume mount
docker-compose down && docker-compose up -d ferrules
```

### Diagnostic Commands

#### Quick Health Check
```bash
# Complete system status
docker exec ferrules-api font-analyzer validate-system

# Check correction engine status
curl http://localhost:3002/health | jq '.correction_engine'

# Verify config file integrity
docker exec ferrules-api jq empty /app/configs/font_corrections.json
```

#### Debug Font Corruption
```bash
# Analyze specific PDF for corruption patterns
docker exec ferrules-api font-analyzer analyze --pdf problem.pdf --debug --output debug-analysis.json

# Extract font information
docker exec ferrules-api font-analyzer fonts --pdf problem.pdf --format table

# Test specific font corrections
docker exec ferrules-api font-analyzer test-font --name "PROBLEMATIC+FontName" --input "corrupted text" --config corrections.json
```

#### Performance Profiling
```bash
# Enable detailed logging
export RUST_LOG=debug
docker restart ferrules-api

# Monitor correction statistics
tail -f ferrules-api.log | grep "correction_stats"

# Analyze processing times
docker exec ferrules-api font-analyzer benchmark --pdf large-document.pdf --iterations 10
```

### Recovery Procedures

#### Reset to Default Configuration
```bash
# Backup current config
cp configs/font_corrections.json configs/font_corrections.json.backup

# Generate fresh default config
docker exec ferrules-api /app/scripts/docker-init.sh --create-default-config

# Restart with clean config
docker restart ferrules-api
```

#### Rebuild Correction Database
```bash
# Clear existing corrections
echo '{"version": "1.0.0", "font_corrections": {}, "pattern_corrections": {}}' > configs/font_corrections.json

# Re-analyze document corpus
for pdf in documents/*.pdf; do
    docker exec ferrules-api font-analyzer analyze --pdf "$pdf" --append-to analysis-combined.json
done

# Generate comprehensive corrections
docker exec ferrules-api font-analyzer generate --report analysis-combined.json --output new-corrections.json
```

### Known Limitations

#### Font Detection Limitations
- Cannot correct fonts with completely randomized character codes
- Subset fonts without any Unicode clues may require manual correction tables
- OCR-generated PDFs may have inconsistent corruption patterns

#### Performance Constraints
- Large correction tables (>10MB) may impact startup time
- Pattern corrections with complex regex can slow down processing
- Hot-reloading disabled for very large configuration files

#### Container Compatibility
- macOS Docker performance issues may affect font analysis speed
- Some lopdf features require specific PDF library versions
- Font file embedding may not work in all container environments

## Development Notes

- **Rust Toolchain**: Specific Rust version required (see rust-toolchain.toml)
- **Manual Startup**: Must start ferrules-api manually on Mac
- **Docker Issues**: Container doesn't work reliably on macOS
- **Log Monitoring**: Check ferrules-api.log for service status
- **Performance**: Optimized for production use with large documents
- **Integration**: Critical component for SpeakDoc PDF processing pipeline
- **Font Correction**: External JSON configuration enables runtime updates without recompilation