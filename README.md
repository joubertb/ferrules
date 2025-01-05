# Ferrules

A Ferrule (a corruption of Latin viriola "small bracelet", under the influence of ferrum "iron"), on a pencil known as a Shoe, is any of a number of types of objects, generally used for fastening, joining, sealing, or reinforcement

## Installation

**Marker Process**

1. **Document Builder:**

- Utilizes PDF provider(pdfium)
  - Implements pdftext_extraction, which utilizes pdftext.dictionnary_output to generate ProviderPageLines.
  - Constructs self.page_bboxes and self.page_lines.
- Constructs the `Document`:

  - Constructs initial PagesGroup: (page_id, highlowres_image, polygon(page_bbox) ) from PDFium.
  - Retrieves the layout using `batch_layout_detection`.
  - Adds layout blocks to pages: bbox to polygon -> page.add_structure(block).
  - Merges layout blocks of layout:
    - Pages layout coverage: Intersection between layout_bbox and provider_bbox to verify + check when the model sometimes indicates a single block of text on a page when it is blank.
    - If dont need OCR:
      - Merge Provider.page_lines with

- OCR Builder

2. **Structure Builder:**: takes document -> StructuredDocument

- For each page:
  - group_caption_blocks(page)
  - group_lists(page)

3. **Processor**: Applies [Processors] to StructuredDocument elements
4. **Render Document:**

## State Machine

# TODO

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
    - [ ] Verify labels
  - [ ] Determine pages needing OCR (coverage lines/blocks)
  - [ ] Merge Layout with pdfium lines
  - [ ] Build ParsedDocument result

- [ ] Transform to HighLevel:

  - [ ] Group_caption_blocks(page)
  - [ ] Group_lists(page)
  - [ ] Build Document TOC
  - [ ] Run processors ( Text, List, PageHeader ) .

- [ ] Render Document

  - [ ] HTML renderer
  - [ ] Markdown renderer

- [ ] API

  - [ ] Batch inference on pages
  - [ ] Dynamic batching of document to process
  - [ ]

- [ ] Build pdfium statically for Linux
- [ ] Configurable inference params: ORTProviders/ batch_size, confidence_score, NMS ..
- [ ] Add OCR (recognition) model
- [ ] Determine page orientation + deskew
