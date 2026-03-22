use std::path::PathBuf;

use build_html::{Html, HtmlChild, HtmlContainer, HtmlElement, HtmlPage, HtmlTag};
use lazy_static::lazy_static;
use regex::Regex;

use crate::blocks::{Block, BlockType};

use super::{Render, Renderer};

static LIST_BULLET_PATTERN: &str = r"(^|[\n ]|<[^>]*>)[•●○ഠ ം◦■▪▫–—-]( )";

lazy_static! {
    /// Pre-compiled regex for list bullet pattern detection
    static ref LIST_BULLET_REGEX: Regex = Regex::new(LIST_BULLET_PATTERN).unwrap();
}

#[derive(Debug)]
pub struct HTMLRenderer {
    root_element: HtmlElement,
    img_src_path: Option<PathBuf>,
}

impl HTMLRenderer {
    pub(crate) fn new(img_src_path: Option<PathBuf>) -> Self {
        let root = HtmlElement::new(HtmlTag::Div);

        Self {
            root_element: root,
            img_src_path,
        }
    }
    pub fn finalize(self, page_title: &str) -> String {
        HtmlPage::new()
            .with_title(page_title)
            .with_html(self.root_element)
            .to_html_string()
    }

    fn render_block_to_container(
        block: &Block,
        container: &mut HtmlElement,
        img_src_path: Option<&PathBuf>,
        list_regex: &Regex,
    ) -> anyhow::Result<()> {
        match &block.kind {
            BlockType::Title(title) => {
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
                container.add_child(el);
            }
            BlockType::Header(text_block) => {
                let el = HtmlElement::new(HtmlTag::Header)
                    .with_child(text_block.text.as_str().into())
                    .into();
                container.add_child(el);
            }
            BlockType::Footer(text_block) => {
                let el = HtmlElement::new(HtmlTag::Footer)
                    .with_child(text_block.text.as_str().into())
                    .into();
                container.add_child(el);
            }
            BlockType::ListBlock(list) => {
                let mut ul = HtmlElement::new(HtmlTag::UnorderedList);
                for item in &list.items {
                    let clean_text = list_regex.replace(&item.text, "").into_owned();
                    let li = HtmlElement::new(HtmlTag::ListElement)
                        .with_child(clean_text.as_str().into())
                        .into();
                    ul.add_child(li);
                }
                container.add_child(ul.into());
            }
            BlockType::TextBlock(text_block) => {
                let el = HtmlElement::new(HtmlTag::ParagraphText)
                    .with_child(text_block.text.as_str().into())
                    .into();
                container.add_child(el);
            }
            BlockType::Image(image_block) => {
                if let Some(img_src_path) = img_src_path {
                    let mut figure = HtmlElement::new(HtmlTag::Figure);
                    let img_src = img_src_path
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

                    container.add_child(figure.into());
                }
            }
            BlockType::Formula(formula) => {
                let el = HtmlElement::new(HtmlTag::ParagraphText)
                    .with_child(formula.text.as_str().into())
                    .into();
                container.add_child(el);
            }
            BlockType::Figure(figure) => {
                if let Some(img_src_path) = img_src_path {
                    let mut figure_el = HtmlElement::new(HtmlTag::Figure);
                    let img_src = img_src_path
                        .join(format!("img_{}.png", figure.id))
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let img = HtmlElement::new(HtmlTag::Image).with_image(img_src, "");
                    figure_el.add_child(img.into());

                    if let Some(caption) = &figure.caption {
                        let figcaption = HtmlElement::new(HtmlTag::Figcaption)
                            .with_child(caption.as_str().into())
                            .into();
                        figure_el.add_child(figcaption);
                    }

                    container.add_child(figure_el.into());
                }
            }
            BlockType::Table(table) => {
                let mut table_html = String::from("<table>");
                if let Some(caption) = &table.caption {
                    table_html.push_str(&format!("<caption>{}</caption>", caption));
                }
                for row in &table.rows {
                    table_html.push_str("<tr>");
                    for cell in &row.cells {
                        let tag = if row.is_header { "th" } else { "td" };
                        table_html.push_str(&format!("<{}", tag));
                        if cell.col_span > 1 {
                            table_html.push_str(&format!(" colspan=\"{}\"", cell.col_span));
                        }
                        if cell.row_span > 1 {
                            table_html.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
                        }
                        table_html.push('>');

                        if !cell.text.is_empty() {
                            table_html.push_str(&cell.text);
                        }

                        table_html.push_str(&format!("</{}>", tag));
                    }
                    table_html.push_str("</tr>");
                }
                table_html.push_str("</table>");
                container.add_child(HtmlChild::Raw(table_html));
            }
        }
        Ok(())
    }
}

impl Renderer for HTMLRenderer {
    type Ok = ();

    fn render_block(&mut self, block: &Block) -> anyhow::Result<Self::Ok> {
        Self::render_block_to_container(
            block,
            &mut self.root_element,
            self.img_src_path.as_ref(),
            &LIST_BULLET_REGEX,
        )
    }
}

#[tracing::instrument(skip_all)]
pub fn to_html<R: Render>(
    blocks: R,
    page_title: &str,
    img_src_path: Option<PathBuf>,
) -> anyhow::Result<String> {
    let mut html_renderer = HTMLRenderer::new(img_src_path);
    blocks.render(&mut html_renderer)?;
    Ok(html_renderer.finalize(page_title))
}
