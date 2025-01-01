use std::{
    hash::Hash,
    path::{Path, PathBuf},
};

use pdfium_render::prelude::{PdfPageRenderRotation, PdfRenderConfig, Pdfium};
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
    fn area(&self) -> f32 {
        self.height() * self.width()
    }
    fn size(&self) -> (f32, f32) {
        (self.width(), self.height())
    }

    fn merge(self, other: &Self) -> Self {
        let x0 = self.x0.min(other.x0);
        let y0 = self.y0.min(other.y0);
        let x1 = self.x1.max(other.x1);
        let y1 = self.y1.max(other.y1);
        Self { x0, y0, x1, y1 }
    }
    fn overlap_x(&self, other: &Self) -> f32 {
        f32::max(
            0f32,
            f32::min(self.x1, other.x1) - f32::max(self.x0, other.x0),
        )
    }
    fn overlap_y(&self, other: &Self) -> f32 {
        f32::max(
            0f32,
            f32::min(self.y1, other.y1) - f32::max(self.y0, other.y0),
        )
    }

    fn intersection_area(&self, other: &Self) -> f32 {
        self.overlap_x(other) * self.overlap_y(other)
    }

    fn rotate(self) -> Self {
        todo!()
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

#[derive(Debug)]
struct Page {
    id: usize,
    blocks: Vec<Block>,
    width: f32,
    height: f32,
    rotation: PdfPageRenderRotation,
}

#[derive(Debug, Default)]
struct Document {
    path: PathBuf,
    pages: Vec<Page>,
}

pub fn parse_document<P: AsRef<Path>>(
    path: P,
    password: Option<&str>,
    flatten_pdf: bool,
) -> anyhow::Result<()> {
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);
    let mut document = pdfium.load_pdf_from_file(&path, password)?;
    // TODO: deal with document embedded forms?
    let mut pages = Vec::with_capacity(document.pages().len() as usize);
    for (index, mut page) in document.pages_mut().iter().enumerate() {
        if flatten_pdf {
            page.flatten()?;
        }

        // FIXME: check that rotation is correct ??
        let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
        let page_image = page
            .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1.0))
            .map(|bitmap| bitmap.as_image())?;
        page_image.save(format!("page_{}.png", index))?;

        let page_bbox = page
            .boundaries()
            .get(pdfium_render::prelude::PdfPageBoundaryBoxType::Crop)?;

        for segment in page.text()?.segments().iter() {
            println!("{}\n\n", segment.text());
        }

        pages.push(Page {
            id: index,
            blocks: vec![],
            width: page_bbox.bounds.width().value,
            height: page_bbox.bounds.height().value,
            rotation: page_rotation,
        });
    }

    dbg!(pages);

    Ok(())
}

#[test]
fn test_parse_document() {
    let path = "/Users/amine/data/quivr/only_pdfs/0000095.pdf";

    assert!(parse_document(path, None, true).is_ok())
}
