use std::path::{Path, PathBuf};

use pdfium_render::prelude::{PdfRenderConfig, Pdfium};
pub mod detection;

#[derive(Debug)]
struct BBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl BBox {
    fn height(&self) -> f32 {
        self.y1 - self.y0
    }
    fn width(&self) -> f32 {
        self.x1 - self.x0
    }
}

#[derive(Debug)]
enum BlockType {
    Header,
    Footer,
    Text,
    Line,
    Span,
    Image,
}

#[derive(Debug)]
struct Block {
    id: usize,
    kind: BlockType,
    page_id: usize,
    bbox: BBox,
}

#[derive(Debug, Default)]
struct Page {
    id: usize,
    blocks: Vec<Block>,
    page_dim: (usize, usize),
    width: usize,
    height: usize,
}

#[derive(Debug, Default)]
struct Document {
    path: PathBuf,
    pages: Vec<Page>,
}

pub fn parse_document<P: AsRef<Path>>(path: P, password: Option<&str>) -> anyhow::Result<()> {
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);
    // let pdfium = Pdfium::new(
    //     Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")).unwrap(),
    // );
    let document = pdfium.load_pdf_from_file(&path, password)?;

    for (index, page) in document.pages().iter().enumerate() {
        let page_image = page
            .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1.0))
            .map(|bitmap| bitmap.as_image())?;

        page_image.save(format!("page_{}.png", index))?;
    }

    Ok(())
}

#[test]
fn test_parse_document() {
    let path = "/Users/amine/data/quivr/only_pdfs/0000095.pdf";

    assert!(parse_document(path, None).is_ok())
}
