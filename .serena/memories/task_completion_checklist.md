# Task Completion Checklist

## Before Submitting Code Changes

### 1. Code Quality Checks
```bash
# Format code according to project standards
cargo fmt

# Run clippy for linting
cargo clippy --all-features -- -D warnings

# Ensure code compiles without warnings
cargo build --release
```

### 2. Testing Requirements
```bash
# Run all unit tests
cargo test

# Run tests for modified crates only
cargo test -p ferrules-core  # if core changes
cargo test -p ferrules-api   # if API changes

# Test with real documents if applicable
cargo run --bin ferrules-cli mathbert.pdf --output-dir test-output
```

### 3. Integration Testing
For font correction or parsing changes:
```bash
# Test font correction system specifically
RUST_LOG=debug cargo run --bin ferrules-cli mathbert.pdf --debug

# Verify corrected PDF output
pdffonts mathbert-corrected.pdf  # Check if ToUnicode CMaps added

# Compare before/after parsing results
diff mathbert-before.json mathbert-after.json
```

### 4. Documentation Updates
- Update rustdoc comments for public APIs
- Update CLAUDE.md if architecture changes
- Add inline comments for complex logic

### 5. Performance Verification
For performance-critical changes:
```bash
# Release build performance test
time ./target/release/ferrules-cli large-document.pdf

# Memory usage check
RUST_LOG=info cargo run --bin ferrules-cli document.pdf 2>&1 | grep "memory\|MB\|processing"
```

### 6. Platform Compatibility
- Test on macOS if available
- Ensure Docker builds work:
  ```bash
  docker build -t ferrules-test .
  docker run --rm ferrules-test ferrules-cli --version
  ```

## Common Validation Steps

### Font Correction Changes
1. Test with mathematical PDFs (mathbert.pdf)
2. Verify parentheses/brackets render correctly
3. Check correction confidence scores in logs
4. Validate ToUnicode CMap generation

### API Changes
1. Test API endpoints manually or with curl
2. Verify JSON output format compatibility
3. Check error handling and status codes
4. Test file upload limits and timeouts

### CLI Changes
1. Test help output: `cargo run --bin ferrules-cli -- --help`
2. Verify all command-line options work
3. Test error scenarios and exit codes
4. Check progress indicators and output formatting

## Pre-Commit Best Practices
- Commit message format: Clear, descriptive summary
- Small, focused commits rather than large changes
- Test locally before committing
- Check that no secrets or test files are committed

## Deployment Checklist
For production deployments:
1. Full release build: `cargo build --release --all-features`
2. Container build verification
3. Memory leak testing for long-running processes
4. Performance benchmarking with realistic workloads