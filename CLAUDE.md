# Ferrules (PDF Parser)

Rust-based PDF parsing engine that converts PDFs into structured JSON for TTS processing.

## Service Components

| Component | Description |
|-----------|-------------|
| `ferrules-core/` | Core parsing library, font correction, script detection |
| `ferrules-api/` | HTTP API server (port 3002) |
| `ferrules-cli/` | CLI for standalone PDF processing |
| `ferrules/` | Main binary and orchestration |

### API Endpoints

- **POST /parse** — Upload PDF, receive structured JSON
- **GET /health** — Health check
- **GET /info** — Service info

### macOS Development

Must be started manually (Docker incompatible on Mac):
```bash
cd ferrules && cargo run --release --bin ferrules-api
# Or use pre-built binary
```
Logs to `ferrules-api.log` in ferrules directory.

## Environment Variables

### FERRULES_DEBUG_OUTPUT

Controls debug output for troubleshooting PDF parsing.

- `none` — No debug output (default)
- `stderr` — Debug output to container logs
- `file` — Saved to `/tmp/ferrules-debug/{doc_name}-debug.txt`
- `both` — Both stderr and file

Worker downloads debug files to `<document_id>/logs/raw-debug.txt.gz`.

### FERRULES_DEBUG_DIR

Custom debug file location (default: `/tmp/ferrules-debug`)

## Font Correction System

### Architecture

A **unified correction engine** dynamically analyzes PDF fonts at runtime — no config files or hardcoded patterns.

**Pipeline:**
1. **Universal Font Corrector** (`font_analysis/universal_corrector.rs`) — Analyzes PDF fonts via glyph names, ToUnicode CMaps, and Adobe Glyph List. Generates mappings for mathematical Unicode ranges (U+1D400-U+1D7FF). Detects mathematical fonts (CMMI, CMSY, CMEX, CambriaMath, etc.)
2. **Character Corrector** (`correction/character.rs`) — Fixes UTF-8 corruption, removes control characters
3. **Dictionary Corrector** (`font_analysis/dictionary.rs`) — Validates words using 52K+ embedded English dictionary. Skips mid-word parentheses (extraction boundary artifacts)

**Key files:**
- `font_analysis/adobe_glyph_list.rs` — Standard glyph name → Unicode mappings
- `correction/engine.rs` — Orchestrates all correction strategies
- `correction/dictionaries/` — Embedded word lists

**Performance:** ~2.7s for 7-page paper, <50MB overhead, linear scaling.

## Mathematical Content Detection (`has_math`)

Ferrules flags text blocks and list items containing math with `has_math: true` in JSON output, enabling the worker's LLM-based text formula translation pipeline.

### Detection Pipeline

`CharSpan` → `Element` → `TextBlock` (OR propagation at each level)

**CharSpan** (`entities.rs`): `has_math_font` set when character comes from a mathematical font OR is a mathematical Unicode symbol (U+1D400-U+1D7FF, plus curated operators like ∀, ∑, ∫, ≠, ⊂).

**Element** (`entities.rs`): `has_math` set if any span in any line has `has_math_font`.

**TextBlock** (`blocks.rs`, `merge.rs`): `has_math` propagates through merges. Serialized only when true (`skip_serializing_if`).

**ListItem** (`blocks.rs`, `merge.rs`): `has_math` propagates from element to each list item. Serialized only when true.

### TeX CMMI/CMSY Font Encoding

TeX math fonts encode Greek letters as control characters (0x00-0x21). `tex_math_encoding()` in `entities.rs` maps these to Unicode Greek (e.g., 0x0F → ε, 0x12 → θ).

CMMI position 0x20 maps to ψ (psi), but 0x20 is also ASCII space — so inter-glyph spaces in CMMI fonts become literal ψ characters. Worker handles this with `replace_cmmi_psi_spaces()`.

## Subscript/Superscript Detection

**Location:** `ferrules-core/src/modtext/script_notation.rs`

Multi-layered composite scoring (99.89% accuracy):
- Vertical offset from baseline (75% weight)
- Font size shrinkage (25% weight)

**Dual-layer processing:**
1. Sequential analysis — character-by-character for simple text
2. Clustering analysis — Y-position grouping for complex math notation

**Features:**
- Local mode-based baseline using ±3pt Y-range
- Footnote reference detection: emits `<foot>` tags (vs `<sup>` for math superscripts)
- Radical sign (√) excluded from script detection
- Works with corrupted fonts and inconsistent baselines

### Footnote References (`<foot>` tag)

Script notation distinguishes footnote references from math superscripts:
- `<foot>1</foot>` — definitively a footnote (worker strips unconditionally)
- `<sup>2</sup>` — math superscript (worker uses heuristics, skips `has_math` blocks)

## JSON Output Format

### Text Block Fields

- `text` — Processed text content
- `fertext` — Original ferrules text (preserved for alignment)
- `has_math` — Mathematical content flag (only present when true)
- `char_spans` — Character bounding boxes for sentence highlighting
- `sentence_ends` — Sentence boundary indices

### Block Types

TextBlock, Title, ListBlock, TableBlock, Caption, Formula, Footer, Header

### Formula Block Handling

Formula blocks (ONNX layout model) get raw text + cropped PNG images (`formula_{block_id}.png`). Text blocks get full processing (hyphen removal, font corrections, script detection).

## Troubleshooting

**API server not responding (macOS):**
Start manually: `cd ferrules && cargo run --release --bin ferrules-api`

**Slow processing:**
Linear with document size. Debug with `RUST_LOG=debug` and `FERRULES_DEBUG_OUTPUT=file`.

**Incorrect corrections:**
Enable debug output to examine font analysis details.

**Known limitations:**
- Cannot correct fonts with entirely random character mappings
- Some PDFs lack sufficient glyph information for correction

## Development Notes

- **Build**: `cargo build --release --bin ferrules-api` (must restart running process after rebuild)
- **Tests**: `cargo test --lib` for unit tests (doc tests may fail independently)
- **Formatting**: `cargo fmt` and `cargo clippy`
- **Regex**: Compile patterns at application start
- **HashMap**: Use deterministic tie-breaking for debug/release consistency

## File Organization

```
ferrules/
├── ferrules-core/           # Core parsing library
│   └── src/
│       ├── blocks.rs        # Block assembly, has_math propagation
│       ├── entities.rs      # CharSpan, Element, math detection
│       ├── font_analysis/   # Universal font corrector, dictionary
│       ├── correction/      # Character/text correction
│       ├── modtext/         # Text modification, script notation
│       ├── parse/           # PDF parsing, element merging
│       └── layout/          # ONNX layout model
├── ferrules-api/            # HTTP API server
├── ferrules-cli/            # CLI tool
├── ferrules/                # Main binary
├── font/                    # Required font files
├── libs/                    # PDF processing libraries
├── models/                  # ML models (layout detection)
├── Cargo.toml               # Workspace configuration
├── Dockerfile               # Container build (Linux)
└── ferrules-api.log         # API server log file
```

## See Also

- [../CLAUDE.md](../CLAUDE.md) - Project overview
- [../worker/CLAUDE.md](../worker/CLAUDE.md) - Worker service that calls Ferrules
