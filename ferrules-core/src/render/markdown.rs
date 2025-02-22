use std::path::PathBuf;

use html2md::parse_html;

use crate::blocks::Block;

use super::{html::HTMLRenderer, Render, Renderer};

#[derive(Debug)]
pub struct MarkdownRender {
    html_renderer: HTMLRenderer,
}

impl MarkdownRender {
    pub(crate) fn new(img_src_path: PathBuf) -> Self {
        let html_renderer = HTMLRenderer::new(img_src_path);
        Self { html_renderer }
    }
    pub fn finalize(self, page_title: &str) -> String {
        let page = self.html_renderer.finalize(page_title);
        parse_html(&page)
    }
}

impl Renderer for MarkdownRender {
    type Ok = ();

    fn render_block(&mut self, block: &Block) -> anyhow::Result<Self::Ok> {
        self.html_renderer.render_block(block)
    }
}

#[tracing::instrument(skip_all)]
pub fn to_markdown<R: Render>(
    blocks: R,
    page_title: &str,
    img_src_path: PathBuf,
) -> anyhow::Result<String> {
    let mut html_renderer = MarkdownRender::new(img_src_path);
    blocks.render(&mut html_renderer)?;
    Ok(html_renderer.finalize(page_title))
}
