use std::path::PathBuf;

use build_html::{Html, HtmlContainer, HtmlElement, HtmlPage, HtmlTag};
use regex::Regex;

use crate::blocks::{Block, BlockType};

use super::{Render, Renderer};

static LIST_BULLET_PATTERN: &str = r"(^|[\n ]|<[^>]*>)[•●○ഠ ം◦■▪▫–—-]( )";

#[derive(Debug)]
pub struct HTMLRenderer {
    root_element: HtmlElement,
    img_src_path: PathBuf,
    list_regex: Regex,
}

impl HTMLRenderer {
    pub(crate) fn new(img_src_path: PathBuf) -> Self {
        let root = HtmlElement::new(HtmlTag::Div);

        let list_regex = Regex::new(LIST_BULLET_PATTERN).unwrap();

        Self {
            root_element: root,
            img_src_path,
            list_regex,
        }
    }
    pub fn finalize(self, page_title: &str) -> String {
        HtmlPage::new()
            .with_title(page_title)
            .with_html(self.root_element)
            .to_html_string()
    }
}

impl Renderer for HTMLRenderer {
    type Ok = ();

    fn render_block(&mut self, block: &Block) -> anyhow::Result<Self::Ok> {
        match &block.kind {
            BlockType::Title(title) => {
                // Convert title level to appropriate h1-h6 tag
                let level = title.level.clamp(1, 6);
                let tag = match level {
                    1 => HtmlTag::Heading1,
                    2 => HtmlTag::Heading2,
                    3 => HtmlTag::Heading3,
                    4 => HtmlTag::Heading4,
                    5 => HtmlTag::Heading5,
                    _ => HtmlTag::Heading6,
                };
                let el = HtmlElement::new(tag)
                    .with_child(title.text.as_str().into())
                    .into();
                self.root_element.add_child(el);
            }
            BlockType::Header(text_block) => {
                let el = HtmlElement::new(HtmlTag::Header)
                    .with_child(text_block.text.as_str().into())
                    .into();
                self.root_element.add_child(el);
            }
            BlockType::Footer(text_block) => {
                let el = HtmlElement::new(HtmlTag::Footer)
                    .with_child(text_block.text.as_str().into())
                    .into();
                self.root_element.add_child(el);
            }
            BlockType::ListBlock(list) => {
                let mut ul = HtmlElement::new(HtmlTag::UnorderedList);
                for item in &list.items {
                    let clean_text = self.list_regex.replace(item, "").into_owned();
                    let li = HtmlElement::new(HtmlTag::ListElement)
                        .with_child(clean_text.as_str().into())
                        .into();
                    ul.add_child(li);
                }
                self.root_element.add_child(ul.into());
            }
            BlockType::TextBlock(text_block) => {
                let el = HtmlElement::new(HtmlTag::ParagraphText)
                    .with_child(text_block.text.as_str().into())
                    .into();
                self.root_element.add_child(el);
            }
            BlockType::Image(image_block) => {
                let mut figure = HtmlElement::new(HtmlTag::Figure);
                let img_src = self
                    .img_src_path
                    .join(image_block.path())
                    .to_str()
                    .unwrap()
                    .to_owned();
                let img = HtmlElement::new(HtmlTag::Image).with_image(img_src, "");
                figure.add_child(img.into());

                if let Some(caption) = &image_block.caption {
                    let figcaption = HtmlElement::new(HtmlTag::Figcaption)
                        .with_child(caption.as_str().into())
                        .into();
                    figure.add_child(figcaption);
                }

                self.root_element.add_child(figure.into());
            }
            _ => {
                eprintln!("not implemented yet")
            }
        }
        Ok(())
    }
}

#[tracing::instrument(skip_all)]
pub fn to_html<R: Render>(
    blocks: R,
    page_title: &str,
    img_src_path: PathBuf,
) -> anyhow::Result<String> {
    let mut html_renderer = HTMLRenderer::new(img_src_path);
    blocks.render(&mut html_renderer)?;
    Ok(html_renderer.finalize(page_title))
}
