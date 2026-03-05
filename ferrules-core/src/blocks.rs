use crate::font_analysis as correction;

use crate::{
    entities::{BBox, Element, ElementType, PageID, SerializableCharSpan},
    error::FerrulesError,
};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

fn is_false(v: &bool) -> bool {
    !v
}

pub type TitleLevel = u8;

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct ImageBlock {
    pub id: usize,
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
}

impl ImageBlock {
    pub(crate) fn path(&self) -> String {
        format!("img_{}.png", self.id)
    }
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct TextBlock {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fertext: Option<String>,
    /// Whether this block contains math content (detected from font analysis)
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_math: bool,
    /// Character spans with bounding boxes for sentence highlighting
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub char_spans: Vec<SerializableCharSpan>,
    /// Sentence end positions (character indices) for precise bbox computation
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sentence_ends: Vec<usize>,
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct ListItem {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fertext: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_math: bool,
    /// Character spans with bounding boxes for sentence highlighting
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub char_spans: Vec<SerializableCharSpan>,
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct List {
    pub items: Vec<ListItem>,
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub enum TableAlgorithm {
    #[default]
    Unknown,
    Lattice,
    Stream,
    Vision,
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct TableBlock {
    pub(crate) id: usize,
    pub(crate) caption: Option<String>,
    pub rows: Vec<TableRow>,
    pub has_borders: bool,
    pub algorithm: TableAlgorithm,
}

impl TableBlock {
    pub(crate) fn path(&self) -> String {
        format!("table_{}.png", self.id)
    }
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    pub is_header: bool,
    pub bbox: BBox,
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct TableCell {
    /// IDs of blocks contained within this cell.
    /// This avoids recursion in serializable structures.
    pub content_ids: Vec<usize>,
    pub text: String,
    pub row_span: u8,
    pub col_span: u8,
    pub bbox: BBox,
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
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

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct FormulaBlock {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula_img: Option<String>,
}

#[derive(
    Clone, Debug, Default, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct FigureBlock {
    pub id: usize,
    pub embedded_texts: Vec<String>,
    pub image_bbox: Option<BBox>,
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
}

impl FigureBlock {
    pub(crate) fn path(&self) -> String {
        format!("img_{}.png", self.id)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize)]
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
    Table(TableBlock),
}

impl std::fmt::Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct Block {
    pub id: usize,
    pub kind: BlockType,
    pub pages_id: Vec<PageID>,
    pub bbox: BBox,
}

impl Block {
    pub(crate) fn merge(&mut self, element: Element) -> Result<(), FerrulesError> {
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

                    // Propagate has_math from merged element
                    text.has_math |= element.has_math;

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
                    Err(FerrulesError::BlockMergeError {
                        element: Box::new(element),
                        block_id: self.id,
                        kind: self.kind.clone(),
                    })
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
                        has_math: element.has_math,
                        char_spans,
                    });
                    Ok(())
                } else {
                    Err(FerrulesError::BlockMergeError {
                        element: Box::new(element),
                        block_id: self.id,
                        kind: self.kind.clone(),
                    })
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
                    Err(FerrulesError::BlockMergeError {
                        element: Box::new(element),
                        block_id: self.id,
                        kind: self.kind.clone(),
                    })
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
                    Err(FerrulesError::BlockMergeError {
                        element: Box::new(element),
                        block_id: self.id,
                        kind: self.kind.clone(),
                    })
                }
            }
            BlockType::Title(_title) => todo!(),
            BlockType::Formula(_formula) => Err(FerrulesError::BlockMergeError {
                element: Box::new(element),
                block_id: self.id,
                kind: self.kind.clone(),
            }),
            BlockType::Image(_image_block) => todo!(),
            BlockType::Figure(_figure_block) => Err(FerrulesError::BlockMergeError {
                element: Box::new(element),
                block_id: self.id,
                kind: self.kind.clone(),
            }),
            BlockType::Table(table) => {
                if let ElementType::Table(incoming_table_opt) = &element.kind {
                    self.bbox.merge(&element.bbox);
                    if let Some(incoming_table) = incoming_table_opt {
                        table.rows.extend(incoming_table.rows.clone());
                    }
                    Ok(())
                } else {
                    Err(FerrulesError::BlockMergeError {
                        element: Box::new(element),
                        block_id: self.id,
                        kind: self.kind.clone(),
                    })
                }
            }
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
            BlockType::Table(_) => "TABLE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_textblock_has_math_serializes() {
        let block = TextBlock {
            text: "Loss = sum".to_string(),
            fertext: None,
            has_math: true,
            char_spans: Vec::new(),
            sentence_ends: Vec::new(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(
            json.contains("\"has_math\":true"),
            "has_math:true should appear in JSON: {json}"
        );
    }

    #[test]
    fn test_textblock_no_math_skips_field() {
        let block = TextBlock {
            text: "Hello world".to_string(),
            fertext: None,
            has_math: false,
            char_spans: Vec::new(),
            sentence_ends: Vec::new(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(
            !json.contains("has_math"),
            "has_math:false should be absent from JSON: {json}"
        );
    }

    #[test]
    fn test_textblock_has_math_deserializes() {
        let json = r#"{"text":"Loss = sum","has_math":true}"#;
        let block: TextBlock = serde_json::from_str(json).unwrap();
        assert!(block.has_math);
    }

    #[test]
    fn test_textblock_missing_has_math_defaults_false() {
        let json = r#"{"text":"Hello world"}"#;
        let block: TextBlock = serde_json::from_str(json).unwrap();
        assert!(!block.has_math);
    }

    #[test]
    fn test_listitem_has_math_serializes() {
        let item = ListItem {
            text: "Loss = sum".to_string(),
            fertext: None,
            has_math: true,
            char_spans: Vec::new(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            json.contains("\"has_math\":true"),
            "has_math:true should appear in JSON: {json}"
        );
    }

    #[test]
    fn test_listitem_has_math_false_not_serialized() {
        let item = ListItem {
            text: "Regular list item".to_string(),
            fertext: None,
            has_math: false,
            char_spans: Vec::new(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("has_math"),
            "has_math:false should be absent from JSON: {json}"
        );
    }

    #[test]
    fn test_listitem_has_math_deserializes() {
        let json = r#"{"text":"Loss = sum","has_math":true}"#;
        let item: ListItem = serde_json::from_str(json).unwrap();
        assert!(item.has_math);
    }

    #[test]
    fn test_listitem_missing_has_math_defaults_false() {
        let json = r#"{"text":"Regular list item"}"#;
        let item: ListItem = serde_json::from_str(json).unwrap();
        assert!(!item.has_math);
    }
}
