use std::path::PathBuf;
pub mod detection;

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
    page_id: usize,
    kind: BlockType,
    pos: (usize, usize),
    width: usize,
    height: usize,
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
