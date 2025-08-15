# Ferrules GitHub Actions Configuration

## Docker Hub Setup

To enable automatic container builds and pushes to Docker Hub, configure the following secrets in your GitHub repository settings:

### Required Secrets

1. **DOCKERHUB_USERNAME**
   - Your Docker Hub username
   - Example: `joubertb`

2. **DOCKERHUB_TOKEN** 
   - Docker Hub access token (not your password)
   - Create at: https://hub.docker.com/settings/security
   - Use "Read, Write, Delete" permissions

### Setting up secrets:

1. Go to your GitHub repository
2. Navigate to Settings → Secrets and variables → Actions
3. Click "New repository secret"
4. Add each secret with the exact name and value

## Workflow Details

### Ferrules Container Workflow
- **File**: `.github/workflows/build-ferrules.yml`
- **Triggers**: 
  - Push to `pdftotts` branch (excludes documentation changes)
  - Pull requests to `pdftotts` branch
- **Features**:
  - Multi-platform builds (linux/amd64, linux/arm64)
  - Docker layer caching via GitHub Actions cache
  - Registry cache for faster subsequent builds
  - Service-specific tagging with `ferrules-` prefix

### Image Tags

The workflow creates the following tags:
- `ferrules-latest` - Latest build from pdftotts branch
- `ferrules-pdftotts-<git-sha>` - Specific commit builds from pdftotts branch
- `ferrules-pr-<number>` - Pull request builds (not pushed to registry)

## Integration with SpeakDoc

This Ferrules PDF parsing service is designed to integrate with the SpeakDoc application:

### Docker Hub Repository
- **Repository**: `joubertb/repos`
- **Image Tags**: `ferrules-latest`, `ferrules-pdftotts-<sha>`
- **Usage**: Pull image for SpeakDoc deployment

### SpeakDoc Integration
The main SpeakDoc application references this image in its `docker-compose-speakdoc.yml`:
```yaml
ferrules-api:
  image: joubertb/repos:ferrules-latest
```

## Build Process

### Rust Build Pipeline
1. **Dependency Installation**: Install Rust toolchain and system dependencies
2. **Library Setup**: Install PDFium and other native libraries
3. **Compilation**: Build optimized Rust binaries for PDF parsing
4. **Container Assembly**: Package service into runtime container

### Build Optimization
- **Multi-stage Build**: Separate build and runtime environments using cargo-chef
- **Caching**: GitHub Actions and registry caching for faster builds
- **Cross-platform**: Builds for both AMD64 and ARM64 architectures
- **Size Optimization**: Minimal runtime container with only necessary components

## Service Configuration

### Port Configuration
- **Default Port**: 3002
- **API Endpoints**: RESTful PDF parsing API
- **Health Check**: `/health` endpoint for service monitoring

### Performance Notes
- **Large Files**: Optimized for processing large PDF documents
- **Memory Requirements**: Significant memory needed for complex PDF parsing
- **CPU Usage**: Multi-threaded processing for better performance
- **Disk Space**: Temporary storage required during processing

## Development Notes

- **Local Development**: Use `cargo build --release` for local compilation
- **Library Requirements**: PDFium and other native libraries must be available
- **Container Size**: Moderate size container with Rust runtime and libraries
- **Build Time**: Extended build time due to Rust compilation and native dependencies
- **Mac Compatibility**: Docker build may fail on Mac due to cross-compilation issues