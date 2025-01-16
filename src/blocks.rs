use crate::entities::{BBox, Element, ElementType, PageID};
use anyhow::bail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ImageBlock {
    pub(crate) caption: Option<String>,
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
    level: u8,
    text: String,
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
    pub(crate) fn merge(&mut self, element: &Element) -> anyhow::Result<()> {
        match &mut self.kind {
            BlockType::TextBlock(text) => {
                if let ElementType::Text(el_text) = &element.kind {
                    self.bbox.merge(&element.bbox);
                    text.text.push('\n');
                    text.text.push_str(&el_text.text);

                    // add page_id
                    Ok(())
                } else {
                    bail!("can't merge element in textblock")
                }
            }
            BlockType::ListBlock(list) => {
                if let ElementType::ListItem(el_text) = &element.kind {
                    self.bbox.merge(&element.bbox);
                    list.items.push(el_text.text.to_owned());
                    Ok(())
                } else {
                    bail!("can't merge element in Listblock")
                }
            }
            BlockType::Header(_) => todo!(),
            BlockType::Footer(_text_block) => todo!(),
            BlockType::Title(_title) => todo!(),
            BlockType::Image(_image_block) => todo!(),
            BlockType::Table => todo!(),
        }
    }
}
