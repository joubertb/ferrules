# Ferrules

A Ferrule (a corruption of Latin viriola "small bracelet", under the influence of ferrum "iron"), on a pencil known as a Shoe, is any of a number of types of objects, generally used for fastening, joining, sealing, or reinforcement

## Installation

// tocome

# Roadmap

- [x] Build pdfium statically for Macos

- [x] Parse document using pdfium

  - [x] Parse char
  - [x] Merge chars into CharSpans
  - [x] Merge spans into Lines

- [ ] Layout:

  - [x] Find Layout Model and run with ORT
  - [x] Accelerate Model on ANE/GPU
  - [x] Extract Page Layout
    - [x] Preprocess pdfium image
    - [x] Postprocess tensor -> nms
    - [x] Verify labels
  - [x] Determine pages needing OCR (coverage lines/blocks)
  - [x] Merge Layout with pdfium lines
    - [x] Rescale / or / downscale line bbox/ layout bbox
    - [x] Merge intersection lines with max bbox into blocks
    - [ ] Add lines to bbox based on distance
    - [ ] Add remaining layout blocks to blocks

- [ ] Transform to HighLevel Document representation:

  - [ ] Group caption/footer blocks with image blocks/tables using minimum gap
  - [ ] Group listItems into list : Find first and merge subsequent items
  - [ ] Merge Blocks into sections
  - [ ] Get PDF Bookmarks (TOC) and reconcile detected titles with TOC
  - [ ] Process SubHeader/Titles using kmeans on line heigths to get the title_level
  - [ ] Run processors (Text, List, PageHeader )

- [ ] Render Document

  - [ ] HTML renderer
  - [ ] Markdown renderer
  - [ ] JSON renderer

- [ ] Add OCR (recognition) model

  - [ ] Use Apple vision if macOS
  - [ ] Find good recognition model (onnxtr ??)

- [ ] Batch inference on pages

  - [x] Export onnx with dynamic batch_size
  - [ ] Run layout on &[DynamicImage]

- [ ] Add tracing
- [ ] Configurable inference params: ORTProviders/ batch_size, confidence_score, NMS ..

- [ ] API

  - [ ] Unify Config for env/CLI/API
  - [ ] Dynamic batching of document(pages) to process

- [ ] Build pdfium statically for Linux
- [ ] Determine page orientation + deskew
- [ ] Optimize layout model for ANE
- [ ] ORT inference in fp16/mixed precision

## Resources:

- Apple vision text detection:

  - https://github.com/straussmaximilian/ocrmac/blob/main/ocrmac/ocrmac.py
  - https://docs.rs/objc2-vision/latest/objc2_vision/index.html
  - https://developer.apple.com/documentation/vision/recognizing-text-in-images

- Use onnxruntime IO bindings: https://ort.pyke.io/perf/io-binding
