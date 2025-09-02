use crate::correction;
use crate::entities::{BBox, Element, ElementType, PageID};
use anyhow::bail;
use serde::{Deserialize, Serialize};

pub type TitleLevel = u8;

/// Apply word-level font corrections to assembled text
fn apply_word_corrections(text: &mut String) {
    // Debug: Log what text we're working with at the block level
    if text.contains("n<sub>j</sub> i") || text.contains("<formula>") {
        eprintln!("🔍 BLOCK DEBUG: apply_word_corrections called with text length {} containing target patterns", text.len());
        eprintln!(
            "🔍 BLOCK DEBUG: Text preview: {}",
            text.chars().take(200).collect::<String>()
        );
    }

    let corrected = correction::correct_assembled_text(text);
    if corrected != *text {
        *text = corrected;
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ImageBlock {
    pub(crate) id: usize,
    pub(crate) caption: Option<String>,
}

impl ImageBlock {
    pub(crate) fn path(&self) -> String {
        format!("img_{}.png", self.id)
    }
}

impl TextBlock {
    /// Get the text content of the block
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Title {
    /// Get the title text
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the title level
    pub fn level(&self) -> TitleLevel {
        self.level
    }
}

impl List {
    /// Get the list items
    pub fn items(&self) -> &[String] {
        &self.items
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TextBlock {
    pub(crate) text: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct List {
    pub(crate) items: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Title {
    pub level: TitleLevel,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "block_type")]
pub enum BlockType {
    Header(TextBlock),
    Footer(TextBlock),
    Title(Title),
    ListBlock(List),
    TextBlock(TextBlock),
    Image(ImageBlock),
    Table,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Block {
    pub id: usize,
    pub kind: BlockType,
    pub pages_id: Vec<PageID>,
    pub bbox: BBox,
}

impl Block {
    pub(crate) fn merge(&mut self, element: Element) -> anyhow::Result<()> {
        match &mut self.kind {
            BlockType::TextBlock(text) => {
                if let ElementType::Text = &element.kind {
                    self.bbox.merge(&element.bbox);
                    text.text.push('\n');
                    text.text.push_str(&element.text_block.text);

                    // Apply word-level corrections to assembled text
                    apply_word_corrections(&mut text.text);

                    // add page_id
                    Ok(())
                } else {
                    bail!("can't merge element in textblock")
                }
            }
            BlockType::ListBlock(list) => {
                if let ElementType::ListItem = &element.kind {
                    self.bbox.merge(&element.bbox);
                    let mut txt = element.text_block.text.trim().to_string();

                    // Apply word-level corrections to list item text
                    apply_word_corrections(&mut txt);

                    list.items.push(txt);
                    Ok(())
                } else {
                    bail!("can't merge element in Listblock")
                }
            }
            BlockType::Header(header) => {
                if let ElementType::Header = &element.kind {
                    self.bbox.merge(&element.bbox);
                    header.text.push_str(&element.text_block.text);

                    // Apply word-level corrections to header text
                    apply_word_corrections(&mut header.text);

                    Ok(())
                } else {
                    bail!("can't merge element in Header")
                }
            }
            BlockType::Footer(footer) => {
                if let ElementType::Footer = &element.kind {
                    self.bbox.merge(&element.bbox);
                    footer.text.push_str(&element.text_block.text);

                    // Apply word-level corrections to footer text
                    apply_word_corrections(&mut footer.text);

                    Ok(())
                } else {
                    bail!("can't merge element in Footer")
                }
            }
            BlockType::Title(_title) => todo!(),
            BlockType::Image(_image_block) => todo!(),
            BlockType::Table => todo!(),
        }
    }

    pub(crate) fn label(&self) -> &str {
        match self.kind {
            BlockType::Header(_) => "HEADER",
            BlockType::Footer(_) => "FOOTER",
            BlockType::TextBlock(_) => "TEXT",
            BlockType::Title(_) => "TITLE",
            BlockType::ListBlock(_) => "LIST",
            BlockType::Image(_) => "IMAGE",
            BlockType::Table => "TABLE",
        }
    }
}
