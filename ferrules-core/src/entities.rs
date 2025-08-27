use image::DynamicImage;
// plsfix disabled - was causing over-aggressive text corrections like "long-context" → "longficontext"
// use plsfix::fix_text;
use regex::Regex;
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

/// Detect and convert subscripts and superscripts to audio-friendly bracket notation
///
/// Analyzes character positioning and font sizes to identify mathematical subscripts
/// and superscripts, converting them to bracket notation for better TTS pronunciation:
/// - Subscripts: "ni" → "n[i]", "LossMSP" → "Loss[MSP]"  
/// - Superscripts: "x²" → "x^[2]", "a³" → "a^[3]"
///
/// Detection criteria:
/// - Vertical position offset (y-coordinate difference)
/// - Font size difference between base and script characters
/// - Horizontal proximity for character grouping
fn detect_script_notation(spans: &[CharSpan]) -> String {
    if spans.is_empty() {
        return String::new();
    }

    // Configuration thresholds - made more lenient
    const SUBSCRIPT_Y_THRESHOLD: f32 = 1.0; // Reduced from 2.0 - more sensitive to small position changes
    const SUPERSCRIPT_Y_THRESHOLD: f32 = 1.0; // Reduced from 2.0
    const FONT_SIZE_RATIO_THRESHOLD: f32 = 0.9; // Increased from 0.85 - less strict font size requirement
    const HORIZONTAL_PROXIMITY: f32 = 20.0; // Increased from 10.0 - allow wider gaps

    let mut result = String::new();
    let mut i = 0;

    while i < spans.len() {
        let base_span = &spans[i];
        let mut script_chars = Vec::new();
        let mut script_type = None; // None, Some("sub"), Some("sup")

        // Also check for common subscript patterns in single spans
        if let Some((base_part, script_part)) = detect_inline_subscript(&base_span.text) {
            result.push_str(&format!("{}[{}]", base_part, script_part));
            i += 1;
            continue;
        }

        // Look ahead for potential script characters
        let mut j = i + 1;
        while j < spans.len() {
            let next_span = &spans[j];

            // Check horizontal proximity - use previous span for proximity, not base
            let prev_span = if j > i + 1 { &spans[j - 1] } else { base_span };
            if next_span.bbox.x0 - prev_span.bbox.x1 > HORIZONTAL_PROXIMITY {
                break;
            }

            // Determine if this is a subscript or superscript relative to base character
            let y_diff = next_span.bbox.y0 - base_span.bbox.y0;
            let font_ratio = next_span.font_size / base_span.font_size;

            // Check if the character is just whitespace or empty - skip these for script detection
            let is_whitespace_only = next_span.text.trim().is_empty();

            // More lenient detection for subscripts, but stricter for superscripts
            let is_subscript = !is_whitespace_only
                && (
                    (y_diff > SUBSCRIPT_Y_THRESHOLD && font_ratio <= FONT_SIZE_RATIO_THRESHOLD)
                        || (y_diff > 0.5 && font_ratio <= 1.0)
                    // Even more lenient for slight position changes
                );
            // Be much more conservative with superscript detection to avoid false positives
            let is_superscript = !is_whitespace_only
                && y_diff < -SUPERSCRIPT_Y_THRESHOLD
                && font_ratio <= FONT_SIZE_RATIO_THRESHOLD
                && font_ratio < 0.8; // Require significant font size difference for superscripts

            if is_subscript {
                if script_type.is_none() {
                    script_type = Some("sub");
                } else if script_type != Some("sub") {
                    break; // Mixed script types, stop grouping
                }
                script_chars.push(&next_span.text);
                j += 1;
            } else if is_superscript {
                if script_type.is_none() {
                    script_type = Some("sup");
                } else if script_type != Some("sup") {
                    break; // Mixed script types, stop grouping
                }
                script_chars.push(&next_span.text);
                j += 1;
            } else {
                break; // Not a script character
            }
        }

        // Generate output based on detected script pattern
        if !script_chars.is_empty() {
            let script_text: String = script_chars.iter().map(|s| s.as_str()).collect();
            // Only apply bracket notation if script text is not empty or just whitespace
            let trimmed_script = script_text.trim();
            if !trimmed_script.is_empty() {
                match script_type {
                    Some("sub") => {
                        // Check if the script text itself contains inline subscripts
                        if let Some((inner_base, inner_script)) =
                            detect_inline_subscript(trimmed_script)
                        {
                            result.push_str(&format!(
                                "{}{}[{}]",
                                base_span.text.trim_end(),
                                inner_base,
                                inner_script
                            ));
                        } else {
                            result.push_str(&format!(
                                "{}[{}]",
                                base_span.text.trim_end(),
                                trimmed_script
                            ));
                        }
                    }
                    Some("sup") => {
                        // Check if the script text itself contains inline subscripts
                        if let Some((inner_base, inner_script)) =
                            detect_inline_subscript(trimmed_script)
                        {
                            result.push_str(&format!(
                                "{}{}^[{}]",
                                base_span.text.trim_end(),
                                inner_base,
                                inner_script
                            ));
                        } else {
                            result.push_str(&format!(
                                "{}^[{}]",
                                base_span.text.trim_end(),
                                trimmed_script
                            ));
                        }
                    }
                    _ => {
                        result.push_str(&base_span.text);
                    }
                }
            } else {
                // If script text is empty/whitespace, treat as regular text
                result.push_str(&base_span.text);
                for script_char in &script_chars {
                    result.push_str(script_char);
                }
            }
            i = j; // Skip processed script characters
        } else {
            result.push_str(&base_span.text);
            i += 1;
        }
    }

    // Add spaces around mathematical symbols for better readability
    add_math_symbol_spacing(&result)
}

// Helper function to add spaces around common mathematical symbols and fix corruptions
fn add_math_symbol_spacing(text: &str) -> String {
    let mut result = text.to_string();

    // First, fix common mathematical symbol corruptions
    result = fix_math_symbol_corruptions(&result);

    let math_symbols = [
        "∈", "∉", "⊂", "⊃", "⊆", "⊇", "∪", "∩", "×", "⋅", "∘", "≤", "≥", "≠", "≡", "≈", "∝", "∞",
        "∑", "∏", "∫", "∂", "∇", "△", "∴", "∵", "→", "←", "↔", "⇒", "⇔",
    ];

    for symbol in &math_symbols {
        // Add spaces around the symbol if they're not already there
        let with_spaces = format!(" {} ", symbol);
        let patterns_to_replace = [
            (format!("{}", symbol), with_spaces.clone()), // symbol with no spaces
            (format!(" {}", symbol), with_spaces.clone()), // symbol with space before only
            (format!("{} ", symbol), with_spaces.clone()), // symbol with space after only
        ];

        for (pattern, replacement) in &patterns_to_replace {
            if result.contains(pattern) && !result.contains(&with_spaces) {
                result = result.replace(pattern, replacement);
                break; // Only replace once per symbol to avoid double-spacing
            }
        }
    }

    // Clean up any double spaces that might have been created
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }

    result
}

// Helper function to fix common mathematical symbol corruptions
fn fix_math_symbol_corruptions(text: &str) -> String {
    let mut result = text.to_string();

    // Fix "∈ /" or "∈/" to "∉" (not element of)
    result = result.replace("∈ /", "∉");
    result = result.replace("∈/", "∉");

    // Fix "6=" or "6[=]" to "≠" (not equal)
    result = result.replace("6=", "≠");
    result = result.replace("6[=]", "≠");

    // Fix equals sign corruption: "E[=]" should be "E ="
    result = result.replace("[=]", " =");

    // Fix bracket corruption around punctuation
    result = result.replace("otherwise[.]", "otherwise.");
    result = result.replace("[.]", ".");
    result = result.replace("[,]", ",");
    result = result.replace("[;]", ";");
    result = result.replace("[:]", ":");

    // Fix misplaced brackets in subscripts like "[e1,]" to "e[1],"
    if let Ok(re) = Regex::new(r"\[([a-zA-Z])([0-9]+),\]") {
        result = re.replace_all(&result, "$1[$2],").to_string();
    }

    // Fix patterns like "[eLE]" to "e[LE]"
    if let Ok(re2) = Regex::new(r"\[([a-zA-Z])([A-Z]+)\]") {
        result = re2.replace_all(&result, "$1[$2]").to_string();
    }

    // Fix angle bracket corruptions like "hn[i], n[j]i" to "(n[i], n[j])"
    if let Ok(re3) = Regex::new(r"h([^h]+)i") {
        result = re3.replace_all(&result, "($1)").to_string();
    }

    result
}

// Helper function to detect common subscript patterns within a single text span
fn detect_inline_subscript(text: &str) -> Option<(String, String)> {
    // Only handle very specific mathematical subscript patterns
    // Be conservative to avoid breaking regular words

    // Handle specific patterns like "Nmask" (capital N + mask)
    if text == "Nmask" {
        return Some(("N".to_string(), "mask".to_string()));
    }

    // Handle comma-separated subscripts like "ei,j", "xi,j", etc.
    if let Ok(re) = Regex::new(r"^([a-z])([ij],[ij]|[ij],[0-9]|[0-9],[ij]|[0-9],[0-9])$") {
        if let Some(caps) = re.captures(text) {
            return Some((caps[1].to_string(), caps[2].to_string()));
        }
    }

    // Handle common single-letter mathematical subscripts: ni, nj, xi, xj, etc.
    if let Ok(re2) = Regex::new(r"^([nxyzeh])([ij])$") {
        if let Some(caps) = re2.captures(text) {
            return Some((caps[1].to_string(), caps[2].to_string()));
        }
    }

    // Handle patterns like "n0", "n1", "x0", "x1" etc (single letter + single digit)
    if let Ok(re3) = Regex::new(r"^([nxyzeh])([0-9])$") {
        if let Some(caps) = re3.captures(text) {
            return Some((caps[1].to_string(), caps[2].to_string()));
        }
    }

    None
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
            // Apply comprehensive text processing when finalizing the line
            let utf8_fixed = fix_utf8_corruption(&self.text);

            // Apply script notation detection to spans for mathematical subscripts/superscripts
            let script_processed = detect_script_notation(&self.spans);

            // Use script-processed text if it differs significantly from original
            // This preserves regular text while converting mathematical notation
            if !script_processed.is_empty() && script_processed.contains('[') {
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

    #[test]
    fn test_detect_script_notation_subscript() {
        // Test subscript detection: "ni" → "n[i]"
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 15.0,
                    y1: 20.0,
                },
                text: "n".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 0,
            },
            CharSpan {
                bbox: BBox {
                    x0: 15.0,
                    y0: 13.0,
                    x1: 18.0,
                    y1: 18.0,
                }, // Lower position (subscript)
                text: "i".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0, // Smaller font
                font_weight: None,
                char_start_idx: 1,
                char_end_idx: 1,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "n[i]");
    }

    #[test]
    fn test_detect_script_notation_subscript_with_trailing_space() {
        // Test subscript detection with trailing space: "Loss " + "total" → "Loss[total]" (no space before bracket)
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 30.0,
                    y1: 20.0,
                },
                text: "Loss ".to_string(), // Note the trailing space
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 4,
            },
            CharSpan {
                bbox: BBox {
                    x0: 30.0,
                    y0: 13.0,
                    x1: 50.0,
                    y1: 18.0,
                }, // Lower position (subscript)
                text: "total".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0, // Smaller font
                font_weight: None,
                char_start_idx: 5,
                char_end_idx: 9,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "Loss[total]"); // Should be trimmed, no space before bracket
    }

    #[test]
    fn test_detect_script_notation_superscript() {
        // Test superscript detection: "x²" → "x^[2]"
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 15.0,
                    y1: 20.0,
                },
                text: "x".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 0,
            },
            CharSpan {
                bbox: BBox {
                    x0: 15.0,
                    y0: 7.0,
                    x1: 18.0,
                    y1: 12.0,
                }, // Higher position (superscript)
                text: "2".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0, // Smaller font
                font_weight: None,
                char_start_idx: 1,
                char_end_idx: 1,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "x^[2]");
    }

    #[test]
    fn test_detect_script_notation_multichar_subscript() {
        // Test multi-character subscript: "Nmask" → "N[mask]"
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 18.0,
                    y1: 20.0,
                },
                text: "N".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 0,
            },
            CharSpan {
                bbox: BBox {
                    x0: 18.0,
                    y0: 13.0,
                    x1: 22.0,
                    y1: 18.0,
                }, // Lower position
                text: "m".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0,
                font_weight: None,
                char_start_idx: 1,
                char_end_idx: 1,
            },
            CharSpan {
                bbox: BBox {
                    x0: 22.0,
                    y0: 13.0,
                    x1: 26.0,
                    y1: 18.0,
                }, // Continued subscript
                text: "a".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0,
                font_weight: None,
                char_start_idx: 2,
                char_end_idx: 2,
            },
            CharSpan {
                bbox: BBox {
                    x0: 26.0,
                    y0: 13.0,
                    x1: 30.0,
                    y1: 18.0,
                }, // Continued subscript
                text: "s".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0,
                font_weight: None,
                char_start_idx: 3,
                char_end_idx: 3,
            },
            CharSpan {
                bbox: BBox {
                    x0: 30.0,
                    y0: 13.0,
                    x1: 34.0,
                    y1: 18.0,
                }, // Continued subscript
                text: "k".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0,
                font_weight: None,
                char_start_idx: 4,
                char_end_idx: 4,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "N[mask]");
    }

    #[test]
    fn test_detect_script_notation_no_script() {
        // Test normal text without subscripts/superscripts
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 15.0,
                    y1: 20.0,
                },
                text: "a".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 0,
            },
            CharSpan {
                bbox: BBox {
                    x0: 15.0,
                    y0: 10.0,
                    x1: 20.0,
                    y1: 20.0,
                }, // Same vertical position
                text: "b".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0, // Same font size
                font_weight: None,
                char_start_idx: 1,
                char_end_idx: 1,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "ab"); // No script notation applied
    }

    #[test]
    fn test_detect_script_notation_empty_script() {
        // Test case where script detection finds space/empty text (should not create empty brackets)
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 15.0,
                    y1: 20.0,
                },
                text: "Loss".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 3,
            },
            CharSpan {
                bbox: BBox {
                    x0: 15.0,
                    y0: 13.0,
                    x1: 18.0,
                    y1: 18.0,
                }, // Lower position but empty/space
                text: " ".to_string(), // Just a space
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0,
                font_weight: None,
                char_start_idx: 4,
                char_end_idx: 4,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "Loss "); // Should preserve space, not create "Loss[ ]"
    }
}
