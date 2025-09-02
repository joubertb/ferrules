//! Mathematical Notation Processing
//!
//! This module handles the detection and conversion of mathematical subscripts and superscripts
//! to bracket notation for improved readability and accessibility.
//!
//! ## Features
//!
//! - Detects subscripts and superscripts based on font size and position
//! - Converts to standardized bracket notation: "x₂" → "x<[2]>", "n^i" → "n^<[i]>"
//! - Handles inline subscript patterns within single text spans
//! - Processes mathematical symbols and spacing

use crate::entities::CharSpan;

/// Check if this baseline/font change represents a real superscript based on positioning and font metrics
fn is_real_superscript(current_span: &CharSpan, baseline_diff: f32, base_font_size: f32) -> bool {
    // Real superscripts should have:
    // 1. Significant font size reduction (typically 70% or smaller of base text)
    // 2. Baseline shift that's proportional to font size
    // 3. Not just small positioning variations

    let font_size_ratio = current_span.font_size / base_font_size;
    let relative_baseline_shift = baseline_diff.abs() / base_font_size;

    // Skip empty or whitespace-only text
    let text_trimmed = current_span.text.trim();
    if text_trimmed.is_empty() {
        return false;
    }

    // Real superscripts typically have smaller font AND significant baseline shift
    let has_smaller_font = font_size_ratio < 0.85; // Font is 85% or smaller
    let has_significant_shift = relative_baseline_shift > 0.3; // Shift is 30% of base font size

    // Both conditions should be present for a real superscript
    // Small baseline variations without font size changes are likely rendering artifacts
    let is_likely_superscript = has_smaller_font && has_significant_shift;

    eprintln!(
        "🔍 SUPERSCRIPT CHECK: '{}' font_ratio={:.2} baseline_shift_ratio={:.2} → {}",
        text_trimmed,
        font_size_ratio,
        relative_baseline_shift,
        if is_likely_superscript {
            "REAL"
        } else {
            "ARTIFACT"
        }
    );

    is_likely_superscript
}

/// Check if this baseline/font change represents a real subscript based on positioning and font metrics
fn is_real_subscript(
    current_span: &CharSpan,
    baseline_diff: f32,
    base_font_size: f32,
    spans: &[CharSpan],
    current_index: usize,
) -> bool {
    let font_size_ratio = current_span.font_size / base_font_size;
    let relative_baseline_shift = baseline_diff.abs() / base_font_size;

    // Skip empty or whitespace-only text
    let text_trimmed = current_span.text.trim();
    if text_trimmed.is_empty() {
        return false;
    }

    // Check if this is a parenthesis or bracket that should be grouped with adjacent subscript content
    if text_trimmed == "(" || text_trimmed == "[" || text_trimmed == "{" {
        // Look ahead to see if the next spans are subscript-level content
        if let Some(next_span) = spans.get(current_index + 1) {
            let next_font_ratio = next_span.font_size / base_font_size;
            let next_text = next_span.text.trim();

            // If the next span is subscript-sized content (not whitespace),
            // then this opening bracket should be treated as part of the subscript group
            if next_font_ratio < 0.85
                && !next_text.is_empty()
                && !next_text.chars().all(|c| c.is_whitespace())
            {
                eprintln!("🔍 SUBSCRIPT CHECK: '{text_trimmed}' → GROUPED (opening bracket with subscript content)");
                return false; // Don't start subscript here, wait for the content
            }
        }
    }

    // Real subscripts have smaller font AND significant baseline shift
    let has_smaller_font = font_size_ratio < 0.85; // Font is 85% or smaller
    let has_significant_shift = relative_baseline_shift > 0.3; // Shift is 30% of base font size

    let is_likely_subscript = has_smaller_font && has_significant_shift;

    eprintln!(
        "🔍 SUBSCRIPT CHECK: '{}' font_ratio={:.2} baseline_shift_ratio={:.2} → {}",
        text_trimmed,
        font_size_ratio,
        relative_baseline_shift,
        if is_likely_subscript {
            "REAL"
        } else {
            "ARTIFACT"
        }
    );

    is_likely_subscript
}

/// Tag types that can be applied to text spans
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub(crate) enum TagType {
    Bold,
    Subscript,
    Superscript,
    Formula,
}

/// Represents a range of characters that should be wrapped with a specific tag
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct TagRange {
    pub start_span_index: usize,
    pub end_span_index: usize,
    pub start_char_index: usize,
    pub end_char_index: usize,
    pub tag_type: TagType,
    pub content_spans: Vec<CharSpan>, // For recursive processing
}

/// Main entry point for recursive tag processing
pub(crate) fn apply_tags_recursive(spans: &[CharSpan], depth: usize) -> String {
    eprintln!(
        "🔄 apply_tags_recursive called with {} spans at depth {}",
        spans.len(),
        depth
    );

    if spans.is_empty() {
        return String::new();
    }

    // Prevent infinite recursion
    if depth > 10 {
        eprintln!("⚠️ Maximum recursion depth reached, returning plain text");
        return spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<&str>>()
            .join("");
    }

    // Step 1: Detect all tag ranges for this level
    let subscript_ranges = detect_subscripts(spans);
    let superscript_ranges = detect_superscripts(spans);
    let bold_ranges = detect_bold(spans);
    // Note: formula detection is handled at Element level, not span level

    // Step 2: Combine and sort all ranges by priority (subscript/superscript first, then bold)
    let mut all_ranges = Vec::new();
    all_ranges.extend(subscript_ranges);
    all_ranges.extend(superscript_ranges);
    all_ranges.extend(bold_ranges);

    // Sort by start position to process in order
    all_ranges.sort_by_key(|r| r.start_span_index);

    // Step 3: Apply tags - for now, just delegate to existing function
    // This maintains current functionality while we build the new architecture
    detect_script_notation(spans)
}

/// Detect subscript patterns in spans
pub(crate) fn detect_subscripts(spans: &[CharSpan]) -> Vec<TagRange> {
    eprintln!("🔍 detect_subscripts called with {} spans", spans.len());
    // TODO: Extract subscript detection logic from detect_script_notation
    // For now, return empty to maintain compatibility
    Vec::new()
}

/// Detect superscript patterns in spans
pub(crate) fn detect_superscripts(spans: &[CharSpan]) -> Vec<TagRange> {
    eprintln!("🔍 detect_superscripts called with {} spans", spans.len());
    // TODO: Extract superscript detection logic from detect_script_notation
    // For now, return empty to maintain compatibility
    Vec::new()
}

/// Detect bold text patterns in spans
pub(crate) fn detect_bold(spans: &[CharSpan]) -> Vec<TagRange> {
    eprintln!("🔍 detect_bold called with {} spans", spans.len());
    // TODO: Extract bold detection logic from detect_script_notation
    // For now, return empty to maintain compatibility
    Vec::new()
}

/// Stack-based detection of subscripts, superscripts, and bold text
///
/// Uses baseline position changes and font changes to determine when to open/close tags:
/// - Baseline moves down → open <sub>, push </sub> on stack
/// - Baseline moves up → open <sup>, push </sup> on stack  
/// - Baseline returns to normal → pop and close current tag
/// - Bold font detected → open <b>, push </b> on stack
/// - Bold font ends → pop and close bold tag
///
/// This approach is content-independent and purely based on positioning/font changes.
pub(crate) fn detect_script_notation(spans: &[CharSpan]) -> String {
    eprintln!("⚡ STACK-BASED detection called with {} spans", spans.len());

    if spans.is_empty() {
        return String::new();
    }

    let full_text: String = spans
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<&str>>()
        .join("");
    eprintln!(
        "⚡ STACK-BASED: Processing text='{}'",
        full_text.chars().take(50).collect::<String>()
    );

    let mut result = String::new();
    let mut tag_stack: Vec<&'static str> = Vec::new();
    let mut baseline: f32 = 0.0;
    let mut baseline_initialized = false;
    let mut current_bold = false;
    let mut in_subscript = false;
    let mut in_superscript = false;
    let mut base_font_size: f32 = 0.0;

    // Define thresholds for baseline changes - now more conservative since we use font analysis
    const SCRIPT_THRESHOLD: f32 = 3.0; // Minimum threshold to consider script changes
    const RETURN_THRESHOLD: f32 = 2.0; // Threshold for returning to baseline

    // Process each span
    for (i, span) in spans.iter().enumerate() {
        eprintln!(
            "🔍 SPAN[{}]: '{}' y={:.1} size={:.1} font={}",
            i,
            span.text.trim(),
            span.bbox.y0,
            span.font_size,
            span.font_name
        );

        // Initialize baseline and base font size on first span
        if !baseline_initialized {
            baseline = span.bbox.y0;
            base_font_size = span.font_size;
            baseline_initialized = true;
            current_bold = is_bold_text(span);
            if current_bold {
                result.push_str("<b>");
                tag_stack.push("</b>");
                eprintln!("🅱️ BOLD START: Pushed </b> on stack");
            }
            eprintln!("📏 BASELINE INIT: {baseline:.1}, BASE FONT SIZE: {base_font_size:.1}");
        } else {
            let baseline_diff = span.bbox.y0 - baseline;
            let is_bold_now = is_bold_text(span);

            // Handle font changes (bold on/off)
            if is_bold_now != current_bold {
                if is_bold_now {
                    result.push_str("<b>");
                    tag_stack.push("</b>");
                    eprintln!("🅱️ BOLD START: Pushed </b> on stack");
                } else {
                    // Close bold tag
                    if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</b>") {
                        let closing_tag = tag_stack.remove(pos);
                        result.push_str(closing_tag);
                        eprintln!("🅱️ BOLD END: Applied {closing_tag}");
                    }
                }
                current_bold = is_bold_now;
            }

            // Check for baseline changes that might indicate scripts
            if baseline_diff.abs() > SCRIPT_THRESHOLD {
                // Close existing script tags before opening new ones
                if in_subscript || in_superscript {
                    if in_subscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sub>") {
                            let closing_tag = tag_stack.remove(pos);
                            result.push_str(closing_tag);
                            eprintln!("🔄 SUB CLOSE: Applied {closing_tag}");
                            in_subscript = false;
                        }
                    }
                    if in_superscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sup>") {
                            let closing_tag = tag_stack.remove(pos);
                            result.push_str(closing_tag);
                            eprintln!("🔄 SUP CLOSE: Applied {closing_tag}");
                            in_superscript = false;
                        }
                    }
                }

                if baseline_diff > 0.0 {
                    // Baseline moved down - potential subscript
                    if is_real_subscript(span, baseline_diff, base_font_size, spans, i) {
                        result.push_str("<sub>");
                        tag_stack.push("</sub>");
                        in_subscript = true;
                        eprintln!("⬇️ SUBSCRIPT START: Real subscript detected");
                    }
                } else {
                    // Baseline moved up - potential superscript
                    if is_real_superscript(span, baseline_diff, base_font_size) {
                        result.push_str("<sup>");
                        tag_stack.push("</sup>");
                        in_superscript = true;
                        eprintln!("⬆️ SUPERSCRIPT START: Real superscript detected");
                    }
                }
                baseline = span.bbox.y0;
            } else if baseline_diff.abs() < RETURN_THRESHOLD && (in_subscript || in_superscript) {
                // Close script tags when returning close to baseline
                // But only if we're not just processing whitespace/punctuation
                let text_trimmed = span.text.trim();
                if !text_trimmed.is_empty()
                    && !text_trimmed
                        .chars()
                        .all(|c| c.is_whitespace() || "=+−-()[]{}.,;:".contains(c))
                {
                    if in_subscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sub>") {
                            let closing_tag = tag_stack.remove(pos);
                            result.push_str(closing_tag);
                            eprintln!(
                                "🔄 BASELINE RETURN: Applied {closing_tag} for '{text_trimmed}'"
                            );
                            in_subscript = false;
                        }
                    }
                    if in_superscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sup>") {
                            let closing_tag = tag_stack.remove(pos);
                            result.push_str(closing_tag);
                            eprintln!(
                                "🔄 BASELINE RETURN: Applied {closing_tag} for '{text_trimmed}'"
                            );
                            in_superscript = false;
                        }
                    }
                    baseline = span.bbox.y0;
                } else {
                    eprintln!("⏭️ KEEPING script mode for whitespace/punct: '{text_trimmed}'");
                }
            }
        }

        // Add the actual text, cleaning up any substitute characters
        let cleaned_text = span.text.replace('\u{001a}', ""); // Remove SUB (substitute) character
        result.push_str(&cleaned_text);
    }

    // Close any remaining open tags
    while let Some(closing_tag) = tag_stack.pop() {
        result.push_str(closing_tag);
        eprintln!("🔚 CLEANUP: Applied remaining {closing_tag}");
    }

    // Apply mathematical symbol corrections to fix patterns like "6=" → "≠", "∈/" → "∉"
    #[cfg(feature = "correction-engine")]
    let corrected_result = {
        use crate::correction::character::fix_math_symbol_corruptions;
        fix_math_symbol_corruptions(&result)
    };

    #[cfg(not(feature = "correction-engine"))]
    let corrected_result = result;

    eprintln!(
        "⚡ STACK-BASED RESULT: '{}'",
        corrected_result.chars().take(100).collect::<String>()
    );
    corrected_result
}

/// Detect if a character span represents bold text
///
/// This function checks the font weight and font name to determine if text should be formatted as bold
/// rather than treated as a subscript or superscript.
pub(crate) fn is_bold_text(span: &CharSpan) -> bool {
    // Check font name for bold indicators (common pattern in PDFs)
    let font_name = span.font_name.to_lowercase();
    if font_name.contains("bold") || font_name.contains("black") || font_name.contains("heavy") {
        return true;
    }

    // Check for font weight indicators
    if let Some(font_weight) = span.font_weight.as_ref() {
        // Use string representation to handle different weight variants
        let weight_str = format!("{font_weight:?}").to_lowercase();
        if weight_str.contains("bold")
            || weight_str.contains("700")
            || weight_str.contains("800")
            || weight_str.contains("900")
        {
            return true;
        }
    }

    false
}

/// Detect inline subscript patterns (stub for compatibility)
pub(crate) fn detect_inline_subscript(_text: &str) -> Option<(String, String)> {
    // This function is no longer used in the stack-based approach
    // Keeping it as a stub for compatibility with mod.rs
    None
}
