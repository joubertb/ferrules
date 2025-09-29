use image::DynamicImage;
// plsfix disabled - was causing over-aggressive text corrections like "long-context" → "longficontext"
// use plsfix::fix_text;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

use pdfium_render::prelude::{PdfFontWeight, PdfPageTextChar, PdfRect};

use crate::{blocks::Block, debug_print, debug_println, layout::model::LayoutBBox};

pub type PageID = usize;
pub type ElementID = usize;

const FERRULES_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Apply character-level font corrections using glyph name resolution
///
/// This implements the PDF viewer approach: code → glyph → glyph name → Unicode
/// This matches how PDF viewers correctly render text, solving the corruption issue
fn apply_character_corrections(
    original_text: &str,
    unicode_value: u32,
    font_name: &str,
) -> (String, bool) {
    // Get the original character for comparison
    let original_char = original_text.chars().next().unwrap_or('\0');

    #[cfg(feature = "correction-engine")]
    {
        use crate::font_analysis::{
            correct_character_with_encoding_differences, correct_character_with_universal_corrector,
        };

        // PRIMARY: Try encoding differences + Adobe Glyph List approach first
        if let Some(corrected_char) =
            correct_character_with_encoding_differences(unicode_value, font_name)
        {
            if corrected_char.is_empty() || corrected_char == "\0" {
                debug_println!(
                    "🔧 ENCODING CORRECTION: Font '{font_name}' - 0x{unicode_value:04X} '{original_char}' → [SUPPRESSED]"
                );
                return (String::new(), true); // Return empty string to suppress character
            } else {
                debug_println!(
                    "🔧 ENCODING CORRECTION: Font '{font_name}' - 0x{unicode_value:04X} '{original_char}' → '{corrected_char}'"
                );
                return (corrected_char, true);
            }
        }

        // FALLBACK: Use UniversalFontCorrector - synthetic mappings approach
        if let Some(corrected_char) =
            correct_character_with_universal_corrector(unicode_value, font_name)
        {
            if corrected_char.is_empty() || corrected_char == "\0" {
                return (String::new(), true); // Return empty string to suppress character
            } else {
                return (corrected_char, true);
            }
        }
    }

    // Try universal glyph-based correction
    // TODO: This requires access to font glyph information from pdfium-render
    // For now, this is disabled until we can extract glyph names
    // {
    //     use crate::font_analysis::UniversalFontCorrector;
    //
    //     if let Some(corrected_char) = UniversalFontCorrector::correct_character_from_glyph(
    //         unicode_value,
    //         None  // glyph_name - need to extract from font
    //     ) {
    //         debug_print!(
    //             "🔧 GLYPH CORRECTOR: Unicode 0x{unicode_value:04X} → '{corrected_char}' (glyph-based)"
    //         );
    //         return (corrected_char.to_string(), true);
    //     }
    // }

    // The universal corrector now handles all font corrections

    // No correction needed - return original text
    (original_text.to_string(), false)
}

// No longer needed - universal corrector handles all font detection

// No longer needed - universal corrector handles all mathematical symbol corrections

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
        // HYPHEN FIX: Handle end-of-line hyphenation when combining lines

        // If current text ends with hyphen and next line starts with letter, remove hyphen
        if self.text.ends_with('-')
            && !txt.is_empty()
            && txt.chars().next().unwrap().is_alphabetic()
        {
            // Remove the trailing hyphen and join directly (no space)
            self.text.pop(); // Remove the '-'
            self.text.push_str(txt);
            debug_print!(
                "🔗 CROSS-LINE HYPHEN REMOVED: text ending with '-' + '{}' → joined without hyphen",
                &txt[..txt.len().min(20)]
            );
        } else {
            // Normal case: add space and append text
            self.text.push(' ');
            self.text.push_str(txt);
        }
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
    Formula,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Element {
    pub id: ElementID,
    pub layout_block_id: i32,
    pub text_block: ElementText,
    pub kind: ElementType,
    pub page_id: usize,
    pub bbox: BBox,
    /// Stores the CharSpans from each line that was pushed to this element
    /// Used for formula processing with proper subscript/superscript detection
    /// Note: Skipped during serialization as this is only needed during processing
    #[serde(skip)]
    pub(crate) line_spans: Vec<Vec<CharSpan>>,
}

impl Element {
    pub fn from_layout_block(id: usize, layout_block: &LayoutBBox, page_id: usize) -> Self {
        let kind = match layout_block.label {
            "Caption" => ElementType::Caption,
            "Formula" => ElementType::Formula,
            "Text" => ElementType::Text,
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
            line_spans: Vec::new(),
        }
    }
    pub fn push_line(&mut self, line: &Line) {
        // Line text is already cleaned in Line::new_from_span() and Line::append()
        if self.text_block.is_empty() {
            self.text_block.push_first(&line.text);
        } else {
            self.text_block.append_line(&line.text);
        }

        // Store the CharSpans for potential subscript/superscript processing
        // This preserves the positioning and font information needed for Formula elements
        self.line_spans.push(line.spans.clone());
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

#[derive(Debug, Clone)]
pub struct CharSpan {
    pub bbox: BBox,
    pub text: String,
    pub rotation: f32,
    pub font_name: String,
    pub font_size: f32,
    pub font_weight: Option<PdfFontWeight>,
    pub char_start_idx: usize,
    pub char_end_idx: usize,
    // Font diagnostic information
    pub original_unicode: Option<char>,
    pub has_corruption: bool,
}

impl CharSpan {
    pub fn new_from_char(char: &PdfPageTextChar, page_bbox: &BBox) -> Self {
        let font_name = char.font_name();
        let original_unicode = char.unicode_char();
        let original_text = original_unicode.unwrap_or_default().to_string();
        let unicode_value = char.unicode_value();

        // Apply character-level font corrections (glyph name resolution)
        let (glyph_corrected_text, has_corruption) =
            apply_character_corrections(&original_text, unicode_value, &font_name);

        // Apply control character corrections (handles \u0012, \u0013, \u0000, \u0001, \u0002)
        #[cfg(feature = "correction-engine")]
        let final_text = {
            use crate::font_analysis::apply_character_corrections;
            let corrected = apply_character_corrections(&glyph_corrected_text);

            // Debug control character corrections at CharSpan level
            if glyph_corrected_text != corrected {
                debug_print!(
                    "🔧 CHARSPAN CONTROL FIX: '{}' → '{}' (Font: {})",
                    glyph_corrected_text
                        .chars()
                        .map(|c| if c.is_control() {
                            format!("\\u{{{:04X}}}", c as u32)
                        } else {
                            c.to_string()
                        })
                        .collect::<String>(),
                    corrected
                        .chars()
                        .map(|c| if c.is_control() {
                            format!("\\u{{{:04X}}}", c as u32)
                        } else {
                            c.to_string()
                        })
                        .collect::<String>(),
                    font_name
                );
            }

            // Check for specific control characters
            for ch in glyph_corrected_text.chars() {
                if matches!(
                    ch,
                    '\u{0002}' | '\u{0012}' | '\u{0013}' | '\u{0000}' | '\u{0001}'
                ) {
                    debug_print!("🎯 FOUND CONTROL CHAR at CharSpan: '\\u{{{:04X}}}' in Font '{}' - Original: '{}'", 
                        ch as u32, font_name, glyph_corrected_text);
                }
            }

            corrected
        };

        #[cfg(not(feature = "correction-engine"))]
        let final_text = glyph_corrected_text;

        // No special case text corrections - use only glyph-based universal approach
        let corrected_final_text = final_text;

        Self {
            bbox: BBox::from_pdfrect(
                char.tight_bounds()
                    .expect("Error init span tight bound char"),
                page_bbox.height(),
            ),
            text: corrected_final_text,
            font_name,
            font_weight: char.font_weight(),
            font_size: char.unscaled_font_size().value,
            rotation: char.get_rotation_clockwise_degrees(),
            char_start_idx: char.index(),
            char_end_idx: char.index(),
            original_unicode,
            has_corruption,
        }
    }
    pub fn append(&mut self, char: &PdfPageTextChar, page_bbox: &BBox) -> Option<()> {
        let char_rotation = char.get_rotation_clockwise_degrees();
        let char_font_size = char.unscaled_font_size().value;
        let char_font_name = char.font_name();
        let char_font_weight = char.font_weight();

        if char_font_size != self.font_size
            || char_font_name != self.font_name
            || char_font_weight != self.font_weight
            || char_rotation != self.rotation
        {
            None
        } else {
            let char_bbox = BBox::from_pdfrect(
                char.loose_bounds().expect("error tight bound"),
                page_bbox.height(),
            );

            let original_text = char.unicode_char().unwrap_or_default().to_string();
            let unicode_value = char.unicode_value();
            let font_name = char.font_name();

            // Apply character-level font corrections (glyph name resolution)
            let (glyph_corrected_text, char_has_corruption) =
                apply_character_corrections(&original_text, unicode_value, &font_name);

            // Apply control character corrections (handles \u0012, \u0013, \u0000, \u0001, \u0002)
            #[cfg(feature = "correction-engine")]
            let char_text = {
                use crate::font_analysis::apply_character_corrections;
                let corrected = apply_character_corrections(&glyph_corrected_text);

                // Debug control character corrections in append method
                if glyph_corrected_text != corrected {
                    debug_print!(
                        "🔧 APPEND CONTROL FIX: '{}' → '{}' (Font: {})",
                        glyph_corrected_text
                            .chars()
                            .map(|c| if c.is_control() {
                                format!("\\u{{{:04X}}}", c as u32)
                            } else {
                                c.to_string()
                            })
                            .collect::<String>(),
                        corrected
                            .chars()
                            .map(|c| if c.is_control() {
                                format!("\\u{{{:04X}}}", c as u32)
                            } else {
                                c.to_string()
                            })
                            .collect::<String>(),
                        font_name
                    );
                }

                corrected
            };

            #[cfg(not(feature = "correction-engine"))]
            let char_text = glyph_corrected_text;

            self.text.push_str(&char_text);

            // No special case text corrections - use only glyph-based universal approach
            self.char_end_idx = char.index();
            self.bbox.merge(&char_bbox);

            // Update corruption flag if any character in span has corruption
            if char_has_corruption {
                self.has_corruption = true;
            }

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

    /// Determines if a new span should start a new line based on spatial positioning
    ///
    /// Uses both X and Y coordinates to detect legitimate line breaks:
    /// - Y-coordinate jump: Significant vertical movement indicates new line
    /// - X-coordinate reset: Horizontal position reset to left margin indicates wrapping
    /// - Text control characters: Only used as hints when spatial positioning is ambiguous
    fn should_start_new_line(&self, new_span: &CharSpan) -> bool {
        // Always start new line if rotation differs
        if new_span.rotation != self.rotation {
            return true;
        }

        // Calculate spatial differences
        let y_diff = new_span.bbox.y0 - self.bbox.y0;
        let x_diff = new_span.bbox.x0 - self.bbox.x1;

        // Thresholds for line break detection
        const SIGNIFICANT_Y_JUMP: f32 = 5.0; // Points indicating clear line break
        const X_RESET_THRESHOLD: f32 = -20.0; // Negative X movement indicating wrap to new line
        const SAME_LINE_Y_TOLERANCE: f32 = 2.0; // Y tolerance for same line (handles slight baseline variations)

        // Strong indicators of new line (spatial positioning takes priority)
        if y_diff.abs() > SIGNIFICANT_Y_JUMP {
            return true;
        }

        // Text wrapping: X resets to left margin with small Y change
        if x_diff < X_RESET_THRESHOLD && y_diff.abs() > SAME_LINE_Y_TOLERANCE {
            return true;
        }

        // Spans are spatially on same line - check if control characters should override
        if y_diff.abs() <= SAME_LINE_Y_TOLERANCE {
            // For very close Y coordinates, ignore text control characters
            // This fixes the footer footnote case where "1\n" and "https://..." should be same line
            return false;
        }

        // Ambiguous spatial positioning - use text control characters as hints
        if new_span.text.ends_with("\n") || new_span.text.ends_with("\x02") {
            return true;
        }

        // Default: continue same line
        false
    }
    // TODO: find a better pattern here
    // return Some if we fail to append the span-> not great
    pub fn append(&mut self, span: CharSpan) -> Result<(), CharSpan> {
        debug_print!(
            "🔵 APPEND called with span: '{}'",
            span.text.chars().take(20).collect::<String>()
        );

        // Use spatial positioning logic instead of simple text/Y checks
        if self.should_start_new_line(&span) {
            // Character corrections are now applied at CharSpan level, no need to re-apply here
            let utf8_fixed = self.text.clone();

            // Apply comprehensive tag processing to spans (bold, subscript, superscript, formula)
            let script_processed = crate::modtext::add_tags(&self.spans);

            // Use script-processed text if it differs significantly from original
            // This preserves regular text while converting mathematical notation
            if !script_processed.is_empty()
                && (script_processed.contains("<sub>")
                    || script_processed.contains("<sup>")
                    || script_processed.contains("<b>"))
            {
                self.text = script_processed;
            } else {
                self.text = utf8_fixed;
            }

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

    /// Finalize line text processing - apply comprehensive text processing for final output
    pub fn finalize(&mut self) {
        // Apply comprehensive text processing to the final line text
        let utf8_fixed = self.text.clone();

        // Apply comprehensive tag processing to spans (bold, subscript, superscript, formula)
        let script_processed = crate::modtext::add_tags(&self.spans);

        // Use script-processed text if it differs significantly from original
        // This preserves regular text while converting mathematical notation
        if !script_processed.is_empty()
            && (script_processed.contains("<sub>")
                || script_processed.contains("<sup>")
                || script_processed.contains("<b>"))
        {
            self.text = script_processed;
        } else {
            self.text = utf8_fixed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_analysis::{
        correct_characters, fix_character_encoding_corruption,
        fix_character_encoding_corruption_with_font,
    };

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

    #[test]
    fn test_character_corruption_detection() {
        // Test mathematical symbol corrections (the main corrections that are still active)
        assert_eq!(correct_characters("∈/"), "∉"); // Mathematical symbol correction
        assert_eq!(correct_characters("6="), "≠"); // Mathematical symbol correction

        // fix_character_encoding_corruption now only does control character filtering
        // Character substitutions have been disabled to prevent false changes
        assert_eq!(fix_character_encoding_corruption("t(e"), "t(e");
        assert_eq!(fix_character_encoding_corruption("w)th"), "w)th");
        assert_eq!(fix_character_encoding_corruption("w(ere"), "w(ere");
        assert_eq!(fix_character_encoding_corruption(")n"), ")n");
        assert_eq!(fix_character_encoding_corruption(")s"), ")s");
        assert_eq!(fix_character_encoding_corruption("(as"), "(as");
        assert_eq!(fix_character_encoding_corruption("t(at"), "t(at");
        assert_eq!(fix_character_encoding_corruption("cons)sts"), "cons)sts");

        // Test no corruption cases
        assert_eq!(
            fix_character_encoding_corruption("normal text"),
            "normal text"
        );
        assert_eq!(
            fix_character_encoding_corruption("hello world"),
            "hello world"
        );

        // Test partial corruption - fix_character_encoding_corruption no longer does substitutions
        assert_eq!(
            fix_character_encoding_corruption("t(e word )s good"),
            "t(e word )s good"
        );

        // Test Unicode quote corruption - fix_character_encoding_corruption no longer does substitutions
        assert_eq!(
            fix_character_encoding_corruption("He said \u{201C}hello\u{201D}"),
            "He said \u{201C}hello\u{201D}"
        );
    }

    #[test]
    fn test_character_corruption_with_font() {
        // fix_character_encoding_corruption_with_font now only does control character filtering
        // Character substitutions have been disabled
        assert_eq!(
            fix_character_encoding_corruption_with_font("t(e", Some("Times-Roman")),
            "t(e"
        );
        assert_eq!(
            fix_character_encoding_corruption_with_font("w)th", Some("Arial")),
            "w)th"
        );

        // Single character - no substitution
        assert_eq!(
            fix_character_encoding_corruption_with_font(")", Some("Times-Roman")),
            ")"
        );
        assert_eq!(
            fix_character_encoding_corruption_with_font("(", Some("Arial")),
            "("
        );

        // Test the specific cases from mathbert.json - no substitution
        assert_eq!(
            fix_character_encoding_corruption_with_font(")nput", Some("Times-Roman")),
            ")nput"
        );
        assert_eq!(
            fix_character_encoding_corruption_with_font("concatenat)on", Some("Arial")),
            "concatenat)on"
        );
    }
}
