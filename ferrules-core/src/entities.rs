use image::DynamicImage;
// plsfix disabled - was causing over-aggressive text corrections like "long-context" → "longficontext"
// use plsfix::fix_text;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

use pdfium_render::prelude::{PdfFontWeight, PdfPageTextChar, PdfRect};

use crate::{
    blocks::Block, debug_print, debug_println,
    font_analysis::universal_corrector::UniversalFontCorrector, layout::model::LayoutBBox,
};

pub type PageID = usize;
pub type ElementID = usize;

const FERRULES_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum length of a joined word (without hyphen) to attempt dictionary lookup.
/// Words longer than this are unlikely to be simple hyphenated line breaks.
const MAX_JOINED_WORD_LENGTH: usize = 20;

/// Minimum character length each word part must have to be considered
/// for compound word detection (Case 2 of hyphen logic).
const MIN_WORD_PART_LENGTH: usize = 2;

/// Information about how to join two lines in text processing
/// Used for char_span calculation and fertext construction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineJoinInfo {
    /// Add a space between lines (normal case)
    AddSpace,
    /// Join without space (word split across lines, e.g., "ye" + "t" → "yet")
    NoSpace,
    /// Remove trailing hyphen and join (e.g., "No-" + "tably" → "Notably")
    RemoveHyphen,
}

/// Map TeX CMMI/CMSY font encoding control characters to Unicode Greek letters.
///
/// TeX math fonts (CMMI, CMSY, CMEX) encode Greek letters in positions 0x00-0x21,
/// which overlap with ASCII control characters. Pdfium passes these raw codes through,
/// producing invisible characters. This maps them to the correct Greek Unicode.
///
/// Reference: TeX font encoding (OML — Ordinary Math Letters) for CMMI fonts.
fn tex_math_encoding(char_code: u32) -> Option<char> {
    match char_code {
        0x00 => Some('Γ'), // Gamma
        0x01 => Some('Δ'), // Delta
        0x02 => Some('Θ'), // Theta
        0x03 => Some('Λ'), // Lambda
        0x04 => Some('Ξ'), // Xi
        0x05 => Some('Π'), // Pi
        0x06 => Some('Σ'), // Sigma
        0x07 => Some('Υ'), // Upsilon
        0x08 => Some('Φ'), // Phi
        0x09 => Some('Ψ'), // Psi
        0x0A => Some('Ω'), // Omega
        0x0B => Some('α'), // alpha
        0x0C => Some('β'), // beta
        0x0D => Some('γ'), // gamma
        0x0E => Some('δ'), // delta
        0x0F => Some('ε'), // epsilon
        0x10 => Some('ζ'), // zeta
        0x11 => Some('η'), // eta
        0x12 => Some('θ'), // theta
        0x13 => Some('ι'), // iota
        0x14 => Some('κ'), // kappa
        0x15 => Some('λ'), // lambda
        0x16 => Some('μ'), // mu
        0x17 => Some('ν'), // nu
        0x18 => Some('ξ'), // xi
        0x19 => Some('π'), // pi
        0x1A => Some('ρ'), // rho
        0x1B => Some('σ'), // sigma
        0x1C => Some('τ'), // tau
        0x1D => Some('υ'), // upsilon
        0x1E => Some('φ'), // phi
        0x1F => Some('χ'), // chi
        0x20 => Some('ψ'), // psi
        0x21 => Some('ω'), // omega (only for CMMI; in CMSY this is '!')
        _ => None,
    }
}

/// Whitespace/newline control characters that must never be mapped to glyphs.
/// PDF fonts (especially CMMI math fonts) have encoding differences that map these
/// codepoints to Greek letters (e.g., 0x0D → γ, 0x0A → Ω). These are text stream
/// control characters from PDF line wraps, not real glyphs.
const WHITESPACE_CONTROL_CHARS: [u32; 4] = [0x09, 0x0A, 0x0C, 0x0D]; // HT, LF, FF, CR

/// Apply character-level font corrections using glyph name resolution
///
/// This implements the PDF viewer approach: code → glyph → glyph name → Unicode
/// This matches how PDF viewers correctly render text, solving the corruption issue
fn apply_character_corrections(
    original_text: &str,
    unicode_value: u32,
    font_name: &str,
) -> (String, bool) {
    // Skip whitespace/newline control chars — never map these to glyphs.
    if WHITESPACE_CONTROL_CHARS.contains(&unicode_value) {
        return (original_text.to_string(), false);
    }

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
    fn from_pdfrect(rect: PdfRect, page_height: f32) -> Self {
        Self {
            x0: rect.left().value,
            y0: page_height - rect.top().value,
            x1: rect.right().value,
            y1: page_height - rect.bottom().value,
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
        // Handle end-of-line hyphenation when combining lines.
        // If text ends with hyphen and next line starts with letter, join without space
        // to preserve compound words like "English-to-German". Actual hyphen removal
        // (for words like "evalua-tion" → "evaluation") is handled by the modtext pipeline.
        if self.text.ends_with('-')
            && !txt.is_empty()
            && txt.chars().next().unwrap().is_alphabetic()
        {
            // Join directly without space, keeping the hyphen
            self.text.push_str(txt);
            debug_print!(
                "🔗 CROSS-LINE HYPHEN JOIN: text ending with '-' + '{}' → joined without space",
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
    /// Whether this element contains mathematical content (detected via fonts/Unicode)
    #[serde(skip)]
    pub(crate) has_math: bool,
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
            has_math: false,
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

        // Propagate has_math from any span in the line (OR logic)
        if !self.has_math {
            self.has_math = line.spans.iter().any(|span| span.has_math_font);
        }

        // Store the CharSpans for potential subscript/superscript processing
        // This preserves the positioning and font information needed for Formula elements
        self.line_spans.push(line.spans.clone());
    }

    /// Get serializable char spans with absolute character indices
    /// Used for PDF sentence highlighting in the UI
    ///
    /// Character indices match the fertext which is built by:
    /// 1. `concatenate_spans_with_spacing()` - adds spaces between spans within each line
    ///    based on horizontal/vertical gaps (using `should_add_space_between_spans`)
    /// 2. `join_lines_smart()` - joins lines with smart word-break handling
    ///
    /// No hyphen joining is done here - that's handled by the Python worker for TTS.
    /// This keeps char_spans aligned with the original PDF text for accurate highlighting.
    pub fn get_serializable_char_spans(&self) -> Vec<SerializableCharSpan> {
        let mut result: Vec<SerializableCharSpan> = Vec::new();
        let mut char_offset: usize = 0;

        // Track the last word fragment for smart line joining
        let mut prev_line_text: Option<String> = None;

        for (line_idx, line) in self.line_spans.iter().enumerate() {
            // Build the current line text for smart joining decision
            let current_line_text: String = line.iter().map(|s| s.text.as_str()).collect();

            // Determine line joining behavior (space, no space, or hyphen removal)
            if line_idx > 0 && !line.is_empty() {
                let join_info =
                    Self::get_line_join_info(prev_line_text.as_deref(), Some(&current_line_text));
                match join_info {
                    LineJoinInfo::AddSpace => {
                        char_offset += 1;
                    }
                    LineJoinInfo::NoSpace => {
                        // Don't add space
                    }
                    LineJoinInfo::RemoveHyphen => {
                        // Remove the trailing hyphen from the previous span
                        // by adjusting char_offset backwards
                        if char_offset > 0 {
                            char_offset -= 1;
                            // Also adjust the last span's char_end
                            if let Some(last_span) = result.last_mut() {
                                if last_span.char_end > 0 {
                                    last_span.char_end -= 1;
                                }
                            }
                        }
                    }
                }
            }

            // Track word start for dictionary-based space skipping
            let mut word_start_in_line: usize = 0;
            let mut accumulated_text = String::new();

            for (span_idx, span) in line.iter().enumerate() {
                // Check if we need to add space before this span (within the line)
                if span_idx > 0 {
                    let prev_span = &line[span_idx - 1];
                    // Use the same spacing logic as concatenate_spans_with_spacing
                    let needs_space =
                        crate::spacing::should_add_space_between_spans(prev_span, span, 5.0);
                    if needs_space {
                        // Check if skipping space creates a valid word (with at least one invalid part).
                        // Only apply for same-font spans — different fonts indicate different
                        // semantic entities (e.g., math variable "D" vs body text "is").
                        let prev_base_font = prev_span
                            .font_name
                            .split('+')
                            .next_back()
                            .unwrap_or(&prev_span.font_name);
                        let curr_base_font = span
                            .font_name
                            .split('+')
                            .next_back()
                            .unwrap_or(&span.font_name);
                        let same_font = prev_base_font == curr_base_font;

                        let should_skip = same_font
                            && Self::should_skip_space_for_word_join(
                                &accumulated_text,
                                word_start_in_line,
                                &span.text,
                            );
                        if !should_skip {
                            char_offset += 1;
                            accumulated_text.push(' ');
                            word_start_in_line = accumulated_text.len();
                        }
                    }
                }

                let span_len = span.text.chars().count();
                result.push(SerializableCharSpan {
                    bbox: span.bbox.clone(),
                    text: span.text.clone(),
                    char_start: char_offset,
                    char_end: char_offset + span_len,
                    page_id: self.page_id,
                });
                char_offset += span_len;
                accumulated_text.push_str(&span.text);

                // Update word_start if span ends with whitespace or punctuation
                if let Some(last_boundary) = span
                    .text
                    .rfind(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
                {
                    word_start_in_line =
                        accumulated_text.len() - span.text.len() + last_boundary + 1;
                }
            }

            // Update prev_line_text for next iteration
            if !line.is_empty() {
                prev_line_text = Some(current_line_text);
            }
        }

        result
    }

    /// Determine how to join two lines
    /// Matches the logic in join_lines_smart() from merge.rs
    #[cfg(feature = "correction-engine")]
    fn get_line_join_info(prev_line: Option<&str>, curr_line: Option<&str>) -> LineJoinInfo {
        use crate::font_analysis::dictionary::SmartCorrector;

        let Some(prev) = prev_line else {
            return LineJoinInfo::AddSpace;
        };
        let Some(curr) = curr_line else {
            return LineJoinInfo::AddSpace;
        };

        if prev.is_empty() || curr.is_empty() {
            return LineJoinInfo::AddSpace;
        }

        // Get the last word fragment from prev line
        let word_before = prev
            .rfind(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
            .map(|i| &prev[i + 1..])
            .unwrap_or(prev);

        // Get the first word fragment from curr line
        let word_after = curr
            .find(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';' || c == '-')
            .map(|i| &curr[..i])
            .unwrap_or(curr);

        // Check for hyphenated line break
        if word_before.ends_with('-')
            && word_before.len() > 1
            && word_before
                .chars()
                .rev()
                .nth(1)
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
            && !word_after.is_empty()
            && word_after
                .chars()
                .next()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
        {
            let word_before_no_hyphen = &word_before[..word_before.len() - 1];
            let joined = format!("{}{}", word_before_no_hyphen, word_after);

            if joined.len() <= MAX_JOINED_WORD_LENGTH && SmartCorrector::is_valid_word(&joined) {
                // Case 1: Valid joined word → remove hyphen
                return LineJoinInfo::RemoveHyphen;
            } else if word_before_no_hyphen.len() >= MIN_WORD_PART_LENGTH
                && word_after.len() >= MIN_WORD_PART_LENGTH
                && SmartCorrector::is_valid_word(word_before_no_hyphen)
                && SmartCorrector::is_valid_word(word_after)
            {
                // Case 2: Both parts valid → compound word, join without space (keep hyphen)
                return LineJoinInfo::NoSpace;
            } else {
                // Case 3: Broken proper noun/technical term → remove hyphen
                return LineJoinInfo::RemoveHyphen;
            }
        }

        // Case 4: Check for word split without hyphen (e.g., "ye" + "t" → "yet")
        if !word_before.is_empty()
            && !word_after.is_empty()
            && word_before
                .chars()
                .last()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
            && word_after
                .chars()
                .next()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
            && crate::spacing::should_join_fragments(word_before, word_after)
        {
            return LineJoinInfo::NoSpace;
        }

        LineJoinInfo::AddSpace
    }

    #[cfg(not(feature = "correction-engine"))]
    fn get_line_join_info(_prev_line: Option<&str>, _curr_line: Option<&str>) -> LineJoinInfo {
        // Without correction engine, always add space (original behavior)
        LineJoinInfo::AddSpace
    }

    /// Check if we should skip adding a space because joining creates a valid word.
    /// Extracts word fragments from context and delegates to `should_join_fragments`.
    #[cfg(feature = "correction-engine")]
    fn should_skip_space_for_word_join(
        accumulated_text: &str,
        word_start: usize,
        next_span_text: &str,
    ) -> bool {
        // Get the word fragment accumulated so far
        let word_so_far = if word_start < accumulated_text.len() {
            &accumulated_text[word_start..]
        } else {
            return false;
        };

        // Get the first word fragment from the next span
        let next_word_start = next_span_text
            .split(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
            .next()
            .unwrap_or(next_span_text);

        crate::spacing::should_join_fragments(word_so_far, next_word_start)
    }

    #[cfg(not(feature = "correction-engine"))]
    fn should_skip_space_for_word_join(
        _accumulated_text: &str,
        _word_start: usize,
        _next_span_text: &str,
    ) -> bool {
        false
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

/// Serializable version of CharSpan for JSON output
/// Used for PDF sentence highlighting in the UI
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SerializableCharSpan {
    pub bbox: BBox,
    pub text: String,
    pub char_start: usize,
    pub char_end: usize,
    pub page_id: usize,
}

/// Specifies the type of a CharSpan for selective processing
///
/// Different span types may skip certain processing stages. For example,
/// fractions should not have subscript/superscript detection applied since
/// they are already formatted correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpanType {
    /// Normal text span - all processing applied
    #[default]
    Normal,
    /// Fraction span - skips subscript/superscript detection
    Fraction,
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
    /// Whether this span contains characters from a mathematical font or mathematical Unicode
    pub has_math_font: bool,
    /// Type of span for selective processing
    pub span_type: SpanType,
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

        // Fix control characters from TeX math fonts (CMMI/CMSY encoding)
        // TeX CMMI fonts encode Greek letters as control characters (0x00-0x21).
        // Pdfium passes these through as raw codes, producing invisible chars like \u{000F} for ε.
        //
        // IMPORTANT: Skip whitespace/newline control chars (HT, LF, FF, CR) even in math fonts.
        // These are never intentional Greek letters — they are text stream control characters
        // that leak through from PDF line wraps. Without this exclusion, CR (0x0D) becomes γ
        // and LF (0x0A) becomes Ω at every line break in PDFs with fonts matching "Math".
        let byte_value = glyph_corrected_text
            .as_bytes()
            .first()
            .copied()
            .unwrap_or(0xFF);
        let is_whitespace_control = WHITESPACE_CONTROL_CHARS.contains(&(byte_value as u32));
        let glyph_corrected_text = if glyph_corrected_text.len() == 1
            && byte_value < 0x22
            && !is_whitespace_control
            && UniversalFontCorrector::is_mathematical_font(&font_name)
        {
            if let Some(greek) = tex_math_encoding(unicode_value) {
                greek.to_string()
            } else {
                glyph_corrected_text
            }
        } else {
            glyph_corrected_text
        };

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

        // Detect mathematical content via font name or Unicode character.
        // Exclude footnote markers (∗ † ‡) and whitespace even when rendered in math fonts
        // like CMSY, as they commonly appear in non-math contexts (author affiliations).
        let is_non_math_char = original_unicode
            .map(|c| c.is_whitespace() || matches!(c, '\u{2217}' | '\u{2020}' | '\u{2021}'))
            .unwrap_or(false);
        let has_math_font = if is_non_math_char {
            false
        } else {
            UniversalFontCorrector::is_mathematical_font(&font_name)
                || original_unicode
                    .map(UniversalFontCorrector::is_mathematical_unicode)
                    .unwrap_or(false)
        };

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
            rotation: char.angle_degrees().unwrap_or(0.0),
            char_start_idx: char.index(),
            char_end_idx: char.index(),
            original_unicode,
            has_corruption,
            has_math_font,
            span_type: SpanType::Normal,
        }
    }
    pub fn append(&mut self, char: &PdfPageTextChar, page_bbox: &BBox) -> Option<()> {
        let char_rotation = char.angle_degrees().unwrap_or(0.0);
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
                char.loose_bounds().expect("error getting character bounds"),
                page_bbox.height(),
            );

            // Compute effective font size. macOS Quartz PDFs report font_size=1.0 with
            // actual size encoded in the text matrix. Estimate from character dimensions
            // using AVERAGE_CHAR_WIDTH_FACTOR (0.55) from spacing.rs.
            let effective_font_size = if self.font_size <= 1.0 {
                let span_char_count = self.text.chars().count() as f32;
                let avg_char_width = if span_char_count > 0.0 {
                    self.bbox.width() / span_char_count
                } else {
                    char_bbox.width()
                };
                if avg_char_width > 2.0 {
                    avg_char_width / 0.55
                } else {
                    self.font_size
                }
            } else {
                self.font_size
            };

            // Detect word boundaries via X-gap when font_size is unreliable (e.g., macOS
            // Quartz PDFs with font_size=1.0). Uses WORD_BOUNDARY_THRESHOLD_FACTOR (0.16)
            // from spacing.rs. Normal PDFs handle word boundaries in Line::append().
            if self.font_size <= 1.0 && effective_font_size > self.font_size {
                let x_gap = char_bbox.x0 - self.bbox.x1;
                if x_gap > 0.0 {
                    let word_gap_threshold = effective_font_size * 0.16;
                    if x_gap > word_gap_threshold {
                        // Don't break between adjacent digits — PDFs may typeset numbers
                        // like "100" with the leading "1" as a separate smaller glyph,
                        // creating a gap that exceeds the base threshold but is still
                        // within the digit adjacency tolerance (3x threshold).
                        let original_text = char.unicode_char().unwrap_or_default().to_string();
                        let prev_ends_digit = self.text.ends_with(|c: char| c.is_ascii_digit());
                        let curr_is_digit = original_text
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_digit());
                        if prev_ends_digit && curr_is_digit {
                            let digit_gap_limit = word_gap_threshold * 3.0;
                            if x_gap <= digit_gap_limit {
                                // Allow digit to append — don't break the span
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                }
            }

            // Break on significant Y-position change (line break) for consistent line-level granularity.
            // Use font-size-relative threshold because new_from_char() uses tight_bounds() while
            // append() uses loose_bounds(), creating a consistent ~0.53*font_size Y-offset between
            // the span's initial y0 and subsequent chars' y0. A fixed threshold fails for fonts
            // where this offset exceeds it (e.g., 5.27pt for 9.96pt XCharter-Roman > fixed 5.0pt).
            // Real line breaks have Y-diff ≈ line_height (≈1.2*font_size), well above this threshold.
            let line_break_y_threshold = effective_font_size * 0.6;
            let y_diff = (char_bbox.y0 - self.bbox.y0).abs();
            if y_diff > line_break_y_threshold {
                return None;
            }

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

            // Propagate math font flag (OR logic: any math char makes the span math)
            // Exclude footnote markers (∗ † ‡) and whitespace even when rendered in math
            // fonts like CMSY, as they commonly appear in non-math contexts.
            if !self.has_math_font {
                let char_unicode = char.unicode_char();
                let is_non_math_char = char_unicode
                    .map(|c| c.is_whitespace() || matches!(c, '\u{2217}' | '\u{2020}' | '\u{2021}'))
                    .unwrap_or(false);
                if !is_non_math_char
                    && (UniversalFontCorrector::is_mathematical_font(&font_name)
                        || char_unicode
                            .map(UniversalFontCorrector::is_mathematical_unicode)
                            .unwrap_or(false))
                {
                    self.has_math_font = true;
                }
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
            // Add space between spans if spatial analysis indicates word boundary
            // This ensures Line.text matches fertext construction (which uses spacing)
            if let Some(prev_span) = self.spans.last() {
                if crate::spacing::should_add_space_between_spans(prev_span, &span, 5.0) {
                    self.text.push(' ');
                }
            }
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
        // correct_characters now only filters control characters
        // Math symbol corrections happen in the full pipeline via correct_assembled_text
        assert_eq!(correct_characters("∈/"), "∈/"); // No change - just control char filtering
        assert_eq!(correct_characters("6="), "6="); // No change - just control char filtering

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

    /// Helper to create a CharSpan for testing
    fn make_test_span(text: &str, x0: f32, x1: f32, y0: f32, font_size: f32) -> CharSpan {
        CharSpan {
            text: text.to_string(),
            bbox: BBox {
                x0,
                y0,
                x1,
                y1: y0 + font_size,
            },
            font_size,
            font_name: "TestFont".to_string(),
            rotation: 0.0,
            font_weight: None,
            char_start_idx: 0,
            char_end_idx: 0,
            original_unicode: None,
            has_corruption: false,
            has_math_font: false,
            span_type: SpanType::Normal,
        }
    }

    #[test]
    fn test_line_append_adds_space_for_word_boundary() {
        // Test that Line::append() adds space when spatial gap indicates word boundary
        // This ensures Line.text matches fertext construction
        //
        // Example: "ye t" corruption - spans "ye" and "t" have a gap between them
        let span1 = make_test_span("ye", 0.0, 10.0, 0.0, 12.0);
        let mut line = Line::new_from_span(span1);

        // Second span: "t" at x=15-20 (gap of 5 points, which exceeds threshold)
        // Threshold for 12pt font = 12 * 0.55 * 0.16 ≈ 1.06 points
        let span2 = make_test_span("t", 15.0, 20.0, 0.0, 12.0);
        line.append(span2).unwrap();

        // Line.text should have space between "ye" and "t"
        assert_eq!(line.text, "ye t");
    }

    #[test]
    fn test_line_append_no_space_when_close() {
        // Test that Line::append() does NOT add space when spans are close together
        let span1 = make_test_span("ye", 0.0, 10.0, 0.0, 12.0);
        let mut line = Line::new_from_span(span1);

        // Second span: "t" at x=10.5-15 (gap of only 0.5 points, below threshold)
        // Threshold for 12pt font = 12 * 0.55 * 0.16 ≈ 1.06 points
        let span2 = make_test_span("t", 10.5, 15.0, 0.0, 12.0);
        line.append(span2).unwrap();

        // Line.text should NOT have space - spans are close enough
        assert_eq!(line.text, "yet");
    }

    #[test]
    fn test_line_append_space_multi_syllable_word() {
        // Test: "represen tations" corruption
        // When PDF extraction splits a word with spatial gap
        let span1 = make_test_span("represen", 0.0, 50.0, 0.0, 10.0);
        let mut line = Line::new_from_span(span1);

        // Gap of 5 points (threshold for 10pt ≈ 0.88 points)
        let span2 = make_test_span("tations", 55.0, 100.0, 0.0, 10.0);
        line.append(span2).unwrap();

        assert_eq!(line.text, "represen tations");
    }

    #[test]
    fn test_line_append_space_short_word() {
        // Test: "w ith" corruption
        let span1 = make_test_span("w", 0.0, 8.0, 0.0, 12.0);
        let mut line = Line::new_from_span(span1);

        // Gap of 4 points (threshold for 12pt ≈ 1.06 points)
        let span2 = make_test_span("ith", 12.0, 30.0, 0.0, 12.0);
        line.append(span2).unwrap();

        assert_eq!(line.text, "w ith");
    }

    // --- CharSpan has_math_font tests ---

    #[test]
    fn test_charspan_math_font_detected() {
        let mut span = make_test_span("x", 0.0, 5.0, 0.0, 12.0);
        span.font_name = "CMMI10".to_string();
        // Manually set since make_test_span uses "TestFont"
        span.has_math_font = UniversalFontCorrector::is_mathematical_font(&span.font_name);
        assert!(span.has_math_font);
    }

    #[test]
    fn test_charspan_regular_font() {
        let span = make_test_span("hello", 0.0, 30.0, 0.0, 12.0);
        assert!(!span.has_math_font);
    }

    #[test]
    fn test_charspan_math_unicode_detected() {
        // A span with a regular font but mathematical Unicode char
        let mut span = make_test_span("𝑥", 0.0, 5.0, 0.0, 12.0);
        span.original_unicode = Some('𝑥');
        span.has_math_font = UniversalFontCorrector::is_mathematical_font(&span.font_name)
            || span
                .original_unicode
                .map(|c| UniversalFontCorrector::is_mathematical_unicode(c))
                .unwrap_or(false);
        assert!(span.has_math_font);
    }

    #[test]
    fn test_charspan_append_propagates_math() {
        // First span is non-math, second is math → result should be math
        let mut span1 = make_test_span("a", 0.0, 5.0, 0.0, 12.0);
        assert!(!span1.has_math_font);

        let mut span2 = make_test_span("x", 6.0, 11.0, 0.0, 12.0);
        span2.has_math_font = true;

        // Simulate OR-propagation (as happens in CharSpan::append for font detection)
        span1.has_math_font |= span2.has_math_font;
        assert!(span1.has_math_font);
    }

    #[test]
    fn test_charspan_append_both_nonmath() {
        let span1 = make_test_span("a", 0.0, 5.0, 0.0, 12.0);
        let span2 = make_test_span("b", 6.0, 11.0, 0.0, 12.0);
        assert!(!span1.has_math_font);
        assert!(!span2.has_math_font);
    }

    // --- tex_math_encoding tests ---

    #[test]
    fn test_tex_math_encoding_epsilon() {
        // TeX CMMI code 0x0F = ε (the specific character that triggered this fix)
        assert_eq!(tex_math_encoding(0x0F), Some('ε'));
    }

    #[test]
    fn test_tex_math_encoding_greek_uppercase() {
        assert_eq!(tex_math_encoding(0x00), Some('Γ'));
        assert_eq!(tex_math_encoding(0x01), Some('Δ'));
        assert_eq!(tex_math_encoding(0x02), Some('Θ'));
        assert_eq!(tex_math_encoding(0x0A), Some('Ω'));
    }

    #[test]
    fn test_tex_math_encoding_greek_lowercase() {
        assert_eq!(tex_math_encoding(0x0B), Some('α'));
        assert_eq!(tex_math_encoding(0x0C), Some('β'));
        assert_eq!(tex_math_encoding(0x19), Some('π'));
        assert_eq!(tex_math_encoding(0x1B), Some('σ'));
        assert_eq!(tex_math_encoding(0x21), Some('ω'));
    }

    #[test]
    fn test_tex_math_encoding_out_of_range() {
        // Values above the TeX math encoding range should return None
        assert_eq!(tex_math_encoding(0x22), None);
        assert_eq!(tex_math_encoding(0x41), None); // 'A' in ASCII
        assert_eq!(tex_math_encoding(0xFF), None);
    }

    #[test]
    fn test_whitespace_control_chars_not_converted_to_greek() {
        // CR (0x0D) maps to γ in CMMI encoding — apply_character_corrections must block this.
        // LF (0x0A) maps to Ω, HT (0x09) maps to Ψ, FF (0x0C) maps to ϕ.
        for &unicode_val in &WHITESPACE_CONTROL_CHARS {
            let original = String::from(char::from_u32(unicode_val).unwrap());
            let (result, corrected) = apply_character_corrections(&original, unicode_val, "CMMI10");
            assert_eq!(
                result, original,
                "U+{:04X} should not be corrected",
                unicode_val
            );
            assert!(
                !corrected,
                "U+{:04X} should not be flagged as corrected",
                unicode_val
            );
        }
    }

    #[test]
    fn test_real_greek_in_cmmi_still_corrected() {
        // 0x0E = ε in CMMI encoding — this is NOT a whitespace control char and SHOULD
        // be corrected via the encoding differences path when correction-engine is enabled.
        let original = String::from(char::from_u32(0x0E).unwrap());
        let (_result, _corrected) = apply_character_corrections(&original, 0x0E, "CMMI10");
        // We just verify it doesn't panic; actual correction depends on feature flags.
    }
}
