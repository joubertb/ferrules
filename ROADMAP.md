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

  - [ ] Move header element to the top of the page
  - [ ] Merge text block based on gap distance
  - [x] Group listItems into list : Find first and merge subsequent items
  - [x] Group caption/footer blocks with image blocks
  - [ ] Group Page header / Page footer
  - [ ] Process SubHeader/Titles using kmeans on line heigths to get the title_level
  - [ ] Merge captions with tables
  - [ ] Run post processors (Text, List, PageHeader )
  - [ ] Get PDF Bookmarks (TOC) and reconcile detected titles with TOC

- [ ] Render Document

  - [ ] HTML renderer
  - [ ] Markdown renderer
  - [ ] JSON renderer
    - [ ] Crop images and save in directory if `--save_image` flag

- [x] Create CLI ferrules
- [ ] Add tracing
- [ ] `eyre` | `thiserror` for custom errosk
- [ ] Configurable inference params: ORTProviders/ batch_size, confidence_score, NMS ..

- [ ] OCR: Find good recognition model (onnxtr ??)

- [ ] Batch inference on pages (TODO -> )

  - [x] Export onnx with dynamic batch_size
  - [ ] Run layout on &[DynamicImage]
  - Explored batching on onnxruntime on coreml isn't faster for some weird reason (probably batch dim)
    - [ ] check on nvidia-gpu if batching is better

- [ ] API

  - [ ] Unify Config for env/CLI/API
  - [ ] Dynamic batching of document(pages) to process

- [ ] Change NMS with more robust for nested bbox of the same type
- [ ] Open document mmap and share range of page between threads
- [ ] Build pdfium statically for Linux
- [ ] Determine page orientation + deskew
- [ ] Optimize layout model for ANE
- [ ] ORT inference in fp16/mixed precision
- [ ] Move to other yolo versions: yolov11s seems better with less params [yolo-doclaynet](https://github.com/ppaanngggg/yolo-doclaynet)
- [ ] Explore arena allocators (one per page)
