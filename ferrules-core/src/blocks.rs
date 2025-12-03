use crate::entities::{BBox, Element, ElementType, PageID, SerializableCharSpan};
use crate::font_analysis as correction;
use anyhow::bail;
use serde::{Deserialize, Serialize};

pub type TitleLevel = u8;

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

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct FigureBlock {
    pub(crate) id: usize,
    pub(crate) embedded_texts: Vec<String>,
    pub(crate) image_bbox: Option<BBox>,
    pub(crate) caption: Option<String>,
}

impl FigureBlock {
    pub(crate) fn path(&self) -> String {
        format!("fig_{}.png", self.id)
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
    pub fn items(&self) -> &[ListItem] {
        &self.items
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TextBlock {
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fertext: Option<String>,
    /// Character spans with bounding boxes for sentence highlighting
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) char_spans: Vec<SerializableCharSpan>,
    /// Sentence end positions (character indices) for precise bbox computation
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) sentence_ends: Vec<usize>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ListItem {
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fertext: Option<String>,
    /// Character spans with bounding boxes for sentence highlighting
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) char_spans: Vec<SerializableCharSpan>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct List {
    pub(crate) items: Vec<ListItem>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Title {
    pub level: TitleLevel,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fertext: Option<String>,
    /// Character spans with bounding boxes for sentence highlighting
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub char_spans: Vec<SerializableCharSpan>,
    /// Sentence end positions (character indices) for precise bbox computation
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sentence_ends: Vec<usize>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct FormulaBlock {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula_img: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "block_type")]
pub enum BlockType {
    Header(TextBlock),
    Footer(TextBlock),
    Title(Title),
    ListBlock(List),
    TextBlock(TextBlock),
    Formula(FormulaBlock),
    Image(ImageBlock),
    Figure(FigureBlock),
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

                    // Store original text if not already stored
                    if text.fertext.is_none() {
                        text.fertext = Some(text.text.clone());
                    }

                    // Get fertext length for char_span offset (char_spans index into original text)
                    let fertext_len = text
                        .fertext
                        .as_ref()
                        .map(|f| f.chars().count())
                        .unwrap_or_else(|| text.text.chars().count());

                    text.text.push('\n');
                    text.text.push_str(&element.text_block.text);

                    // Merge fertext - element.text_block.text is original text before corrections
                    if let Some(ref mut fertext) = text.fertext {
                        fertext.push('\n');
                        fertext.push_str(&element.text_block.text);
                    }

                    // Apply word-level corrections to assembled text (not fertext)
                    correction::apply_word_corrections(&mut text.text);

                    // Collect char_spans from element with adjusted offsets
                    // Use fertext length since char_spans index into original text
                    let offset = fertext_len + 1;
                    for span in element.get_serializable_char_spans() {
                        text.char_spans.push(SerializableCharSpan {
                            bbox: span.bbox,
                            text: span.text,
                            char_start: span.char_start + offset,
                            char_end: span.char_end + offset,
                            page_id: span.page_id,
                        });
                    }

                    // add page_id
                    Ok(())
                } else {
                    bail!("can't merge element in textblock")
                }
            }
            BlockType::ListBlock(list) => {
                if let ElementType::ListItem = &element.kind {
                    self.bbox.merge(&element.bbox);
                    let original_text = element.text_block.text.trim().to_string();
                    let mut txt = original_text.clone();

                    // Apply word-level corrections to list item text
                    correction::apply_word_corrections(&mut txt);

                    // Collect char_spans from element
                    let char_spans = element.get_serializable_char_spans();

                    list.items.push(ListItem {
                        text: txt,
                        fertext: Some(original_text),
                        char_spans,
                    });
                    Ok(())
                } else {
                    bail!("can't merge element in Listblock")
                }
            }
            BlockType::Header(header) => {
                if let ElementType::Header = &element.kind {
                    self.bbox.merge(&element.bbox);

                    // Store original text if not already stored
                    if header.fertext.is_none() {
                        header.fertext = Some(header.text.clone());
                    }

                    header.text.push_str(&element.text_block.text);

                    // Apply word-level corrections to header text
                    correction::apply_word_corrections(&mut header.text);

                    Ok(())
                } else {
                    bail!("can't merge element in Header")
                }
            }
            BlockType::Footer(footer) => {
                if let ElementType::Footer = &element.kind {
                    self.bbox.merge(&element.bbox);

                    // Store original text if not already stored
                    if footer.fertext.is_none() {
                        footer.fertext = Some(footer.text.clone());
                    }

                    footer.text.push_str(&element.text_block.text);

                    // Apply word-level corrections to footer text
                    correction::apply_word_corrections(&mut footer.text);

                    Ok(())
                } else {
                    bail!("can't merge element in Footer")
                }
            }
            BlockType::Title(_title) => todo!(),
            BlockType::Formula(_formula) => bail!("can't merge element in Formula"),
            BlockType::Image(_image_block) => todo!(),
            BlockType::Figure(_figure_block) => todo!(),
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
            BlockType::Formula(_) => "FORMULA",
            BlockType::Image(_) => "IMAGE",
            BlockType::Figure(_) => "FIGURE",
            BlockType::Table => "TABLE",
        }
    }
}
