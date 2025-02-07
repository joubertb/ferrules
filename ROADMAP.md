## Roadmap

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
  - [x] OCR -> Use Apple vision on macOS target_os
  - [x] Merge Layout with pdfium lines
    - [x] Rescale / or / downscale line bbox/ layout bbox
    - [x] Merge intersection lines (from pdfium and OCR) with max bbox into blocks
    - [x] Add lines to bbox based on distance
    - [x] Add remaining layout blocks to blocks based on position

- [ ] Document merge:

  - [x] Group listItems into list : Find first and merge subsequent items
  - [x] Group caption/footer blocks with image blocks
  - [x] Group Page header / Page footer
  - [x] Move header element to the top of the page
  - [ ] Merge text block based on gap distance
  - [ ] Process SubHeader/Titles using kmeans on line heigths to get the title_level
  - [ ] Run Block processors (Text, List, PageHeader )
  - [ ] Get PDF Bookmarks (TOC) and reconcile detected titles with TOC

- Tables

  - [ ] Extract tables
  - [ ] Group captions with tables

- [ ] Render Document

  - [x] JSON renderer
    - [x] Crop images and save in directory if `--save_image` flag
  - [ ] HTML renderer
  - [ ] Markdown renderer (based on html renderer)

- CLI ferrules

  - [x] Add variables
  - [x] Add debug flag
  - [x] Add range flag
  - [x] Configure hyperparams/execution providers
  - [ ] Add export format -> JSON (default) or markdown

- [x] Build pdfium statically for Linux
- [x] Change NMS algorithm to more robust one
- [x] Add tracing to core
- [x] Configurable inference params: ORTProviders/ batch_size, confidence_score, NMS ..
- [ ] `eyre` | `thiserror` for custom errosk

- [ ] OCR: Find good recognition model for (target_os != macos)

- Inference:

  - [x] Export onnx layout model with dynamic `batch_size`
  - [x] Run layout on &[DynamicImage]
  - [x] Implement Linux/CUDA inference (EP)
  - [ ] Batch inference on pages (For Nvidia GPU, batch_size on macos didn't yield good results)

- API

  - [x] Full OTEL + sentry tracing in API
  - [x] Clap API + Env variables
  - [ ] Unify Config for env/CLI/API
  - [ ] Dynamic batching of document(pages) to process

- Optim
  - [ ] Determine page orientation + deskew
  - [ ] Optimize layout model for ANE -> Look at changing shapes and operators to maximize ANE perf
  - [ ] ORT inference in fp16/mixed precision
  - [ ] Move to other Yolo versions: yolov11s seems better with less params [yolo-doclaynet](https://github.com/ppaanngggg/yolo-doclaynet)
  - [ ] Explore arena allocators (one per page)
  - [ ] String -> CowStr
