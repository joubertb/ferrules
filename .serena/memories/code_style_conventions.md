# Ferrules Code Style and Conventions

## Rust Toolchain
- **Version**: Nightly Rust (specified in `rust-toolchain.toml`)
- **Features**: Uses `#![feature(portable_simd)]` and other nightly features
- **Edition**: 2021 edition

## Formatting Standards
Configuration in `.rustfmt.toml`:
```toml
blank_lines_upper_bound = 1
blank_lines_lower_bound = 0
```
- Use `cargo fmt` to format code
- Empty lines should be truly empty (no whitespace)
- Maximum of 1 blank line between items

## Code Organization
### Module Structure
- **Public APIs**: Clearly documented with rustdoc comments
- **Internal modules**: Use `pub(crate)` for internal visibility
- **Feature flags**: Use conditional compilation for optional features

### Documentation Style
```rust
//! # Module Documentation
//!
//! Brief description of module purpose
//!
//! ## Key Features
//! - Feature 1
//! - Feature 2

/// Function documentation
/// 
/// # Arguments
/// - `arg1` - Description
/// 
/// # Returns
/// Description of return value
pub fn example_function() {}
```

## Naming Conventions
- **Types**: PascalCase (`ParseNativeRequest`, `FontCorruptionMap`)
- **Functions**: snake_case (`parse_text_spans`, `analyze_font_corruption`)
- **Constants**: SCREAMING_SNAKE_CASE (`MAX_CONCURRENT_NATIVE_REQS`)
- **Modules**: snake_case (`font_analysis`, `pdf_preprocessor`)

## Error Handling
- Use `anyhow::Result<T>` for most error returns
- Use `anyhow::Context` for error context: `.context("operation description")?`
- Prefer early returns with `?` operator
- Use `eprintln!` for debug/error output to stderr

## Dependencies and Features
### Workspace Dependencies
Shared dependencies defined in workspace `Cargo.toml`:
- `anyhow` - Error handling
- `serde` - Serialization 
- `tokio` - Async runtime
- `tracing` - Logging
- `uuid` - Unique identifiers

### Feature Flags
- Use feature flags for optional functionality (e.g., `correction-engine`)
- Conditional compilation with `#[cfg(feature = "feature-name")]`

## Async/Await Patterns
- Use `tokio` for async runtime
- Prefer `async/await` over manual futures
- Use appropriate async primitives (channels, mutexes)

## Memory Management
- Use `Arc<>` for shared ownership of large data
- Use `Box<>` for heap allocation when needed
- Prefer borrowing (`&T`) over ownership when possible
- Use `Vec<>` for dynamic arrays

## Testing Conventions
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Test implementation
    }
}
```

## Performance Considerations
- Enable optimizations in release profile
- Use `#[instrument]` for tracing critical functions
- Profile memory usage for large document processing
- Consider parallel processing for CPU-intensive operations

## Platform-Specific Code
- Use `#[cfg(target_os = "macos")]` for macOS-specific features
- Graceful fallbacks when platform features unavailable
- Docker compatibility considerations for cross-platform deployment