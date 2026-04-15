# Ferrules (PDF Parser)

> Full documentation: [README.md](README.md)

## macOS Development

Auto-started by `start.speakdoc development run` as a host process (Docker incompatible on Mac).
To build: `start.speakdoc development build --ferrules` (runs `cargo build --release`).
To skip auto-start: `start.speakdoc development run --no-ferrules-autostart`.
To start manually: `cd ferrules && cargo run --release --bin ferrules-api`.
Logs to `ferrules-api.log` in ferrules directory.

## Development

- **Build**: `cargo build --release --bin ferrules-api` (must restart running process after rebuild)
- **Tests**: `cargo test --lib` for unit tests (doc tests may fail independently)
- **Formatting**: `cargo fmt` and `cargo clippy`
- **Regex**: Compile patterns at application start
- **HashMap**: Use deterministic tie-breaking for debug/release consistency
