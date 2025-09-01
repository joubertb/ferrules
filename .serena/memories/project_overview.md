# Ferrules Project Overview

## Purpose
Ferrules is a modern, high-performance PDF document parsing library written in Rust, designed to generate LLM-ready documents efficiently. It's a fast alternative to Python-based solutions like `unstructured`, providing robust PDF parsing and text extraction capabilities.

## Key Features
- **📄 PDF Parsing**: Uses pdfium2 for document parsing with advanced text extraction
- **🔄 Document Transformation**: Intelligent grouping of captions, footers, lists, and document structure
- **🖨️ Multi-format Output**: HTML, Markdown, and JSON rendering options
- **⚡ High Performance**: Built with Rust for maximum speed and memory safety
- **🛠️ Dual Interface**: Both CLI and HTTP API server
- **🧠 ML-Powered**: Uses machine learning for layout analysis and document understanding
- **🔧 Font Correction System**: Advanced font corruption detection and correction for mathematical PDFs

## Main Use Case
Originally built as part of the SpeakDoc application for converting PDFs to audio, Ferrules processes PDFs and converts them to structured JSON format that can be consumed by text-to-speech systems. It handles complex mathematical documents with corrupted fonts, which was a key development focus.

## Architecture
- **ferrules-core**: Core library containing PDF parsing algorithms, font correction, and text processing
- **ferrules-api**: HTTP API server for PDF processing via REST endpoints
- **ferrules-cli**: Command-line interface for standalone PDF processing
- **ferrules**: Main binary and orchestration logic

## Technology Stack
- **Language**: Rust (nightly toolchain)
- **PDF Processing**: pdfium2 library
- **ML Framework**: ONNX Runtime (ORT) for layout analysis 
- **OCR**: Apple Vision on macOS
- **Hardware Acceleration**: CoreML (macOS), CUDA/TensorRT (Linux)
- **Web Framework**: Axum (for API server)
- **CLI**: Clap for command-line parsing
- **Async Runtime**: Tokio
- **Serialization**: Serde (JSON output)
- **Tracing**: Comprehensive logging and monitoring

## Platform Support
- **macOS**: Full native support with CoreML acceleration
- **Linux**: Full support with GPU acceleration (CUDA, TensorRT) 
- **Docker**: Multi-platform containers available

## Key Differentiators
- **Zero Python Dependencies**: Pure Rust implementation
- **Hardware Acceleration**: Leverages Apple Neural Engine, CUDA, etc.
- **Font Corruption Handling**: Specialized system for mathematical PDF font issues
- **Production Ready**: Designed for high-performance server deployments