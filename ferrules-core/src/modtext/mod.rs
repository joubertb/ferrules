//! Text Modification Module
//!
//! This module provides text modification functionality for enhanced readability
//! and improved text processing. It handles mathematical notation, subscripts,
//! superscripts, and other text enhancements that improve accessibility and
//! usability across various applications.
//!
//! ## Usage
//!
//! ### Mathematical Notation Processing
//! ```rust
//! use ferrules_core::modtext;
//!
//! // Process mathematical subscripts and superscripts for better readability
//! let enhanced = modtext::process_mathematical_notation(&char_spans);
//! ```
//!
//! ### Feature Flag
//! This module is gated behind the `modtext` feature flag. When disabled,
//! all functions return unmodified text for minimal impact on performance.

use crate::debug_print;

/// Remove end-of-line hyphens from span text
///
/// This function handles hyphenated words that span across line breaks by:
/// 1. Detecting spans that end with "-" (hyphen)
/// 2. Checking if the next span starts with a letter
/// 3. Removing the hyphen to properly rejoin the word
///
/// Example: "Vi-" + "jil" → "Vijil"
fn remove_end_of_line_hyphens(spans: &[crate::entities::CharSpan]) -> String {
    if spans.is_empty() {
        return String::new();
    }

    let mut result = String::new();

    for (i, span) in spans.iter().enumerate() {
        let span_text = &span.text;

        // Check if this span ends with hyphen and there's a next span
        if span_text.ends_with('-') && i + 1 < spans.len() {
            let next_span = &spans[i + 1];
            let next_text = &next_span.text;

            // If next span starts with a letter, remove the hyphen (end-of-line hyphenation)
            if !next_text.is_empty() && next_text.chars().next().unwrap().is_alphabetic() {
                // Remove the trailing hyphen
                let without_hyphen = &span_text[..span_text.len() - 1];
                result.push_str(without_hyphen);
                debug_print!(
                    "🔗 HYPHEN REMOVED: '{}' + '{}' → '{}{}'",
                    span_text,
                    &next_text[..next_text.len().min(10)],
                    without_hyphen,
                    &next_text[..next_text.len().min(10)]
                );
            } else {
                // Keep the hyphen - not end-of-line hyphenation
                result.push_str(span_text);
            }
        } else {
            // Normal span - just add it
            result.push_str(span_text);
        }
    }

    result
}

/// Remove line-ending hyphens at the span level
///
/// This function identifies spans that end with "word-" and the next span starts with "word"
/// and combines them into a single "wordword" span, which is the proper way to handle
/// line-ending hyphenation artifacts from PDF text extraction.
fn remove_line_ending_hyphens(spans: &mut Vec<crate::entities::CharSpan>) {
    debug_print!("🔍 HYPHEN REMOVAL: Processing {} spans", spans.len());

    let mut i = 0;
    while i < spans.len().saturating_sub(1) {
        let current_span = &spans[i];
        let next_span = &spans[i + 1];

        // Debug log spans that contain "Vi" or end with hyphens
        if current_span.text.contains("Vi") || current_span.text.ends_with('-') {
            debug_print!(
                "🔍 SPAN {}: '{}' (next: '{}')",
                i,
                current_span.text,
                next_span.text
            );
        }

        // Check if current span ends with line-ending hyphen pattern
        // Only process regular hyphens (U+002D), not em-dashes (U+2014) or en-dashes (U+2013)
        if current_span.text.len() > 1
            && current_span.text.ends_with('-')  // Regular hyphen only
            && !current_span.text.ends_with('—') // Not em-dash
            && !current_span.text.ends_with('–') // Not en-dash
            && current_span.text.chars().rev().nth(1).map(|c| c.is_alphabetic()).unwrap_or(false)
        {
            debug_print!(
                "🔍 FOUND HYPHEN CANDIDATE: '{}' + '{}'",
                current_span.text,
                next_span.text
            );

            // Check if next span starts with word characters
            if !next_span.text.is_empty()
                && next_span
                    .text
                    .chars()
                    .next()
                    .map(|c| c.is_alphabetic())
                    .unwrap_or(false)
            {
                // Skip hyphen removal for likely compound words
                // Common compound word patterns that should be preserved
                let word_before = current_span.text.trim_end_matches('-');
                let word_after = &next_span.text;

                // Check for common compound word patterns
                let likely_compound = is_likely_compound_word(word_before, word_after);

                if likely_compound {
                    debug_print!(
                        "🔍 COMPOUND WORD: Preserving hyphen in '{}-{}' (likely compound word)",
                        word_before,
                        word_after
                    );
                    i += 1;
                    continue;
                }

                // Combine: remove hyphen and join with next span
                let combined_text = format!("{}{}", word_before, next_span.text);

                debug_print!(
                    "🔗 HYPHEN REMOVAL: '{}' + '{}' → '{}'",
                    current_span.text,
                    next_span.text,
                    combined_text
                );

                // Update current span with combined text
                spans[i].text = combined_text;

                // Remove the next span since it's now combined
                spans.remove(i + 1);

                // Don't increment i since we removed a span
                continue;
            } else {
                debug_print!(
                    "🔍 HYPHEN SKIPPED: Next span doesn't start with word char: '{}'",
                    next_span.text
                );
            }
        }

        i += 1;
    }
}

#[cfg(feature = "modtext")]
pub mod mathematical;

#[cfg(feature = "modtext")]
pub mod script_notation;

/// Process text spans for mathematical notation enhancement
///
/// This is the main entry point for mathematical text processing.
/// When the `modtext` feature is disabled, this returns the original text unchanged.
///
/// # Example
/// ```rust
/// use ferrules_core::modtext;
///
/// let enhanced = modtext::process_mathematical_notation(&char_spans);
/// ```
#[allow(dead_code)]
pub fn process_mathematical_notation(spans: &[crate::entities::CharSpan]) -> String {
    debug_print!(
        "🟢 process_mathematical_notation CALLED with {} spans",
        spans.len()
    );
    #[cfg(feature = "modtext")]
    {
        script_notation::apply_text_formatting(spans)
    }

    #[cfg(not(feature = "modtext"))]
    {
        // When feature is disabled, just concatenate the text without processing
        spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Process text spans to add all appropriate tags (bold, subscript, superscript, formula)
///
/// This is the new unified entry point for all text processing with composable tags.
/// When the `modtext` feature is disabled, this returns the original text unchanged.
///
/// # Example
/// ```rust
/// use ferrules_core::modtext;
///
/// let enhanced = modtext::add_tags(&char_spans);
/// ```
pub fn add_tags(spans: &[crate::entities::CharSpan]) -> String {
    // HYPHEN FIX: Handle end-of-line hyphenation before text assembly
    // This fixes cases like "Vi-" + "jil" → "Vijil" (not "Vifijil")
    let hyphen_processed_text = remove_end_of_line_hyphens(spans);

    debug_print!(
        "🏷️ add_tags CALLED with {} spans: '{}' → after hyphen removal: '{}'",
        spans.len(),
        spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>()
            .chars()
            .take(50)
            .collect::<String>(),
        hyphen_processed_text.chars().take(50).collect::<String>()
    );

    #[cfg(feature = "modtext")]
    {
        // Hyphen removal is now handled at span level before HTML processing
        // Just apply the HTML formatting to the already-processed spans
        script_notation::apply_tags_recursive(spans, 0)
    }

    #[cfg(not(feature = "modtext"))]
    {
        // When feature is disabled, return the hyphen-processed text
        hyphen_processed_text
    }
}

/// Detect inline subscript patterns within text
///
/// When the `modtext` feature is disabled, this returns None.
#[allow(dead_code)]
pub fn detect_inline_subscript_pattern(_text: &str) -> Option<(String, String)> {
    #[cfg(feature = "modtext")]
    {
        script_notation::detect_inline_subscript(_text)
    }

    #[cfg(not(feature = "modtext"))]
    {
        None
    }
}

/// Format mathematical formula text with appropriate tags
///
/// When the `modtext` feature is enabled, wraps formula text in XML-style tags.
/// When disabled, returns the text unchanged.
/// Note: Subscript/superscript/bold processing is now handled by add_tags() at the Line level.
pub fn format_formula_text(text: &str) -> String {
    debug_print!(
        "📋 FORMULA: format_formula_text called with: {}",
        text.chars().take(100).collect::<String>()
    );

    // Step 1: Normalize mathematical Unicode variants to ASCII
    let normalized_text = mathematical::normalize_mathematical_unicode(text);
    debug_print!(
        "📋 FORMULA STEP 1 (Unicode normalize): '{}' → '{}'",
        text.chars().take(50).collect::<String>(),
        normalized_text.chars().take(50).collect::<String>()
    );

    // Step 2: Combine mathematical diacritical marks
    let combined_text = mathematical::combine_mathematical_accents(&normalized_text);
    debug_print!(
        "📋 FORMULA STEP 2 (Combine accents): '{}' → '{}'",
        normalized_text.chars().take(50).collect::<String>(),
        combined_text.chars().take(50).collect::<String>()
    );

    // Step 3: Standardize subscript/superscript notation
    let standardized_text = combined_text.clone(); // Note: Script standardization now handled by script_notation module
    debug_print!(
        "📋 FORMULA STEP 3 (Standardize scripts): '{}' → '{}'",
        combined_text.chars().take(50).collect::<String>(),
        standardized_text.chars().take(50).collect::<String>()
    );

    // Step 4: Apply mathematical symbol corrections to fix patterns like "6=" → "≠", "∈/" → "∉"
    // Control character corrections are now applied at CharSpan level
    #[cfg(feature = "correction-engine")]
    let corrected_text = {
        use crate::font_analysis::text_corrections::fix_math_symbol_corruptions;
        let fixed = fix_math_symbol_corruptions(&standardized_text);
        debug_print!(
            "📋 FORMULA STEP 4 (Symbol corrections): '{}' → '{}'",
            standardized_text.chars().take(50).collect::<String>(),
            fixed.chars().take(50).collect::<String>()
        );
        fixed
    };

    #[cfg(not(feature = "correction-engine"))]
    let corrected_text = standardized_text;

    // Step 5: Clean up substitute characters and apply spacing
    let cleaned_text = corrected_text.replace('\u{001a}', ""); // Remove SUB (substitute) character
    let final_text = add_spacing_after_math_symbols(&cleaned_text);

    #[cfg(feature = "modtext")]
    {
        format!("<formula>{final_text}</formula>")
    }

    #[cfg(not(feature = "modtext"))]
    {
        final_text
    }
}

/// Process CharSpans into a flattened vector with line break detection
///
/// This shared function handles the common logic for both formula and text processing:
/// - Flattens line_spans into a single vector
/// - Detects y-coordinate jumps indicating line breaks
/// - Inserts semicolon separators for multi-line content
/// - Handles both simple and complex line break detection patterns
fn process_spans_with_line_breaks(
    line_spans: &[Vec<crate::entities::CharSpan>],
    use_complex_detection: bool,
) -> Vec<crate::entities::CharSpan> {
    let mut all_spans = Vec::new();

    let mut prev_y: Option<f32> = None;
    let mut prev_significant_y: Option<f32> = None;
    const LINE_BREAK_THRESHOLD: f32 = 10.0;
    const SMALL_SPAN_THRESHOLD: f32 = 5.0;

    for line in line_spans {
        for span in line {
            // Skip truly empty spans for text processing, but preserve space spans
            if !use_complex_detection && span.text.is_empty() {
                continue;
            }

            let current_y = span.bbox.y0;

            // Add semicolons as line break separators ONLY for formulas (use_complex_detection=true)
            if use_complex_detection {
                let should_insert_semicolon = if let Some(py) = prev_y {
                    let y_diff = current_y - py;
                    if y_diff > LINE_BREAK_THRESHOLD {
                        true
                    } else if let Some(sig_y) = prev_significant_y {
                        let total_diff = current_y - sig_y;
                        y_diff > SMALL_SPAN_THRESHOLD && total_diff > LINE_BREAK_THRESHOLD
                    } else {
                        false
                    }
                } else {
                    false
                };

                if should_insert_semicolon {
                    let separator_span = crate::entities::CharSpan {
                        text: "; ".to_string(),
                        bbox: span.bbox.clone(),
                        rotation: span.rotation,
                        font_name: span.font_name.clone(),
                        font_size: span.font_size,
                        font_weight: span.font_weight,
                        char_start_idx: span.char_start_idx,
                        char_end_idx: span.char_end_idx,
                        original_unicode: None,
                        has_corruption: false,
                    };
                    all_spans.push(separator_span);

                    let ref_y = prev_significant_y.unwrap_or(prev_y.unwrap());
                    let total_diff = current_y - ref_y;
                    debug_print!("➕ FORMULA LINE BREAK: Added semicolon separator for y-jump {ref_y:.1} -> {current_y:.1} (diff: +{total_diff:.1})");
                }
            }

            all_spans.push(span.clone());

            // Update tracking variables for formula processing
            if use_complex_detection {
                prev_y = Some(current_y);
                // Update significant y-position for non-punctuation spans
                if !span.text.trim().is_empty()
                    && span.text.trim() != "."
                    && span.text.trim() != ","
                {
                    prev_significant_y = Some(current_y);
                }
            }
        }
    }

    all_spans
}

/// Format mathematical formula text with CharSpan-based subscript/superscript detection
///
/// This function provides proper subscript/superscript detection by processing the
/// original CharSpans that contain positioning and font information. This is more
/// accurate than pattern-based detection as it uses actual PDF positioning data.
pub fn format_formula_with_spans(
    text: &str,
    line_spans: &[Vec<crate::entities::CharSpan>],
) -> String {
    debug_print!(
        "📋 FORMULA WITH SPANS: Processing formula text with {} line(s) of spans: {}",
        line_spans.len(),
        text.chars().take(100).collect::<String>()
    );

    // Use shared span processing with complex line break detection
    let all_spans = process_spans_with_line_breaks(line_spans, true);

    debug_print!(
        "📋 FORMULA WITH SPANS: Flattened to {} total spans",
        all_spans.len()
    );

    if all_spans.is_empty() {
        debug_print!(
            "📋 FORMULA WITH SPANS: No spans available, falling back to text-only processing"
        );
        return format_formula_text(text);
    }

    // Apply subscript/superscript detection using the actual CharSpans
    let script_processed = add_tags(&all_spans);

    debug_print!(
        "📋 FORMULA WITH SPANS: Script processing result: '{}'",
        script_processed.chars().take(100).collect::<String>()
    );

    // Apply the same enhancement pipeline as format_formula_text

    // Step 1: Normalize mathematical Unicode variants to ASCII
    let normalized_text = mathematical::normalize_mathematical_unicode(&script_processed);
    debug_print!(
        "📋 FORMULA WITH SPANS STEP 1 (Unicode normalize): '{}' → '{}'",
        script_processed.chars().take(50).collect::<String>(),
        normalized_text.chars().take(50).collect::<String>()
    );

    // Step 2: Combine mathematical diacritical marks
    let combined_text = mathematical::combine_mathematical_accents(&normalized_text);
    debug_print!(
        "📋 FORMULA WITH SPANS STEP 2 (Combine accents): '{}' → '{}'",
        normalized_text.chars().take(50).collect::<String>(),
        combined_text.chars().take(50).collect::<String>()
    );

    // Step 3: Standardize subscript/superscript notation (this may have minimal effect since add_tags already handles this)
    let standardized_text = combined_text.clone(); // Note: Script standardization now handled by script_notation module
    debug_print!(
        "📋 FORMULA WITH SPANS STEP 3 (Standardize scripts): '{}' → '{}'",
        combined_text.chars().take(50).collect::<String>(),
        standardized_text.chars().take(50).collect::<String>()
    );

    // Step 4: Apply mathematical symbol corrections to the processed text
    #[cfg(feature = "correction-engine")]
    let corrected_text = {
        use crate::font_analysis::text_corrections::fix_math_symbol_corruptions;
        let fixed = fix_math_symbol_corruptions(&standardized_text);
        debug_print!(
            "📋 FORMULA WITH SPANS STEP 4 (Symbol corrections): '{}' → '{}'",
            standardized_text.chars().take(50).collect::<String>(),
            fixed.chars().take(50).collect::<String>()
        );
        fixed
    };

    #[cfg(not(feature = "correction-engine"))]
    let corrected_text = standardized_text;

    // Clean up substitute characters and apply spacing
    let cleaned_text = corrected_text.replace('\u{001a}', ""); // Remove SUB (substitute) character
    let final_text = add_spacing_after_math_symbols(&cleaned_text);

    // Wrap with formula tags
    #[cfg(feature = "modtext")]
    {
        let result = format!("<formula>{final_text}</formula>");
        debug_print!(
            "📋 FORMULA WITH SPANS FINAL: '{}'",
            result.chars().take(150).collect::<String>()
        );
        result
    }

    #[cfg(not(feature = "modtext"))]
    {
        debug_print!(
            "📋 FORMULA WITH SPANS FINAL: '{}'",
            final_text.chars().take(150).collect::<String>()
        );
        final_text
    }
}

/// Process text with CharSpans for subscript detection but without formula wrapper
/// This is like format_formula_with_spans but returns plain text for TEXT elements
pub fn process_text_with_spans(
    text: &str,
    line_spans: &[Vec<crate::entities::CharSpan>],
) -> String {
    debug_print!(
        "📋 TEXT WITH SPANS: Processing text with {} line(s) of spans: {}",
        line_spans.len(),
        text.chars().take(100).collect::<String>()
    );

    // Apply dictionary correction to clean text FIRST, before any HTML processing
    let corrected_text = crate::font_analysis::correct_assembled_text(text);
    debug_print!(
        "📋 TEXT WITH SPANS: Dictionary corrected: '{}' → '{}'",
        text.chars().take(50).collect::<String>(),
        corrected_text.chars().take(50).collect::<String>()
    );

    // Use shared span processing with simple line break detection for text
    let mut all_spans = process_spans_with_line_breaks(line_spans, false);

    debug_print!(
        "📋 TEXT WITH SPANS: Flattened to {} total spans",
        all_spans.len()
    );

    // Apply hyphen removal at span level BEFORE HTML processing
    // This fixes line-ending hyphens like "Vi-" + "jil" → "Vijil" at the source
    remove_line_ending_hyphens(&mut all_spans);

    // Apply subscript/superscript detection using the spans (dictionary correction already applied to text)
    let script_processed = script_notation::apply_text_formatting(&all_spans);

    debug_print!(
        "📋 TEXT WITH SPANS: Script processing result: '{}'",
        script_processed.chars().take(100).collect::<String>()
    );

    // Dictionary correction already applied to input text - use processed result
    let final_text = script_processed;

    // Clean up substitute characters
    let cleaned_text = final_text.replace('\u{001a}', "");

    debug_print!(
        "📋 TEXT WITH SPANS FINAL: '{}'",
        cleaned_text.chars().take(100).collect::<String>()
    );

    cleaned_text
}

/// Add spacing around mathematical symbols when no space exists
///
/// This function ensures proper readability by adding spaces before and after mathematical
/// symbols like ∈, ∪, ∩, etc. when they are immediately preceded or followed by text.
/// Also handles mathematical summation X operators.
fn add_spacing_after_math_symbols(text: &str) -> String {
    let math_symbols = [
        '∈', '∉', '∪', '∩', '∅', '⊂', '⊃', '⊆', '⊇', '∀', '∃', '∄', '∧', '∨', '¬', '→', '←', '↔',
        '⇒', '⇐', '⇔', '∴', '∵', '≡', '≢', '≤', '≥', '≠', '≈', '≅', '≃', '∼', '∝', '±', '∓', '×',
        '÷', '∗', '∘', '√', '∛', '∜', '∞', '∑', '∏', '∫', '∮', '∂', '∇', '△', '∠', '⊥', '∥', '≼',
        '≽', '⊑', '⊒', '⊥', '⊤',
    ];

    // First pass: handle mathematical X summation operators
    let temp_result = handle_mathematical_x_operators(text);

    let mut result = String::new();
    let chars: Vec<char> = temp_result.chars().collect();

    for i in 0..chars.len() {
        let current_char = chars[i];

        // Check if current character is a mathematical symbol
        if math_symbols.contains(&current_char) {
            // Check if we need space before the symbol
            if i > 0 {
                let prev_char = chars[i - 1];
                let last_result_char = result.chars().last();
                if let Some(last_char) = last_result_char {
                    if !last_char.is_whitespace() && !is_tag_char(prev_char) && prev_char != '(' {
                        result.push(' ');
                        debug_print!("➕ MATH SPACING: Added space before '{current_char}' after '{prev_char}'");
                    }
                }
            }

            result.push(current_char);

            // Check if we need space after the symbol
            if i + 1 < chars.len() {
                let next_char = chars[i + 1];
                if !next_char.is_whitespace()
                    && next_char != ','
                    && next_char != '.'
                    && next_char != ';'
                    && next_char != ')'
                    && !is_tag_char(next_char)
                {
                    result.push(' ');
                    debug_print!(
                        "➕ MATH SPACING: Added space after '{current_char}' before '{next_char}'"
                    );
                }
            }
        } else {
            result.push(current_char);
        }
    }

    result
}

/// Handle mathematical X operators (summation/product) by adding proper spacing
/// This function specifically looks for patterns like "Xn" or "X<sub>" where X represents summation
fn handle_mathematical_x_operators(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let current_char = chars[i];

        // Check for mathematical X summation pattern
        if current_char == 'X' && i + 1 < chars.len() {
            let next_char = chars[i + 1];

            // Check if this looks like a mathematical summation:
            // - X followed by lowercase letter (variable like 'n', 'm', etc.)
            // - X followed by '<' (subscripted variable like 'X<sub>')
            let is_math_summation = next_char.is_lowercase() || next_char == '<';

            // Additional context check: look for mathematical context indicators
            let has_math_context = text.contains("∈")
                || text.contains("∉")
                || text.contains("<sub>")
                || text.contains("<sup>")
                || text.contains("=");

            if is_math_summation && has_math_context {
                result.push('X');
                result.push(' '); // Add space after X
                debug_print!(
                    "➕ MATH X SPACING: Added space after summation X before '{next_char}'"
                );
            } else {
                result.push(current_char);
            }
        } else {
            result.push(current_char);
        }

        i += 1;
    }

    result
}

/// Helper function to check if character is part of HTML tag
fn is_tag_char(c: char) -> bool {
    c == '<' || c == '>' || c == '/'
}

/// Determine if two word parts likely form a compound word that should keep its hyphen
///
/// This function identifies common compound word patterns to prevent incorrect hyphen removal.
/// Returns true if the hyphen should be preserved, false if it's likely a line-ending artifact.
fn is_likely_compound_word(word_before: &str, word_after: &str) -> bool {
    let before_lower = word_before.to_lowercase();
    let after_lower = word_after.to_lowercase();

    // Common compound word patterns
    let compound_patterns = [
        // Technical/academic compounds
        ("state", "of"),
        ("art", "of"),
        ("up", "to"),
        ("well", "known"),
        ("well", "established"),
        ("high", "quality"),
        ("high", "performance"),
        ("low", "level"),
        ("high", "level"),
        ("real", "time"),
        ("real", "world"),
        ("large", "scale"),
        ("small", "scale"),
        ("fine", "tuned"),
        ("pre", "trained"),
        ("multi", "layer"),
        ("multi", "head"),
        ("cross", "attention"),
        ("self", "attention"),
        ("end", "to"),
        ("to", "end"),
        // Common prefixes that form compounds
        ("non", ""),
        ("pre", ""),
        ("post", ""),
        ("anti", ""),
        ("pro", ""),
        ("semi", ""),
        ("multi", ""),
        ("inter", ""),
        ("intra", ""),
        ("extra", ""),
        ("ultra", ""),
        ("super", ""),
        ("sub", ""),
        ("over", ""),
        ("under", ""),
        ("out", ""),
        // Common academic/technical suffixes
        ("", "based"),
        ("", "driven"),
        ("", "aware"),
        ("", "specific"),
        ("", "related"),
        ("", "oriented"),
        ("", "focused"),
        ("", "free"),
        ("", "like"),
        ("", "wide"),
    ];

    // Check exact pattern matches
    for (first, second) in compound_patterns.iter() {
        if (first.is_empty() || before_lower.ends_with(first))
            && (second.is_empty() || after_lower.starts_with(second))
        {
            return true;
        }
    }

    // Additional heuristics for compound words:

    // 1. Both parts are short (likely compound elements)
    if word_before.len() <= 4 && word_after.len() <= 4 {
        return true;
    }

    // 2. Pattern like "X-of-Y" compounds (state-of-the-art)
    if after_lower.starts_with("of")
        || after_lower.starts_with("the")
        || after_lower.starts_with("a")
    {
        return true;
    }

    // 3. Technical prefixes
    let tech_prefixes = [
        "AI", "ML", "NLP", "LLM", "API", "GUI", "CLI", "HTTP", "HTTPS", "JSON", "XML",
    ];
    if tech_prefixes
        .iter()
        .any(|&prefix| before_lower == prefix.to_lowercase())
    {
        return true;
    }

    // 4. Numbers followed by units or descriptors (not usually line breaks)
    if word_before.chars().any(|c| c.is_numeric()) && word_after.len() <= 8 {
        return true;
    }

    false
}
