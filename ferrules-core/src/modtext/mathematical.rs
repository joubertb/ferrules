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

/// Configuration for subscript detection thresholds
///
/// This structure allows for easy tuning of the detection algorithm based on
/// different PDF sources and requirements. Values can be adjusted based on
/// debug analysis results.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct SubscriptDetectionConfig {
    /// Minimum absolute baseline shift (in points) to consider subscript detection
    pub absolute_baseline_threshold: f32,

    /// Maximum font size ratio (current/base) for text to be considered subscript
    /// Default: 0.85 means font must be 85% or smaller than base font
    pub font_size_ratio_threshold: f32,

    /// Minimum relative baseline shift (as fraction of base font size)
    /// Default: 0.3 means shift must be 30% or more of base font size
    pub relative_baseline_threshold: f32,

    /// Whether both font size AND baseline shift conditions must be met
    /// true = AND logic (both conditions required)
    /// false = OR logic (either condition sufficient)
    pub use_and_logic: bool,

    /// Threshold for returning to baseline to close subscript tags
    pub return_threshold: f32,
}

impl Default for SubscriptDetectionConfig {
    fn default() -> Self {
        Self {
            absolute_baseline_threshold: 3.0,
            font_size_ratio_threshold: 0.85,
            relative_baseline_threshold: 0.3,
            use_and_logic: true,
            return_threshold: 2.0,
        }
    }
}

impl SubscriptDetectionConfig {
    /// Create a more permissive configuration for PDFs with subtle subscripts
    #[allow(dead_code)]
    pub fn permissive() -> Self {
        Self {
            absolute_baseline_threshold: 2.0,
            font_size_ratio_threshold: 0.95,
            relative_baseline_threshold: 0.15,
            use_and_logic: false, // OR logic - either condition works
            return_threshold: 1.5,
        }
    }

    /// Create a strict configuration for PDFs with clear subscript formatting
    #[allow(dead_code)]
    pub fn strict() -> Self {
        Self {
            absolute_baseline_threshold: 4.0,
            font_size_ratio_threshold: 0.75,
            relative_baseline_threshold: 0.4,
            use_and_logic: true, // AND logic - both conditions required
            return_threshold: 3.0,
        }
    }
}

/// Check if this baseline/font change represents a real superscript based on positioning and font metrics
fn is_real_superscript(current_span: &CharSpan, baseline_diff: f32, base_font_size: f32) -> bool {
    // Real superscripts should have:
    // 1. Significant font size reduction (typically 70% or smaller of base text)
    // 2. UPWARD baseline shift (negative baseline_diff)
    // 3. Baseline shift that's proportional to font size

    // Superscripts must have negative baseline shift (upward movement)
    if baseline_diff >= 0.0 {
        return false;
    }

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

/// Analyze all spans for potential subscripts and generate comprehensive debug report
fn analyze_potential_subscripts(spans: &[CharSpan], base_font_size: f32, baseline: f32) {
    eprintln!("\n=== COMPREHENSIVE SUBSCRIPT ANALYSIS ===");
    eprintln!("Base Font Size: {base_font_size:.1}, Base Baseline: {baseline:.1}");
    eprintln!("Total Spans: {}", spans.len());

    let mut potential_subscripts = Vec::new();
    let full_text: String = spans.iter().map(|s| s.text.as_str()).collect();

    eprintln!("\nFull Text: '{full_text}'");
    eprintln!("\n--- DETAILED SPAN ANALYSIS ---");

    for (i, span) in spans.iter().enumerate() {
        let text_trimmed = span.text.trim();
        if text_trimmed.is_empty() {
            continue;
        }

        // Calculate metrics
        let baseline_diff = span.bbox.y0 - baseline;
        let font_size_ratio = span.font_size / base_font_size;
        let relative_baseline_shift = baseline_diff.abs() / base_font_size;

        // Current detection result
        let current_detection = if baseline_diff.abs() > 3.0 {
            let has_smaller_font = font_size_ratio < 0.85;
            let has_significant_shift = relative_baseline_shift > 0.3;
            has_smaller_font && has_significant_shift
        } else {
            false
        };

        eprintln!("SPAN[{i}]: '{text_trimmed}'");
        eprintln!(
            "  Position: y={:.1}, baseline_diff={:.1} ({})",
            span.bbox.y0,
            baseline_diff,
            if baseline_diff > 0.0 {
                "DOWN"
            } else if baseline_diff < 0.0 {
                "UP"
            } else {
                "SAME"
            }
        );
        eprintln!(
            "  Font: size={:.1}, base={:.1}, ratio={:.3}",
            span.font_size, base_font_size, font_size_ratio
        );
        eprintln!(
            "  Metrics: abs_shift={:.1}, rel_shift={:.3} ({:.1}%)",
            baseline_diff.abs(),
            relative_baseline_shift,
            relative_baseline_shift * 100.0
        );
        eprintln!(
            "  Detection: current={}, font_ok={}, baseline_ok={}",
            if current_detection {
                "SUBSCRIPT"
            } else {
                "NORMAL"
            },
            if font_size_ratio < 0.85 { "YES" } else { "NO" },
            if relative_baseline_shift > 0.3 {
                "YES"
            } else {
                "NO"
            }
        );

        if current_detection {
            potential_subscripts.push((
                i,
                text_trimmed,
                baseline_diff,
                font_size_ratio,
                relative_baseline_shift,
                current_detection,
            ));
        }

        eprintln!();
    }

    if !potential_subscripts.is_empty() {
        eprintln!("\n=== DETECTED SUBSCRIPTS SUMMARY ===");
        eprintln!("| Idx | Char | Abs Shift | Rel Shift | Font Ratio | Detected |");
        eprintln!("|-----|------|-----------|-----------|------------|----------|");

        for (idx, text, baseline_diff, font_ratio, rel_shift, detected) in &potential_subscripts {
            eprintln!(
                "| {:3} | {:4} | {:9.1} | {:8.1}% | {:10.3} | {:8} |",
                idx,
                text,
                baseline_diff.abs(),
                rel_shift * 100.0,
                font_ratio,
                if *detected { "YES" } else { "NO" }
            );
        }
        eprintln!("Total detected subscripts: {}", potential_subscripts.len());
    }

    eprintln!("=== END ANALYSIS ===\n");
}

/// Local baseline-aware subscript detection for mathematical expressions with large baseline shifts
/// This function looks at local font sizes rather than global baseline for subscript detection
fn is_local_baseline_subscript(
    current_span: &CharSpan,
    baseline_diff: f32,
    base_font_size: f32,
    spans: &[CharSpan],
    current_index: usize,
) -> bool {
    let text_trimmed = current_span.text.trim();
    if text_trimmed.is_empty() {
        return false;
    }

    // Find nearby spans to establish local font context - use smaller window for more precise context
    let window_size = 2; // Look at 2 spans before and after for tighter context
    let start_idx = current_index.saturating_sub(window_size);
    let end_idx = (current_index + window_size + 1).min(spans.len());
    let context_spans = &spans[start_idx..end_idx];

    // Find font sizes in the local context and establish what should be "normal" vs "subscript"
    let mut font_sizes: Vec<f32> = context_spans
        .iter()
        .filter(|s| !s.text.trim().is_empty() && s.font_size > 1.0)
        .map(|s| s.font_size)
        .collect();

    if font_sizes.is_empty() {
        return false;
    }

    font_sizes.sort_by(|a, b| b.partial_cmp(a).unwrap()); // Sort descending

    // Use the second-largest font size as local baseline if available, otherwise largest
    // This handles cases where we have: 10pt(global) > 7pt(local baseline) > 5pt(subscript)
    let local_baseline_font_size = if font_sizes.len() >= 2 && font_sizes[0] >= base_font_size * 0.9
    {
        // If largest font is close to global baseline, use second largest as local baseline
        font_sizes[1]
    } else {
        // Otherwise use largest as local baseline
        font_sizes[0]
    };

    // Compare current font size to LOCAL baseline font size
    let local_font_ratio = current_span.font_size / local_baseline_font_size;
    let has_smaller_font_locally = local_font_ratio < 0.85;

    // Still require some baseline shift, but be more lenient with font size requirements
    let relative_baseline_shift = baseline_diff.abs() / base_font_size;
    let has_significant_shift = relative_baseline_shift > 0.25;

    let is_contextual_subscript = has_smaller_font_locally && has_significant_shift;

    eprintln!(
        "🎯 LOCAL BASELINE: '{}' font={:.1} local_baseline={:.1}({:.3}) global_baseline={:.1} shift={:.1}({:.3}) → {}",
        text_trimmed,
        current_span.font_size, local_baseline_font_size, local_font_ratio,
        base_font_size, baseline_diff.abs(), relative_baseline_shift,
        if is_contextual_subscript { "✓ LOCAL_SUBSCRIPT" } else { "✗ NOT_SUBSCRIPT" }
    );

    is_contextual_subscript
}

/// Check if this baseline/font change represents a real subscript based on positioning and font metrics
fn is_real_subscript(
    current_span: &CharSpan,
    baseline_diff: f32,
    base_font_size: f32,
    _spans: &[CharSpan],
    _current_index: usize,
) -> bool {
    // Real subscripts should have:
    // 1. Significant font size reduction (typically 70% or smaller of base text)
    // 2. DOWNWARD baseline shift (positive baseline_diff) - NOT upward!
    // 3. Baseline shift that's proportional to font size

    // Subscripts must have positive baseline shift (downward movement)
    // Upward movements should be handled by is_real_superscript
    if baseline_diff <= 0.0 {
        return false;
    }

    let font_size_ratio = current_span.font_size / base_font_size;
    let relative_baseline_shift = baseline_diff / base_font_size; // Use signed value for downward

    // Skip empty or whitespace-only text
    let text_trimmed = current_span.text.trim();
    if text_trimmed.is_empty() {
        return false;
    }

    // Real subscripts have smaller font AND significant DOWNWARD baseline shift
    let has_smaller_font = font_size_ratio < 0.85;
    let has_significant_shift = relative_baseline_shift > 0.25; // Downward shift is 25% of base font size

    let is_likely_subscript = has_smaller_font && has_significant_shift;

    // Enhanced debug logging with detailed rejection reasons
    let mut rejection_reasons = Vec::new();
    if !has_smaller_font {
        rejection_reasons.push(format!("font_too_large({font_size_ratio:.3}>0.85)"));
    }
    if !has_significant_shift {
        rejection_reasons.push(format!(
            "shift_too_small({relative_baseline_shift:.3}<0.25)"
        ));
    }
    if baseline_diff <= 0.0 {
        rejection_reasons.push(format!("upward_movement({baseline_diff:.1}<=0.0)"));
    }

    let decision_detail = if is_likely_subscript {
        "✓ REAL_SUBSCRIPT".to_string()
    } else if rejection_reasons.is_empty() {
        "? UNKNOWN_REJECT".to_string()
    } else {
        format!("✗ ARTIFACT ({})", rejection_reasons.join(", "))
    };

    eprintln!(
        "🔍 SUBSCRIPT DETAILED: '{}' font={:.1}/{:.1}({:.3}) baseline={:.1}({:.3}) thresholds=font<0.85&shift>0.25&downward → {}",
        text_trimmed,
        current_span.font_size, base_font_size, font_size_ratio,
        baseline_diff, relative_baseline_shift,
        decision_detail
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
/// Helper function to close a subscript or superscript tag, trimming trailing whitespace first
fn close_script_tag(result: &mut String, tag: &str) {
    if tag == "</sub>" || tag == "</sup>" {
        // Trim trailing whitespace before closing script tags
        *result = result.trim_end().to_string();
    }
    result.push_str(tag);
}

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
    let mut baseline: f32;
    let mut original_baseline: f32 = 0.0; // Store the original baseline reference
    let mut baseline_initialized = false;
    let mut current_bold = false;
    let mut in_subscript = false;
    let mut in_superscript = false;
    let mut base_font_size: f32 = 0.0;
    let mut subscript_baseline: f32 = 0.0; // Track baseline of current subscript region

    // REDUCED thresholds to catch smaller baseline shifts like ni/nj subscripts
    // Previous SCRIPT_THRESHOLD=3.0 was missing subscripts with baseline_diff=2.8
    const SCRIPT_THRESHOLD: f32 = 2.0; // Reduced from 3.0 to catch more subtle shifts
    const RETURN_THRESHOLD: f32 = 1.5; // Reduced proportionally
    const SUBSCRIPT_CONTINUITY_THRESHOLD: f32 = 3.0; // Allow ±3 points variation within subscript

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

        // Debug specific characters to understand mask splitting
        let text_trimmed = span.text.trim();
        if text_trimmed.contains('m')
            || text_trimmed.contains('a')
            || text_trimmed.contains('s')
            || text_trimmed.contains('k')
        {
            eprintln!(
                "🎯 MASK DEBUG: Found '{}' at y={:.1}, in_subscript={}",
                text_trimmed, span.bbox.y0, in_subscript
            );
        }

        // Initialize baseline and base font size on first span
        if !baseline_initialized {
            baseline = span.bbox.y0;
            original_baseline = span.bbox.y0; // Store the original baseline
            base_font_size = span.font_size;
            baseline_initialized = true;
            current_bold = is_bold_text(span);
            if current_bold {
                result.push_str("<b>");
                tag_stack.push("</b>");
                eprintln!("🅱️ BOLD START: Pushed </b> on stack");
            }
            eprintln!("📏 BASELINE INIT: {baseline:.1}, BASE FONT SIZE: {base_font_size:.1}");

            // Run comprehensive analysis after baseline is initialized
            analyze_potential_subscripts(spans, base_font_size, baseline);
        } else {
            let baseline_diff = span.bbox.y0 - original_baseline; // Use original baseline, not current
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

            // POSITIONING-BASED SUBSCRIPT CONTINUITY LOGIC
            // When we're already in subscript mode, check for continuity based purely on positioning
            if in_subscript {
                let _subscript_diff = (span.bbox.y0 - subscript_baseline).abs();
                let text_trimmed = span.text.trim();

                // Check if current character is also a subscript based on positioning
                let should_be_subscript = if baseline_diff.abs() > 10.0 {
                    // Large baseline shift: use local baseline-aware detection
                    is_local_baseline_subscript(span, baseline_diff, base_font_size, spans, i)
                } else {
                    // Normal case: use standard detection
                    is_real_subscript(span, baseline_diff, base_font_size, spans, i)
                };

                // Continue subscript only if:
                // 1. Within baseline continuity threshold AND
                // 2. Current character would also be detected as subscript OR
                // 3. Text is empty (whitespace spans)
                //
                // For positioning-based subscript continuity, we need to be stricter:
                // Only continue if the character is within the subscript baseline range AND
                // has a similar positioning profile to existing subscripts
                let baseline_diff_from_subscript = (span.bbox.y0 - subscript_baseline).abs();
                let has_very_small_font = span.font_size <= base_font_size * 0.65; // Much stricter font requirement
                let is_close_to_subscript_baseline =
                    baseline_diff_from_subscript <= SUBSCRIPT_CONTINUITY_THRESHOLD;

                let should_continue = (is_close_to_subscript_baseline
                    && (should_be_subscript || has_very_small_font))
                    || text_trimmed.is_empty();

                // Debug mask continuity decision
                if text_trimmed.contains('m')
                    || text_trimmed.contains('a')
                    || text_trimmed.contains('s')
                    || text_trimmed.contains('k')
                    || text_trimmed.contains('∈')
                    || text_trimmed.contains('N')
                {
                    eprintln!("🎯 CONTINUITY: '{text_trimmed}' baseline_diff_from_subscript={baseline_diff_from_subscript:.1}, should_be_subscript={should_be_subscript}, has_very_small_font={has_very_small_font}, should_continue={should_continue}");
                    eprintln!("🎯 POSITIONING: subscript_baseline={:.1}, span.y0={:.1}, global_baseline_diff={:.1}", 
                        subscript_baseline, span.bbox.y0, baseline_diff);
                }

                if should_continue {
                    // Continue subscript based on positioning
                    eprintln!("⬇️ SUBSCRIPT CONTINUE: Within continuity threshold ({baseline_diff_from_subscript:.1} <= {SUBSCRIPT_CONTINUITY_THRESHOLD})");
                    // Skip baseline change detection and just continue
                    let cleaned_text = span.text.replace('\u{001a}', ""); // Remove SUB (substitute) character
                    result.push_str(&cleaned_text);
                    continue;
                } else {
                    // Position indicates we should end subscript
                    if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sub>") {
                        let closing_tag = tag_stack.remove(pos);
                        close_script_tag(&mut result, closing_tag);
                        eprintln!("🔄 SUB CLOSE: Applied {closing_tag} - position indicates end of subscript");
                        in_subscript = false;
                    }
                }
            }

            // Check for baseline changes that might indicate scripts
            if baseline_diff.abs() > SCRIPT_THRESHOLD {
                // ENHANCED CONTEXT-AWARE SUBSCRIPT DETECTION
                // For mathematical contexts with large baseline shifts, use local font context
                let should_be_subscript = if baseline_diff.abs() > 10.0 {
                    // Large baseline shift: use local baseline-aware detection
                    is_local_baseline_subscript(span, baseline_diff, base_font_size, spans, i)
                } else {
                    // Normal case: use standard detection
                    is_real_subscript(span, baseline_diff, base_font_size, spans, i)
                };

                let should_be_superscript =
                    is_real_superscript(span, baseline_diff, base_font_size);

                // Only close existing script tags if the new character should be in a different mode
                if should_be_subscript && !in_subscript {
                    // Close superscript if open, then start subscript
                    if in_superscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sup>") {
                            let closing_tag = tag_stack.remove(pos);
                            close_script_tag(&mut result, closing_tag);
                            eprintln!("🔄 SUP CLOSE: Applied {closing_tag}");
                            in_superscript = false;
                        }
                    }
                    result.push_str("<sub>");
                    tag_stack.push("</sub>");
                    in_subscript = true;
                    subscript_baseline = span.bbox.y0; // Set subscript baseline for continuity checks
                    eprintln!("⬇️ SUBSCRIPT START: Real subscript detected");
                } else if should_be_superscript && !in_superscript {
                    // Close subscript if open, then start superscript
                    if in_subscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sub>") {
                            let closing_tag = tag_stack.remove(pos);
                            close_script_tag(&mut result, closing_tag);
                            eprintln!("🔄 SUB CLOSE: Applied {closing_tag}");
                            in_subscript = false;
                        }
                    }
                    result.push_str("<sup>");
                    tag_stack.push("</sup>");
                    in_superscript = true;
                    eprintln!("⬆️ SUPERSCRIPT START: Real superscript detected");
                } else if !should_be_subscript && !should_be_superscript {
                    // Character should be normal - close any open script tags
                    if in_subscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sub>") {
                            let closing_tag = tag_stack.remove(pos);
                            close_script_tag(&mut result, closing_tag);
                            eprintln!("🔄 SUB CLOSE: Applied {closing_tag}");
                            in_subscript = false;
                        }
                    }
                    if in_superscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sup>") {
                            let closing_tag = tag_stack.remove(pos);
                            close_script_tag(&mut result, closing_tag);
                            eprintln!("🔄 SUP CLOSE: Applied {closing_tag}");
                            in_superscript = false;
                        }
                    }
                } else if should_be_subscript && in_subscript {
                    eprintln!("⬇️ SUBSCRIPT CONTINUE: Already in subscript mode");
                } else if should_be_superscript && in_superscript {
                    eprintln!("⬆️ SUPERSCRIPT CONTINUE: Already in superscript mode");
                }
            } else if baseline_diff.abs() < RETURN_THRESHOLD && (in_subscript || in_superscript) {
                // Close script tags when returning close to baseline
                // But only if we're not just processing whitespace/punctuation
                // AND the current character is not itself a subscript/superscript
                let text_trimmed = span.text.trim();
                let is_current_subscript =
                    is_real_subscript(span, baseline_diff, base_font_size, spans, i);
                let is_current_superscript =
                    is_real_superscript(span, baseline_diff, base_font_size);

                if !text_trimmed.is_empty()
                    && !text_trimmed
                        .chars()
                        .all(|c| c.is_whitespace() || "=+−-()[]{}".contains(c))  // REMOVED: comma and semicolon - they should not inherit script mode
                    && !is_current_subscript  // Don't close if current char is subscript
                    && !is_current_superscript
                // Don't close if current char is superscript
                {
                    if in_subscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sub>") {
                            let closing_tag = tag_stack.remove(pos);
                            close_script_tag(&mut result, closing_tag);
                            eprintln!(
                                "🔄 BASELINE RETURN: Applied {closing_tag} for '{text_trimmed}'"
                            );
                            in_subscript = false;
                        }
                    }
                    if in_superscript {
                        if let Some(pos) = tag_stack.iter().rposition(|&tag| tag == "</sup>") {
                            let closing_tag = tag_stack.remove(pos);
                            close_script_tag(&mut result, closing_tag);
                            eprintln!(
                                "🔄 BASELINE RETURN: Applied {closing_tag} for '{text_trimmed}'"
                            );
                            in_superscript = false;
                        }
                    }
                } else {
                    eprintln!("⏭️ KEEPING script mode for whitespace/punct: '{text_trimmed}'");
                }
            }
        }

        // Add the actual text, cleaning up any substitute characters
        let cleaned_text = span.text.replace('\u{001a}', ""); // Remove SUB (substitute) character
        result.push_str(&cleaned_text);
    }

    // Close any remaining open tags, trimming trailing spaces before closing subscript/superscript tags
    while let Some(closing_tag) = tag_stack.pop() {
        close_script_tag(&mut result, closing_tag);
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
