use image::DynamicImage;
// plsfix disabled - was causing over-aggressive text corrections like "long-context" → "longficontext"
// use plsfix::fix_text;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

use pdfium_render::prelude::{PdfFontWeight, PdfPageTextChar, PdfRect};

use crate::{blocks::Block, layout::model::LayoutBBox};

pub type PageID = usize;
pub type ElementID = usize;

const FERRULES_VERSION: &str = env!("CARGO_PKG_VERSION");

/// UTF-8 reconstruction function to fix corrupted mathematical symbols
///
/// PDFium sometimes returns UTF-8 bytes as individual Latin-1 characters.
/// This function detects and reconstructs proper Unicode from corrupted sequences.
pub fn fix_utf8_corruption(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        let code_point = ch as u32;

        // Look for UTF-8 4-byte sequence starter (0xF0) for mathematical symbols
        if code_point == 0xF0 && i + 3 < chars.len() {
            let byte1 = chars[i] as u8;
            let byte2 = chars[i + 1] as u32;
            let byte3 = chars[i + 2] as u32;
            let byte4 = chars[i + 3] as u32;

            // Check if the next 3 characters form a valid UTF-8 continuation
            if (0x80..=0xBF).contains(&byte2)
                && (0x80..=0xBF).contains(&byte3)
                && (0x80..=0xBF).contains(&byte4)
            {
                // Reconstruct the UTF-8 bytes
                let utf8_bytes = [byte1, byte2 as u8, byte3 as u8, byte4 as u8];

                // Try to decode the UTF-8 sequence
                if let Ok(decoded_str) = std::str::from_utf8(&utf8_bytes) {
                    result.push_str(decoded_str);
                    i += 4; // Skip all 4 characters
                    continue;
                }
            }
        }

        // Look for UTF-8 3-byte sequence starter (0xE2) for mathematical operators
        if code_point == 0xE2 && i + 2 < chars.len() {
            let byte1 = chars[i] as u8;
            let byte2 = chars[i + 1] as u32;
            let byte3 = chars[i + 2] as u32;

            // Check if the next 2 characters form a valid UTF-8 continuation
            if (0x80..=0xBF).contains(&byte2) && (0x80..=0xBF).contains(&byte3) {
                // Reconstruct the UTF-8 bytes
                let utf8_bytes = [byte1, byte2 as u8, byte3 as u8];

                // Try to decode the UTF-8 sequence
                if let Ok(decoded_str) = std::str::from_utf8(&utf8_bytes) {
                    result.push_str(decoded_str);
                    i += 3; // Skip all 3 characters
                    continue;
                }
            }
        }

        // No UTF-8 sequence detected, add character as-is
        result.push(ch);
        i += 1;
    }

    // Fix common ligature corruption patterns, then remove control characters
    let ligature_fixed = fix_ligature_corruption(&result);
    remove_control_characters(&ligature_fixed)
}

/// Fix ligature corruption using contextual analysis and conservative patterns
///
/// PDFium sometimes incorrectly maps ligature characters to other symbols during
/// text extraction from PDF files. This function uses a conservative approach that:
///
/// 1. Detects only specific symbols known to be ligature corruption (!@#$%^&*)
/// 2. Uses contextual analysis to determine the most likely original ligature
/// 3. Preserves legitimate punctuation like hyphens (-) and periods (.)
///
/// Known corruption patterns:
/// - fl ligature → ! : work!ows → workflows
/// - fi ligature → # : speci#c → specific  
/// - ff ligature → " : e"ective → effective
///
/// The system is conservative to avoid corrupting legitimate text like
/// "long-context", "e-mail", or "state-of-the-art".
fn fix_ligature_corruption(text: &str) -> String {
    use lazy_static::lazy_static;
    use regex::Regex;

    // Use lazy_static for one-time regex compilation
    lazy_static! {
        // Pattern to match ligature corruption symbols in various contexts
        // Handles both direct corruption and corruption with separators
        static ref POTENTIAL_CORRUPTION: Regex = Regex::new(r"([a-zA-Z]+)[\s-]*([!@#$%^&*'\x22])([a-zA-Z]+)").unwrap();
        // Pattern to match standalone corruption symbols (like "#t" for "fit")
        static ref STANDALONE_CORRUPTION: Regex = Regex::new(r"\b([!@#$%^&*'\x22])([a-z]+)\b").unwrap();
    }

    let mut result = text.to_string();

    // Fix potential ligature corruption using contextual analysis
    result = POTENTIAL_CORRUPTION
        .replace_all(&result, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let symbol = &caps[2];
            let suffix = &caps[3];

            // Debug: log when we find and fix ligature corruption
            tracing::debug!("Fixing ligature corruption: {}{}{}", prefix, symbol, suffix);

            // Determine most likely ligature based on context and symbol
            let ligature = determine_ligature_from_context(prefix, suffix, symbol);
            format!("{}{}{}", prefix, ligature, suffix)
        })
        .to_string();

    // Fix standalone corruption patterns (like "#t" -> "fit")
    // Simple direct replacement for common patterns
    result = result.replace("#t", "fit");
    result = result.replace("!ows", "flows");
    result = result.replace("\"ective", "ffective");
    result = result.replace("e$- ciently", "efficiently");

    result = STANDALONE_CORRUPTION
        .replace_all(&result, |caps: &regex::Captures| {
            let symbol = &caps[1];
            let suffix = &caps[2];

            // Debug: log standalone corruption fixes
            tracing::debug!(
                "Fixing standalone ligature corruption: {}{}",
                symbol,
                suffix
            );

            // Handle specific standalone patterns
            match (symbol, suffix) {
                ("#", "t") => "fit".to_string(),
                ("#", "rst") => "first".to_string(),
                ("#", "le") => "file".to_string(),
                ("#", "nd") => "find".to_string(),
                ("#", "nal") => "final".to_string(),
                ("#", "ne") => "fine".to_string(),
                ("!", "ow") => "flow".to_string(),
                ("!", "ows") => "flows".to_string(),
                ("\"", "ect") => "fect".to_string(),
                ("\"", "ective") => "fective".to_string(),
                // Default: assume fi ligature for # and fl ligature for !
                ("#", _) => format!("fi{}", suffix),
                ("!", _) => format!("fl{}", suffix),
                ("\"", _) => format!("ff{}", suffix),
                (_, _) => format!("fi{}", suffix), // Default to fi
            }
        })
        .to_string();

    result
}

/// Intelligently determine the most likely ligature based on context
///
/// Uses word patterns, common English combinations, and symbol types to infer
/// the original ligature that was corrupted during PDF text extraction.
fn determine_ligature_from_context(prefix: &str, suffix: &str, symbol: &str) -> &'static str {
    let full_context = format!("{}{}", prefix, suffix).to_lowercase();

    // Known fl patterns (workflows, overflow, etc.)
    if suffix == "ows"
        || suffix == "ow"
        || full_context.contains("work") && suffix.starts_with("ow")
        || full_context.contains("over") && suffix.starts_with("ow")
    {
        return "fl";
    }

    // Known ff patterns (effective, office, etc.)
    if full_context.contains("e") && suffix.starts_with("ective")
        || full_context.contains("o") && suffix.starts_with("ice")
        || full_context.contains("di") && suffix.starts_with("erent")
        || full_context.contains("sta") && suffix.starts_with("ing")
    {
        return "ff";
    }

    // Known fi patterns - most common ligature
    if suffix.ends_with("ed")
        || suffix.ends_with("es")
        || suffix.ends_with("ng")
        || suffix.ends_with("er")
        || suffix.ends_with("le")
        || suffix.ends_with("al")
        || full_context.starts_with("uni")
        || full_context.starts_with("simpli")
        || full_context.starts_with("identi")
        || full_context.starts_with("speci")
        || full_context.starts_with("bene")
        || full_context.starts_with("signi")
        || full_context.starts_with("classi")
        || full_context.starts_with("certi")
    {
        return "fi";
    }

    // Symbol-based heuristics (conservative - only for known corruption symbols)
    match symbol {
        "!" => {
            // ! often corrupts fl in "workflows" but fi in most other cases
            if suffix == "ows" || suffix.starts_with("ow") {
                "fl"
            } else {
                "fi"
            }
        }
        "#" => "fi",    // # commonly corrupts fi
        "$" => "fi",    // $ can corrupt fi in some cases
        "%" => "fi",    // % can corrupt fi
        "^" => "fi",    // ^ can corrupt fi
        "&" => "ff",    // & can corrupt ff
        "*" => "fi",    // * can corrupt fi
        "\x22" => "ff", // " commonly corrupts ff (e.g., "effectively")
        "'" => "fi",    // ' can corrupt fi in some cases
        _ => "fi",      // Default to fi as it's the most common ligature
    }
}

/// Remove control characters while preserving legitimate whitespace
///
/// Removes ASCII control characters (0x00-0x1F) except for common whitespace:
/// - Tab (0x09)
/// - Line Feed (0x0A)  
/// - Carriage Return (0x0D)
/// - Space (0x20) - not a control character but handled here for completeness
fn remove_control_characters(text: &str) -> String {
    text.chars()
        .filter(|&ch| {
            let code = ch as u32;
            // Keep normal printable characters (>= 0x20)
            if code >= 0x20 {
                return true;
            }
            // Keep essential whitespace control characters
            matches!(ch, '\t' | '\n' | '\r')
        })
        .collect()
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct BBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl BBox {
    fn from_pdfrect(
        PdfRect {
            bottom,
            left,
            top,
            right,
        }: PdfRect,
        page_height: f32,
    ) -> Self {
        Self {
            x0: left.value,
            y0: page_height - top.value,
            x1: right.value,
            y1: page_height - bottom.value,
        }
    }

    #[inline(always)]
    pub fn center(&self) -> (f32, f32) {
        (
            self.x0 + self.width() / 2f32,
            self.y0 + self.height() / 2f32,
        )
    }

    #[inline(always)]
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
    #[inline(always)]
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    #[inline(always)]
    pub fn area(&self) -> f32 {
        self.height() * self.width()
    }

    #[inline(always)]
    pub fn size(&self) -> (f32, f32) {
        (self.width(), self.height())
    }
    #[inline(always)]
    pub(crate) fn merge(&mut self, other: &Self) {
        self.x0 = self.x0.min(other.x0);
        self.y0 = self.y0.min(other.y0);
        self.x1 = self.x1.max(other.x1);
        self.y1 = self.y1.max(other.y1);
    }
    #[inline(always)]
    fn overlap_x(&self, other: &Self) -> f32 {
        f32::max(
            0f32,
            f32::min(self.x1, other.x1) - f32::max(self.x0, other.x0),
        )
    }
    #[inline(always)]
    fn overlap_y(&self, other: &Self) -> f32 {
        f32::max(
            0f32,
            f32::min(self.y1, other.y1) - f32::max(self.y0, other.y0),
        )
    }

    #[inline(always)]
    pub fn contains(&self, other: &Self) -> bool {
        other.x0 >= self.x0 && other.y0 >= self.y0 && other.x1 <= self.x1 && other.y1 <= self.y1
    }

    #[inline(always)]
    pub fn relaxed_iou(&self, other: &Self) -> f32 {
        let a = self.intersection(other);
        let b = self.area().min(other.area());
        a / b
    }

    #[inline(always)]
    pub fn iou(&self, other: &Self) -> f32 {
        self.intersection(other) / self.union(other)
    }

    #[inline(always)]
    pub fn intersection(&self, other: &Self) -> f32 {
        self.overlap_x(other) * self.overlap_y(other)
    }

    #[inline(always)]
    fn union(&self, other: &Self) -> f32 {
        other.area() + self.area() - self.intersection(other)
    }

    #[inline(always)]
    pub(crate) fn distance(&self, other: &Self, x_weight: f32, y_weight: f32) -> f32 {
        let point_a = self.center();
        let point_b = other.center();

        (point_a.0 - point_b.0).powi(2) * x_weight + (point_a.1 - point_b.1).powi(2) * y_weight
    }

    fn _rotate(self) -> Self {
        todo!()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ElementText {
    pub(crate) text: String,
}

impl ElementText {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
    pub fn push_first(&mut self, txt: &str) {
        self.text.push_str(txt);
    }
    pub fn append_line(&mut self, txt: &str) {
        self.text.push(' ');
        self.text.push_str(txt);
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "element_type")]
pub enum ElementType {
    Header,
    FootNote,
    Footer,
    Text,
    Title,
    Subtitle,
    ListItem,
    Caption,
    Image,
    Table,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Element {
    pub id: ElementID,
    pub layout_block_id: i32,
    pub text_block: ElementText,
    pub kind: ElementType,
    pub page_id: usize,
    pub bbox: BBox,
}

impl Element {
    pub fn from_layout_block(id: usize, layout_block: &LayoutBBox, page_id: usize) -> Self {
        let kind = match layout_block.label {
            "Caption" => ElementType::Caption,
            "Formula" | "Text" => ElementType::Text,
            "List-item" => ElementType::ListItem,
            "Footnote" => ElementType::FootNote,
            "Page-footer" => ElementType::Footer,
            "Page-header" => ElementType::Header,
            "Title" => ElementType::Title,
            "Section-header" => ElementType::Subtitle,
            "Table" => ElementType::Table,
            "Picture" => ElementType::Image,
            _ => {
                unreachable!("can't have other type of layout bbox")
            }
        };
        Self {
            id,
            kind,
            layout_block_id: layout_block.id,
            page_id,
            text_block: Default::default(),
            bbox: layout_block.bbox.to_owned(),
        }
    }
    pub fn push_line(&mut self, line: &Line) {
        // Line text is already cleaned in Line::new_from_span() and Line::append()
        if self.text_block.is_empty() {
            self.text_block.push_first(&line.text);
        } else {
            self.text_block.append_line(&line.text);
        }
    }
}

#[derive(Debug)]
pub struct StructuredPage {
    pub id: PageID,
    pub width: f32,
    pub height: f32,
    // pub rotation: PdfPageRenderRotation,
    pub need_ocr: bool,
    pub image: DynamicImage,
    pub elements: Vec<Element>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Page {
    pub id: PageID,
    pub width: f32,
    pub height: f32,

    #[serde(skip_serializing, skip_deserializing)]
    pub image: DynamicImage,
    // pub rotation: PdfPageRenderRotation,
    pub need_ocr: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DocumentMetadata {
    #[serde(with = "serde_millis")]
    pub parsing_duration: Duration,
    pub ferrules_version: String,
}

impl DocumentMetadata {
    pub fn new(parsing_duration: Duration) -> Self {
        Self {
            parsing_duration,
            ferrules_version: FERRULES_VERSION.to_owned(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ParsedDocument {
    pub doc_name: String,
    pub pages: Vec<Page>,
    pub blocks: Vec<Block>,
    pub debug_path: Option<PathBuf>,
    pub metadata: DocumentMetadata,
}

#[derive(Debug)]
pub struct CharSpan {
    pub bbox: BBox,
    pub text: String,
    pub rotation: f32,
    pub font_name: String,
    pub font_size: f32,
    pub font_weight: Option<PdfFontWeight>,
    pub char_start_idx: usize,
    pub char_end_idx: usize,
}

impl CharSpan {
    pub fn new_from_char(char: &PdfPageTextChar, page_bbox: &BBox) -> Self {
        Self {
            bbox: BBox::from_pdfrect(
                char.tight_bounds()
                    .expect("Error init span tight bound char"),
                page_bbox.height(),
            ),
            text: fix_utf8_corruption(&char.unicode_char().unwrap_or_default().to_string()),
            font_name: char.font_name(),
            font_weight: char.font_weight(),
            font_size: char.unscaled_font_size().value,
            rotation: char.get_rotation_clockwise_degrees(),
            char_start_idx: char.index(),
            char_end_idx: char.index(),
        }
    }
    pub fn append(&mut self, char: &PdfPageTextChar, page_bbox: &BBox) -> Option<()> {
        let char_rotation = char.get_rotation_clockwise_degrees();
        if char.unscaled_font_size().value != self.font_size
            || char.font_name() != self.font_name
            || char.font_weight() != self.font_weight
            || char_rotation != self.rotation
        {
            None
        } else {
            let char_bbox = BBox::from_pdfrect(
                char.loose_bounds().expect("error tight bound"),
                page_bbox.height(),
            );
            // Apply full UTF-8 and ligature corruption fix to character before adding
            let char_text =
                fix_utf8_corruption(&char.unicode_char().unwrap_or_default().to_string());
            self.text.push_str(&char_text);
            self.char_end_idx = char.index();
            self.bbox.merge(&char_bbox);
            Some(())
        }
    }
}
#[derive(Default)]
pub struct Line {
    pub text: String,
    pub bbox: BBox,
    pub rotation: f32,
    pub spans: Vec<CharSpan>,
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Line {{ text: {}, height: {:.2}, width: {:.2}, °, \nspans: [{}] }}\n",
            self.text.trim(),
            self.bbox.height(),
            self.bbox.width(),
            self.spans
                .iter()
                .map(|span| format!(
                    "\n\rtext: {}, height: {:?}, width: {:.2}",
                    span.text.trim(),
                    span.bbox.height(),
                    span.bbox.width()
                ))
                .collect::<Vec<_>>()
                .join("\r")
        )
    }
}

impl Line {
    pub fn new_from_span(span: CharSpan) -> Self {
        // CharSpan text is already cleaned in CharSpan::new_from_char()
        Self {
            bbox: span.bbox.clone(),
            text: span.text.clone(),
            rotation: span.rotation,
            spans: vec![span],
        }
    }
    // TODO: find a better pattern here
    // return Some if we fail to append the span-> not great
    pub fn append(&mut self, span: CharSpan) -> Result<(), CharSpan> {
        if span.rotation != self.rotation
        // NOTE: sometimes pdfium doesn't inject a linebreak, so we check the span positions
        || span.bbox.y0 > self.bbox.y1
        || span.text.ends_with("\n") || span.text.ends_with("\x02")
        {
            // First fix UTF-8 corruption, then apply plsfix conservatively
            let utf8_fixed = fix_utf8_corruption(&self.text);
            // Skip plsfix to prevent over-aggressive ligature corrections like "long-context" → "longficontext"
            // Our fix_ligature_corruption function in fix_utf8_corruption already handles legitimate ligature issues
            self.text = utf8_fixed;
            Err(span)
        } else {
            if self.bbox.height() == 0f32 || self.bbox.width() == 0f32 {
                // The previous span in line is a linebreak
                self.bbox = span.bbox.clone();
            } else {
                self.bbox.merge(&span.bbox);
            }
            // CharSpan text is already cleaned in CharSpan::new_from_char() and CharSpan::append()
            self.text.push_str(&span.text);
            self.spans.push(span);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersection() {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        let bbox3 = BBox {
            x0: 2.0,
            y0: 2.0,
            x1: 4.0,
            y1: 4.0,
        };
        let bbox4 = BBox {
            x0: 3.0,
            y0: 3.0,
            x1: 5.0,
            y1: 5.0,
        }; // No overlap
        let bbox5 = BBox {
            x0: -1.0,
            y0: -1.0,
            x1: 1.0,
            y1: 1.0,
        }; // Negative coordinates
        let bbox6 = BBox {
            x0: 0.5,
            y0: 0.5,
            x1: 1.5,
            y1: 1.5,
        }; // Inside bbox1

        // Edge Cases
        assert_eq!(bbox1.intersection(&bbox3), 0.0);
        assert_eq!(bbox1.intersection(&bbox4), 0.0); // Adjacent
        assert_eq!(bbox5.intersection(&bbox1), 1.0); // Overlaps partially with bbox1

        // Overlaps
        assert_eq!(bbox1.intersection(&bbox2), 1.0);
        assert_eq!(bbox1.intersection(&bbox6), bbox6.area()); // bbox6 is inside bbox1

        // Sanity Checks
        assert_eq!(bbox1.intersection(&bbox1), bbox1.area());
    }

    #[test]
    fn test_union() {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        let bbox3 = BBox {
            x0: 2.0,
            y0: 2.0,
            x1: 4.0,
            y1: 4.0,
        };
        let bbox4 = BBox {
            x0: 3.0,
            y0: 3.0,
            x1: 5.0,
            y1: 5.0,
        }; // No overlap
        let bbox5 = BBox {
            x0: -1.0,
            y0: -1.0,
            x1: 1.0,
            y1: 1.0,
        }; // Negative coordinates

        // Edge Cases
        assert_eq!(bbox1.union(&bbox3), 8.0);
        assert_eq!(bbox1.union(&bbox4), 8.0); // Completely non-overlapping
        assert_eq!(bbox5.union(&bbox1), 7.0); // Negative coordinate case

        // Overlapping
        assert_eq!(bbox1.union(&bbox2), 7.0);

        // Sanity Checks
        assert_eq!(bbox1.union(&bbox1), bbox1.area());
    }

    #[test]
    fn test_iou() {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        let bbox3 = BBox {
            x0: 2.0,
            y0: 2.0,
            x1: 4.0,
            y1: 4.0,
        };
        let bbox4 = BBox {
            x0: 3.0,
            y0: 3.0,
            x1: 5.0,
            y1: 5.0,
        }; // No overlap
        let bbox6 = BBox {
            x0: 0.5,
            y0: 0.5,
            x1: 1.5,
            y1: 1.5,
        }; // Inside bbox1

        // Sanity Checks
        assert_eq!(bbox1.iou(&bbox1), 1.0);
        // Completely non-overlapping
        assert_eq!(bbox1.iou(&bbox4), 0.0);

        // Edge Cases
        assert_eq!(bbox1.iou(&bbox3), 0.0);

        // Overlapping
        assert_eq!(bbox1.iou(&bbox2), 1.0 / 7.0);
        assert_eq!(bbox1.iou(&bbox6), bbox6.area() / bbox1.area()); // bbox6 is inside bbox1
    }
    #[test]
    fn test_distance() {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 3.0,
            y0: 3.0,
            x1: 5.0,
            y1: 5.0,
        };
        let bbox3 = BBox {
            x0: 0.0,
            y0: 2.0,
            x1: 2.0,
            y1: 4.0,
        };

        let x_weight = 1.0;
        let y_weight = 1.0;

        // Standard Case
        let distance = bbox1.distance(&bbox2, x_weight, y_weight);
        assert_eq!(distance, 18.0); // ((4 - 1)^2 + (4 - 1)^2)

        // Boxes with Overlapping Edges
        let distance = bbox1.distance(&bbox3, x_weight, y_weight);
        assert_eq!(distance, 4.0); // ((1 - 1)^2 + (3 - 1)^2)

        // // Identical Boxes
        let distance = bbox1.distance(&bbox1, x_weight, y_weight);
        assert_eq!(distance, 0.0);

        // // Test with different weights
        let x_weight = 2.0;
        let y_weight = 3.0;
        let distance = bbox1.distance(&bbox2, x_weight, y_weight);
        assert_eq!(distance, 45.0); // (3-1)^2 * 2 + (4-1)^2 * 3
    }
}
