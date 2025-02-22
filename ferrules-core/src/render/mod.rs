use anyhow::Context;

use crate::{blocks::Block, entities::ParsedDocument};

pub mod html;
pub mod markdown;

pub trait Render {
    type Output;
    fn render<R: Renderer>(&self, renderer: &mut R) -> anyhow::Result<Self::Output>;
}

pub trait Renderer {
    type Ok;

    fn render_block(&mut self, block: &Block) -> anyhow::Result<Self::Ok>;
}

impl Render for &ParsedDocument {
    type Output = ();

    fn render<R: Renderer>(&self, renderer: &mut R) -> anyhow::Result<()> {
        for block in &self.blocks {
            renderer.render_block(block).context("can't render block")?;
        }
        Ok(())
    }
}
