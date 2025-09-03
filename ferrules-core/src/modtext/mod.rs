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

#[cfg(feature = "modtext")]
pub mod mathematical;

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
    eprintln!(
        "🟢 process_mathematical_notation CALLED with {} spans",
        spans.len()
    );
    #[cfg(feature = "modtext")]
    {
        mathematical::detect_script_notation(spans)
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
    eprintln!("🏷️ add_tags CALLED with {} spans", spans.len());
    #[cfg(feature = "modtext")]
    {
        mathematical::apply_tags_recursive(spans, 0)
    }

    #[cfg(not(feature = "modtext"))]
    {
        // When feature is disabled, just concatenate the text without processing
        spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Detect inline subscript patterns within text
///
/// When the `modtext` feature is disabled, this returns None.
#[allow(dead_code)]
pub fn detect_inline_subscript_pattern(_text: &str) -> Option<(String, String)> {
    #[cfg(feature = "modtext")]
    {
        mathematical::detect_inline_subscript(_text)
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
    eprintln!(
        "📋 FORMULA: format_formula_text called with: {}",
        text.chars().take(100).collect::<String>()
    );

    // Step 1: Normalize mathematical Unicode variants to ASCII
    let normalized_text = normalize_mathematical_unicode(text);
    eprintln!(
        "📋 FORMULA STEP 1 (Unicode normalize): '{}' → '{}'",
        text.chars().take(50).collect::<String>(),
        normalized_text.chars().take(50).collect::<String>()
    );

    // Step 2: Combine mathematical diacritical marks
    let combined_text = combine_mathematical_accents(&normalized_text);
    eprintln!(
        "📋 FORMULA STEP 2 (Combine accents): '{}' → '{}'",
        normalized_text.chars().take(50).collect::<String>(),
        combined_text.chars().take(50).collect::<String>()
    );

    // Step 3: Standardize subscript/superscript notation
    let standardized_text = standardize_script_notation(&combined_text);
    eprintln!(
        "📋 FORMULA STEP 3 (Standardize scripts): '{}' → '{}'",
        combined_text.chars().take(50).collect::<String>(),
        standardized_text.chars().take(50).collect::<String>()
    );

    // Step 4: Apply mathematical symbol corrections to fix patterns like "6=" → "≠", "∈/" → "∉"
    // Control character corrections are now applied at CharSpan level
    #[cfg(feature = "correction-engine")]
    let corrected_text = {
        use crate::correction::character::fix_math_symbol_corruptions;
        let fixed = fix_math_symbol_corruptions(&standardized_text);
        eprintln!(
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

/// Format mathematical formula text with CharSpan-based subscript/superscript detection
///
/// This function provides proper subscript/superscript detection by processing the
/// original CharSpans that contain positioning and font information. This is more
/// accurate than pattern-based detection as it uses actual PDF positioning data.
pub fn format_formula_with_spans(
    text: &str,
    line_spans: &[Vec<crate::entities::CharSpan>],
) -> String {
    eprintln!(
        "📋 FORMULA WITH SPANS: Processing formula text with {} line(s) of spans: {}",
        line_spans.len(),
        text.chars().take(100).collect::<String>()
    );

    // Process spans while detecting significant y-coordinate jumps (line breaks)
    // and insert semicolon separators for multi-line formulas
    let mut all_spans = Vec::new();
    let mut prev_y: Option<f32> = None;
    let mut prev_significant_y: Option<f32> = None; // Track y-position of last significant span
    const LINE_BREAK_THRESHOLD: f32 = 10.0; // Significant y-coordinate jump indicates new line
    const SMALL_SPAN_THRESHOLD: f32 = 5.0; // Small y-jumps that might be punctuation

    for line in line_spans {
        for span in line {
            let current_y = span.bbox.y0;

            // Check for significant downward y-coordinate jump indicating a new line
            // (In PDF coordinates, larger y values are typically lower on the page)
            if let Some(py) = prev_y {
                let y_diff = current_y - py;

                // For small jumps, also check against the last significant position
                let should_insert_semicolon = if y_diff > LINE_BREAK_THRESHOLD {
                    true
                } else if let Some(sig_y) = prev_significant_y {
                    let total_diff = current_y - sig_y;
                    y_diff > SMALL_SPAN_THRESHOLD && total_diff > LINE_BREAK_THRESHOLD
                } else {
                    false
                };

                if should_insert_semicolon {
                    // Only add semicolon for downward jumps (to next line)
                    let mut separator_span = span.clone();
                    separator_span.text = "; ".to_string();
                    all_spans.push(separator_span);
                    let ref_y = prev_significant_y.unwrap_or(py);
                    let total_diff = current_y - ref_y;
                    eprintln!("➕ LINE BREAK: Added semicolon separator for y-jump {ref_y:.1} -> {current_y:.1} (diff: +{total_diff:.1})");
                    prev_significant_y = Some(current_y);
                }
            }

            all_spans.push(span.clone());
            prev_y = Some(current_y);

            // Update significant y-position for non-punctuation spans
            if !span.text.trim().is_empty() && span.text.trim() != "." && span.text.trim() != "," {
                prev_significant_y = Some(current_y);
            }
        }
    }

    eprintln!(
        "📋 FORMULA WITH SPANS: Flattened to {} total spans",
        all_spans.len()
    );

    if all_spans.is_empty() {
        eprintln!(
            "📋 FORMULA WITH SPANS: No spans available, falling back to text-only processing"
        );
        return format_formula_text(text);
    }

    // Apply subscript/superscript detection using the actual CharSpans
    let script_processed = add_tags(&all_spans);

    eprintln!(
        "📋 FORMULA WITH SPANS: Script processing result: '{}'",
        script_processed.chars().take(100).collect::<String>()
    );

    // Apply the same enhancement pipeline as format_formula_text

    // Step 1: Normalize mathematical Unicode variants to ASCII
    let normalized_text = normalize_mathematical_unicode(&script_processed);
    eprintln!(
        "📋 FORMULA WITH SPANS STEP 1 (Unicode normalize): '{}' → '{}'",
        script_processed.chars().take(50).collect::<String>(),
        normalized_text.chars().take(50).collect::<String>()
    );

    // Step 2: Combine mathematical diacritical marks
    let combined_text = combine_mathematical_accents(&normalized_text);
    eprintln!(
        "📋 FORMULA WITH SPANS STEP 2 (Combine accents): '{}' → '{}'",
        normalized_text.chars().take(50).collect::<String>(),
        combined_text.chars().take(50).collect::<String>()
    );

    // Step 3: Standardize subscript/superscript notation (this may have minimal effect since add_tags already handles this)
    let standardized_text = standardize_script_notation(&combined_text);
    eprintln!(
        "📋 FORMULA WITH SPANS STEP 3 (Standardize scripts): '{}' → '{}'",
        combined_text.chars().take(50).collect::<String>(),
        standardized_text.chars().take(50).collect::<String>()
    );

    // Step 4: Apply mathematical symbol corrections to the processed text
    #[cfg(feature = "correction-engine")]
    let corrected_text = {
        use crate::correction::character::fix_math_symbol_corruptions;
        let fixed = fix_math_symbol_corruptions(&standardized_text);
        eprintln!(
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
        eprintln!(
            "📋 FORMULA WITH SPANS FINAL: '{}'",
            result.chars().take(150).collect::<String>()
        );
        result
    }

    #[cfg(not(feature = "modtext"))]
    {
        eprintln!(
            "📋 FORMULA WITH SPANS FINAL: '{}'",
            final_text.chars().take(150).collect::<String>()
        );
        final_text
    }
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
                        eprintln!("➕ MATH SPACING: Added space before '{current_char}' after '{prev_char}'");
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
                    eprintln!(
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
                || text.contains("Loss")
                || text.contains("=");

            if is_math_summation && has_math_context {
                result.push('X');
                result.push(' '); // Add space after X
                eprintln!("➕ MATH X SPACING: Added space after summation X before '{next_char}'");
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

/// Combine mathematical diacritical marks with their base characters
///
/// This function detects mathematical accents that appear as separate characters
/// and combines them properly with their base characters using Unicode combining marks.
///
/// # Examples
/// - `rˆi` → `r̂i` (circumflex combined with r)
/// - `x˜n` → `x̃n` (tilde combined with x)
/// - `A‾B` → `ĀB` (overline combined with A)
fn combine_mathematical_accents(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let current_char = chars[i];

        // Check if the next character is a mathematical accent that should combine
        if i + 1 < chars.len() {
            let next_char = chars[i + 1];

            match next_char {
                // Circumflex/Hat: ˆ (U+02C6) → combining circumflex (U+0302)
                'ˆ' | '^' => {
                    result.push(current_char);
                    result.push('\u{0302}'); // Combining circumflex accent
                    eprintln!("🔄 ACCENT: Combined '{current_char}ˆ' → '{current_char}̂'");
                    i += 2; // Skip both characters
                    continue;
                }
                // Tilde: ˜ (U+02DC) → combining tilde (U+0303)
                '˜' | '~' => {
                    result.push(current_char);
                    result.push('\u{0303}'); // Combining tilde
                    eprintln!("🔄 ACCENT: Combined '{current_char}˜' → '{current_char}̃'");
                    i += 2; // Skip both characters
                    continue;
                }
                // Overline/Bar: ‾ (U+203E) → combining overline (U+0305)
                '‾' | '¯' => {
                    result.push(current_char);
                    result.push('\u{0305}'); // Combining overline
                    eprintln!("🔄 ACCENT: Combined '{current_char}‾' → '{current_char}̄'");
                    i += 2; // Skip both characters
                    continue;
                }
                // Dot above: · (U+00B7) → combining dot above (U+0307)
                '·' if current_char.is_alphabetic() => {
                    result.push(current_char);
                    result.push('\u{0307}'); // Combining dot above
                    eprintln!("🔄 ACCENT: Combined '{current_char}·' → '{current_char}̇'");
                    i += 2; // Skip both characters
                    continue;
                }
                _ => {}
            }
        }

        // No combining pattern found, just add the current character
        result.push(current_char);
        i += 1;
    }

    result
}

/// Convert mathematical Unicode variants to regular ASCII characters
///
/// Mathematical Unicode characters (like those from Mathematical Alphanumeric Symbols)
/// are converted to their ASCII equivalents for better compatibility and readability.
fn normalize_mathematical_unicode(text: &str) -> String {
    let mut result = String::new();

    for c in text.chars() {
        let normalized = match c {
            // Mathematical Script/Italic variants → ASCII
            '𝑟' | '𝒓' | '𝓇' | '𝓻' | '𝔯' | '𝕣' | '𝖗' | '𝗋' | '𝗿' | '𝘳' | '𝙧' => {
                'r'
            }
            '𝑞' | '𝒒' | '𝓆' | '𝓺' | '𝔮' | '𝕢' | '𝖖' | '𝗊' | '𝗾' | '𝘲' | '𝙦' => {
                'q'
            }
            '𝑖' | '𝒊' | '𝒾' | '𝓲' | '𝔦' | '𝕚' | '𝖎' | '𝗂' | '𝗶' | '𝘪' | '𝙞' => {
                'i'
            }
            '𝑘' | '𝒌' | '𝓀' | '𝓴' | '𝔨' | '𝕜' | '𝖐' | '𝗄' | '𝗸' | '𝘬' | '𝙠' => {
                'k'
            }
            '𝑗' | '𝒋' | '𝒿' | '𝓳' | '𝔧' | '𝕛' | '𝖏' | '𝗃' | '𝗷' | '𝘫' | '𝙟' => {
                'j'
            }
            '𝑛' | '𝒏' | '𝓃' | '𝓷' | '𝔫' | '𝕟' | '𝖓' | '𝗇' | '𝗻' | '𝘯' | '𝙣' => {
                'n'
            }
            '𝑚' | '𝒎' | '𝓂' | '𝓶' | '𝔪' | '𝕞' | '𝖒' | '𝗆' | '𝗺' | '𝘮' | '𝙢' => {
                'm'
            }
            '𝑥' | '𝒙' | '𝓍' | '𝔁' | '𝔵' | '𝕩' | '𝖝' | '𝗑' | '𝘅' | '𝘹' | '𝙭' => {
                'x'
            }
            '𝑦' | '𝒚' | '𝓎' | '𝔂' | '𝔶' | '𝕪' | '𝖞' | '𝗒' | '𝘆' | '𝘺' | '𝙮' => {
                'y'
            }
            '𝑧' | '𝒛' | '𝓏' | '𝔃' | '𝔷' | '𝕫' | '𝖟' | '𝗓' | '𝘇' | '𝘻' | '𝙯' => {
                'z'
            }
            // Mathematical Bold/Script uppercase
            '𝐀' | '𝑨' | '𝒜' | '𝓐' | '𝔄' | '𝔸' | '𝕬' | '𝖠' | '𝗔' | '𝘈' | '𝘼' | '𝙰' => {
                'A'
            }
            '𝐁' | '𝑩' | '𝒷' | '𝓑' | '𝔅' | '𝔹' | '𝕭' | '𝖡' | '𝗕' | '𝘉' | '𝘽' | '𝙱' => {
                'B'
            }
            '𝐂' | '𝑪' | '𝒸' | '𝓒' | '𝔆' | '𝖢' | '𝗖' | '𝘊' | '𝘾' | '𝙲' => {
                'C'
            }
            '𝐃' | '𝑫' | '𝒹' | '𝓓' | '𝔇' | '𝔻' | '𝕯' | '𝖣' | '𝗗' | '𝘋' | '𝘿' | '𝙳' => {
                'D'
            }
            '𝐄' | '𝑬' | '𝒺' | '𝓔' | '𝔈' | '𝔼' | '𝕰' | '𝖤' | '𝗘' | '𝘌' | '𝙀' | '𝙴' => {
                'E'
            }
            '𝑀' | '𝑴' | '𝒩' | '𝓜' | '𝔐' | '𝕸' | '𝖬' | '𝗠' | '𝘔' | '𝙈' | '𝙼' => {
                'M'
            }
            '𝑃' | '𝑷' | '𝒫' | '𝓟' | '𝔓' | '𝕻' | '𝖯' | '𝗣' | '𝘗' | '𝙋' | '𝙿' => {
                'P'
            }
            '𝑄' | '𝑸' | '𝒬' | '𝓠' | '𝔔' | '𝕼' | '𝖰' | '𝗤' | '𝘘' | '𝙌' | '𝚀' => {
                'Q'
            }
            // Keep regular characters as-is
            _ => c,
        };

        if normalized != c {
            eprintln!("🔄 UNICODE: Normalized '{c}' → '{normalized}'");
        }

        result.push(normalized);
    }

    result
}

/// Standardize subscript and superscript notation to HTML tags
///
/// Converts Unicode subscript/superscript characters to HTML tags for consistency.
/// This ensures all mathematical notation uses the same format.
fn standardize_script_notation(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let current_char = chars[i];

        match current_char {
            // Unicode subscripts → <sub> tags
            '₀' | '₁' | '₂' | '₃' | '₄' | '₅' | '₆' | '₇' | '₈' | '₉' => {
                let digit = match current_char {
                    '₀' => '0',
                    '₁' => '1',
                    '₂' => '2',
                    '₃' => '3',
                    '₄' => '4',
                    '₅' => '5',
                    '₆' => '6',
                    '₇' => '7',
                    '₈' => '8',
                    '₉' => '9',
                    _ => current_char,
                };
                result.push_str(&format!("<sub>{digit}</sub>"));
                eprintln!("🔄 SUBSCRIPT: '{current_char}' → '<sub>{digit}</sub>'");
                i += 1;
                continue;
            }
            'ₐ' | 'ₑ' | 'ᵢ' | 'ⱼ' | 'ₖ' | 'ₗ' | 'ₘ' | 'ₙ' | 'ₒ' | 'ₚ' | 'ᵣ' | 'ₛ' | 'ₜ' | 'ᵤ'
            | 'ᵥ' | 'ₓ' => {
                let letter = match current_char {
                    'ₐ' => 'a',
                    'ₑ' => 'e',
                    'ᵢ' => 'i',
                    'ⱼ' => 'j',
                    'ₖ' => 'k',
                    'ₗ' => 'l',
                    'ₘ' => 'm',
                    'ₙ' => 'n',
                    'ₒ' => 'o',
                    'ₚ' => 'p',
                    'ᵣ' => 'r',
                    'ₛ' => 's',
                    'ₜ' => 't',
                    'ᵤ' => 'u',
                    'ᵥ' => 'v',
                    'ₓ' => 'x',
                    _ => current_char,
                };
                result.push_str(&format!("<sub>{letter}</sub>"));
                eprintln!("🔄 SUBSCRIPT: '{current_char}' → '<sub>{letter}</sub>'");
                i += 1;
                continue;
            }
            // Unicode superscripts → <sup> tags
            '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹' => {
                let digit = match current_char {
                    '⁰' => '0',
                    '¹' => '1',
                    '²' => '2',
                    '³' => '3',
                    '⁴' => '4',
                    '⁵' => '5',
                    '⁶' => '6',
                    '⁷' => '7',
                    '⁸' => '8',
                    '⁹' => '9',
                    _ => current_char,
                };
                result.push_str(&format!("<sup>{digit}</sup>"));
                eprintln!("🔄 SUPERSCRIPT: '{current_char}' → '<sup>{digit}</sup>'");
                i += 1;
                continue;
            }
            'ᵃ' | 'ᵇ' | 'ᶜ' | 'ᵈ' | 'ᵉ' | 'ᶠ' | 'ᵍ' | 'ʰ' | 'ⁱ' | 'ʲ' | 'ᵏ' | 'ˡ' | 'ᵐ' | 'ⁿ'
            | 'ᵒ' | 'ᵖ' | 'ʳ' | 'ˢ' | 'ᵗ' | 'ᵘ' | 'ᵛ' | 'ʷ' | 'ˣ' | 'ʸ' | 'ᶻ' => {
                let letter = match current_char {
                    'ᵃ' => 'a',
                    'ᵇ' => 'b',
                    'ᶜ' => 'c',
                    'ᵈ' => 'd',
                    'ᵉ' => 'e',
                    'ᶠ' => 'f',
                    'ᵍ' => 'g',
                    'ʰ' => 'h',
                    'ⁱ' => 'i',
                    'ʲ' => 'j',
                    'ᵏ' => 'k',
                    'ˡ' => 'l',
                    'ᵐ' => 'm',
                    'ⁿ' => 'n',
                    'ᵒ' => 'o',
                    'ᵖ' => 'p',
                    'ʳ' => 'r',
                    'ˢ' => 's',
                    'ᵗ' => 't',
                    'ᵘ' => 'u',
                    'ᵛ' => 'v',
                    'ʷ' => 'w',
                    'ˣ' => 'x',
                    'ʸ' => 'y',
                    'ᶻ' => 'z',
                    _ => current_char,
                };
                result.push_str(&format!("<sup>{letter}</sup>"));
                eprintln!("🔄 SUPERSCRIPT: '{current_char}' → '<sup>{letter}</sup>'");
                i += 1;
                continue;
            }
            // Keep other characters as-is
            _ => {
                result.push(current_char);
                i += 1;
            }
        }
    }

    result
}
