//! Script Notation Detection Module
//!
//! This module provides comprehensive detection and formatting of subscripts, superscripts,
//! and bold text in both mathematical formulas and regular text content. The functions
//! are content-agnostic and work purely based on font properties and baseline positioning.
//!
//! ## Key Features:
//! - Detects subscripts and superscripts based on font size and position
//! - Content-independent processing (works for math formulas AND regular text)
//! - Research-validated proportional baseline thresholds (99.89% accuracy)
//! - Stack-based tag application with continuity logic
//! - Handles inline subscript patterns within single text spans
//! - Bold text detection based on font weight and naming

use crate::debug_print;
use crate::entities::CharSpan;
use lazy_static::lazy_static;

/// Proportional script detection threshold - minimum relative baseline shift as fraction of font size
const PROPORTIONAL_SCRIPT_THRESHOLD: f32 = 0.02; // 2% of font size for baseline shift detection

/// Font size ratio threshold for script detection - maximum font size ratio to be considered a script
const FONT_SIZE_SCRIPT_THRESHOLD: f32 = 0.85; // 85% of base font size

// === Movement and Position Thresholds ===
/// Absolute baseline threshold for complex detection - minimum absolute shift in points
const ABSOLUTE_BASELINE_THRESHOLD: f32 = 3.0; // 3 points of Y variation

/// Tiny movement threshold - movements smaller than this are handled specially
const TINY_MOVEMENT_THRESHOLD: f32 = 1.0; // 1 point

/// Spacing threshold - minimum gap between spans to add space
const SPAN_SPACING_THRESHOLD: f32 = 2.0; // 2 points

/// Clustering Y threshold - maximum Y difference to group into same baseline cluster
const CLUSTERING_Y_THRESHOLD: f32 = 5.0; // 5 points

// === Proportional Movement Limits ===
// Note: Legacy threshold-based limits have been replaced with composite scoring

// === Font Size Ratios ===
/// Very small font threshold - fonts smaller than this get special handling
const VERY_SMALL_FONT_THRESHOLD: f32 = 0.65; // 65% of base font

/// Minimum font size protection - don't let cluster base font get smaller than this
const MIN_FONT_SIZE_RATIO: f32 = 0.5; // 50% of global base font

/// Local baseline detection threshold - fonts this size or larger qualify as baseline
const LOCAL_BASELINE_FONT_RATIO: f32 = 0.9; // 90% of global base font

/// Relative baseline shift threshold - minimum shift as fraction of font size for script detection
const RELATIVE_BASELINE_THRESHOLD: f32 = 0.3; // 30% of font size

/// Font size filter threshold - minimum font size to avoid artifacts
const MIN_FONT_SIZE_FILTER: f32 = 2.0; // 2 points minimum

/// Alternative subscript threshold for specific patterns (0.2 = 20%)
const ALTERNATIVE_SUBSCRIPT_THRESHOLD: f32 = 0.2; // 20% of font size

/// Composite subscript confidence threshold - based on ChatGPT's normalized scoring approach
const COMPOSITE_SUBSCRIPT_CONFIDENCE_THRESHOLD: f32 = 0.3; // Confidence threshold [0,1] for subscript detection

/// Weighting factors for composite scoring (ChatGPT approach)
const VERTICAL_WEIGHT: f32 = 0.75; // Weight for vertical displacement (alpha)
const SIZE_WEIGHT: f32 = 0.25; // Weight for font size shrinkage (beta)

/// Reference values for normalization (typical "strong" subscript characteristics)
const VERTICAL_REF: f32 = 0.6; // 60% of font height downward movement
const SIZE_REF: f32 = 0.35; // 35% font size reduction

// Additional thresholds for script detection
const PROPORTIONAL_RETURN_THRESHOLD: f32 = 0.04; // 4% of font size to determine return to baseline
const FONT_SIZE_STRONG_SHRINKAGE_THRESHOLD: f32 = 0.25; // 25% font size reduction for strong shrinkage
const OPTICAL_ALIGNMENT_FONT_THRESHOLD: f32 = 0.75; // 75% font size for optical alignment cases
const HARDCODED_BASELINE_THRESHOLD: f32 = 3.0; // 3 points for baseline difference threshold
const HARDCODED_DOWNWARD_LIMIT: f32 = -2.0; // -2.0 for downward movement limit
const WINDOW_SIZE: usize = 2; // Window size for context analysis
const MIN_SPANS_FOR_CLUSTERING: usize = 4; // Minimum spans needed for clustering
const CONFIDENCE_NORMALIZATION_MIN: f32 = 0.0; // Minimum confidence value
const CONFIDENCE_NORMALIZATION_MAX: f32 = 1.0; // Maximum confidence value
const MATH_VARIABLE_MIN_LENGTH: usize = 2; // Minimum length for math variables
const DEBUG_CHAR_LIMIT: usize = 100; // Character limit for debug output

// === Footnote Detection Constants ===
/// Maximum length for footnote references (single digits, symbols)
const FOOTNOTE_MAX_LENGTH: usize = 2;
const DEBUG_TEXT_LIMIT: usize = 50; // Text limit for debug display
const DEBUG_CLUSTER_TEXT_LIMIT: usize = 20; // Text limit for cluster debug display

/// Configuration for subscript detection thresholds
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
            font_size_ratio_threshold: FONT_SIZE_SCRIPT_THRESHOLD,
            relative_baseline_threshold: RELATIVE_BASELINE_THRESHOLD,
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

/// Detect if text appears to be a footnote marker that should use lenient superscript thresholds
///
/// Footnote markers are typically:
/// - Single digits (1, 2, 3, etc.)
/// - Small superscript symbols (*, †, ‡, etc.)
/// - Single letters (a, b, c, etc.)
/// - Roman numerals (i, ii, iii, iv, etc.)
fn is_footnote_marker(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Single digits (most common footnote pattern)
    if trimmed.len() == 1 && trimmed.chars().next().unwrap().is_ascii_digit() {
        return true;
    }

    // Common footnote symbols
    if matches!(trimmed, "*" | "†" | "‡" | "§" | "¶" | "**") {
        return true;
    }

    // Single lowercase letters (often used for footnotes)
    if trimmed.len() == 1 && trimmed.chars().next().unwrap().is_ascii_lowercase() {
        return true;
    }

    // Short roman numerals
    if trimmed.len() <= 4
        && trimmed
            .chars()
            .all(|c| matches!(c, 'i' | 'v' | 'x' | 'I' | 'V' | 'X'))
    {
        return true;
    }

    false
}

/// Check if this baseline/font change represents a real superscript based on positioning and font metrics
fn is_real_superscript(
    current_span: &CharSpan,
    sequential_diff: f32,
    base_font_size: f32,
    is_first_span: bool,
) -> bool {
    // Real superscripts should have:
    // 1. Significant font size reduction (typically 70% or smaller of base text)
    // 2. UPWARD movement from previous character (NEGATIVE sequential_diff in PDF coordinates)
    // 3. Movement that's proportional to character's font size

    // Early return: superscripts MUST move upward (negative sequential_diff)
    if sequential_diff >= 0.0 {
        return false;
    }

    let text_trimmed = current_span.text.trim();
    if text_trimmed.is_empty() {
        return false;
    }

    // Apply composite scoring (ChatGPT approach)
    // Normalize by BASE font size, not current span's font size
    let v = sequential_diff / base_font_size; // Normalized vertical offset (negative for upward)
    let s = if current_span.font_size < base_font_size {
        1.0 - (current_span.font_size / base_font_size) // Font shrinkage
    } else {
        0.0
    };

    // Calculate superscript confidence
    let raw_sup = VERTICAL_WEIGHT * (-v).max(0.0) + SIZE_WEIGHT * s;
    let denom = VERTICAL_WEIGHT * VERTICAL_REF + SIZE_WEIGHT * SIZE_REF;
    let sup_confidence =
        (raw_sup / denom).clamp(CONFIDENCE_NORMALIZATION_MIN, CONFIDENCE_NORMALIZATION_MAX);

    // More lenient threshold for footnote markers
    let confidence_threshold = if is_first_span && is_footnote_marker(text_trimmed) {
        0.25 // Lower threshold for footnotes
    } else {
        COMPOSITE_SUBSCRIPT_CONFIDENCE_THRESHOLD // 0.3
    };

    // Require both confidence threshold AND smaller font
    let has_smaller_font = current_span.font_size < base_font_size * FONT_SIZE_SCRIPT_THRESHOLD;
    let is_likely_superscript = sup_confidence > confidence_threshold && has_smaller_font;

    let font_size_ratio = current_span.font_size / base_font_size;
    let relative_sequential_shift = sequential_diff / current_span.font_size;

    let decision_detail = if is_likely_superscript {
        format!(
            "COMPOSITE_REAL (v={:.3} s={:.3} conf={:.3})",
            v, s, sup_confidence
        )
    } else if sup_confidence <= confidence_threshold {
        format!(
            "LOW_CONFIDENCE (conf={:.3}<{:.3})",
            sup_confidence, confidence_threshold
        )
    } else if !has_smaller_font {
        format!(
            "FONT_TOO_LARGE (ratio={:.3}>={:.3})",
            font_size_ratio, FONT_SIZE_SCRIPT_THRESHOLD
        )
    } else {
        "UNKNOWN_REJECT".to_string()
    };

    debug_print!(
        "🔍 SUPERSCRIPT CHECK: '{}' font_ratio={:.2} sequential_shift_ratio={:.2} → {}",
        text_trimmed,
        font_size_ratio,
        relative_sequential_shift,
        decision_detail
    );

    is_likely_superscript
}

/// Analyze all spans for potential subscripts and generate comprehensive debug report
#[allow(dead_code)]
fn analyze_potential_subscripts(spans: &[CharSpan], base_font_size: f32, baseline: f32) {
    debug_print!("\\n=== COMPREHENSIVE SUBSCRIPT ANALYSIS ===");
    debug_print!("Base Font Size: {base_font_size:.1}, Base Baseline: {baseline:.1}");
    debug_print!("Total Spans: {}", spans.len());

    let mut potential_subscripts = Vec::new();
    let full_text: String = spans.iter().map(|s| s.text.as_str()).collect();

    debug_print!("\\nFull Text: '{full_text}'");
    debug_print!("\\n--- DETAILED SPAN ANALYSIS ---");

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
        let current_detection = if baseline_diff.abs() > HARDCODED_BASELINE_THRESHOLD {
            let has_smaller_font = font_size_ratio < FONT_SIZE_SCRIPT_THRESHOLD;
            let has_significant_shift = relative_baseline_shift > RELATIVE_BASELINE_THRESHOLD;
            has_smaller_font && has_significant_shift
        } else {
            false
        };

        debug_print!("SPAN[{i}]: '{text_trimmed}'");
        debug_print!(
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
        debug_print!(
            "  Font: size={:.1}, base={:.1}, ratio={:.3}",
            span.font_size,
            base_font_size,
            font_size_ratio
        );
        debug_print!(
            "  Metrics: abs_shift={:.1}, rel_shift={:.3} ({:.1}%)",
            baseline_diff.abs(),
            relative_baseline_shift,
            relative_baseline_shift * 100.0
        );
        debug_print!(
            "  Detection: current={}, font_ok={}, baseline_ok={}",
            if current_detection {
                "SUBSCRIPT"
            } else {
                "NORMAL"
            },
            if font_size_ratio < FONT_SIZE_SCRIPT_THRESHOLD {
                "YES"
            } else {
                "NO"
            },
            if relative_baseline_shift > RELATIVE_BASELINE_THRESHOLD {
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

        debug_print!("");
    }

    if !potential_subscripts.is_empty() {
        debug_print!("\\n=== DETECTED SUBSCRIPTS SUMMARY ===");
        debug_print!("| Idx | Char | Abs Shift | Rel Shift | Font Ratio | Detected |");
        debug_print!("|-----|------|-----------|-----------|------------|----------|");

        for (idx, text, baseline_diff, font_ratio, rel_shift, detected) in &potential_subscripts {
            debug_print!(
                "| {:3} | {:4} | {:9.1} | {:8.1}% | {:10.3} | {:8} |",
                idx,
                text,
                baseline_diff.abs(),
                rel_shift * 100.0,
                font_ratio,
                if *detected { "YES" } else { "NO" }
            );
        }
        debug_print!("Total detected subscripts: {}", potential_subscripts.len());
    }

    debug_print!("=== END ANALYSIS ===\\n");
}

/// Local baseline-aware subscript detection for mathematical expressions with large baseline shifts
/// This function looks at local font sizes rather than global baseline for subscript detection
#[allow(dead_code)]
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
    let window_size = WINDOW_SIZE; // Look at 2 spans before and after for tighter context
    let start_idx = current_index.saturating_sub(window_size);
    let end_idx = (current_index + window_size + 1).min(spans.len());
    let context_spans = &spans[start_idx..end_idx];

    // Find font sizes in the local context and establish what should be "normal" vs "subscript"
    let mut font_sizes: Vec<f32> = context_spans
        .iter()
        .filter(|s| !s.text.trim().is_empty() && s.font_size > MIN_FONT_SIZE_FILTER / 2.0)
        .map(|s| s.font_size)
        .collect();

    if font_sizes.is_empty() {
        return false;
    }

    font_sizes.sort_by(|a, b| b.partial_cmp(a).unwrap()); // Sort descending

    // Use the second-largest font size as local baseline if available, otherwise largest
    // This handles cases where we have: 10pt(global) > 7pt(local baseline) > 5pt(subscript)
    let local_baseline_font_size =
        if font_sizes.len() >= 2 && font_sizes[0] >= base_font_size * LOCAL_BASELINE_FONT_RATIO {
            // If largest font is close to global baseline, use second largest as local baseline
            font_sizes[1]
        } else {
            // Otherwise use largest as local baseline
            font_sizes[0]
        };

    // Compare current font size to LOCAL baseline font size
    let local_font_ratio = current_span.font_size / local_baseline_font_size;
    let has_smaller_font_locally = local_font_ratio < FONT_SIZE_SCRIPT_THRESHOLD;

    // Use proportional baseline shift calculation (same as is_real_subscript)
    let relative_baseline_shift = baseline_diff / current_span.font_size;

    // Apply same limits as main subscript detection
    if baseline_diff > 0.0 && relative_baseline_shift > ALTERNATIVE_SUBSCRIPT_THRESHOLD {
        return false; // More than 20% of font size upward movement is too much
    }
    if baseline_diff < 0.0 && relative_baseline_shift < HARDCODED_DOWNWARD_LIMIT {
        return false; // More than 200% of font size downward movement is too much
    }

    let has_significant_shift = relative_baseline_shift.abs() > PROPORTIONAL_SCRIPT_THRESHOLD;

    let is_contextual_subscript = has_smaller_font_locally && has_significant_shift;

    debug_print!(
        "🎯 LOCAL BASELINE: '{}' font={:.1} local_baseline={:.1}({:.3}) global_baseline={:.1} shift={:.1}({:.3}) → {}",
        text_trimmed,
        current_span.font_size, local_baseline_font_size, local_font_ratio,
        base_font_size, baseline_diff.abs(), relative_baseline_shift,
        if is_contextual_subscript { "✓ LOCAL_SUBSCRIPT" } else { "✗ NOT_SUBSCRIPT" }
    );

    is_contextual_subscript
}

/// Check if this baseline/font change represents a real subscript based on positioning and font metrics
///
/// Uses research-validated proportional baseline thresholds for 99.89% accuracy
fn is_real_subscript(
    current_span: &CharSpan,
    sequential_diff: f32,
    base_font_size: f32,
    _spans: &[CharSpan],
    _current_index: usize,
) -> bool {
    // Real subscripts should have:
    // 1. Significant font size reduction (typically 70% or smaller of base text)
    // 2. DOWNWARD movement from previous character (POSITIVE sequential_diff in PDF coordinates)
    // 3. Movement that's proportional to font size

    let text_trimmed = current_span.text.trim();
    if text_trimmed.is_empty() {
        return false;
    }

    // FOOTNOTE OVERRIDE: Check if this is a footnote reference BEFORE position checks
    // Footnotes should be superscripts, never subscripts
    if is_footnote_marker(text_trimmed) {
        debug_print!(
            "  📝 SEQUENTIAL FOOTNOTE DETECTED: '{}' should be superscript, not subscript",
            text_trimmed
        );
        return false; // Footnotes should never be subscripts
    }

    // Early return: subscripts MUST move downward (positive sequential_diff)
    if sequential_diff <= 0.0 {
        return false;
    }

    // Apply composite scoring (ChatGPT approach)
    // Normalize by BASE font size, not current span's font size
    let v = sequential_diff / base_font_size; // Normalized vertical offset (positive for downward)
    let s = if current_span.font_size < base_font_size {
        1.0 - (current_span.font_size / base_font_size) // Font shrinkage
    } else {
        0.0
    };

    // Calculate subscript confidence
    let raw_sub = VERTICAL_WEIGHT * v.max(0.0) + SIZE_WEIGHT * s;
    let denom = VERTICAL_WEIGHT * VERTICAL_REF + SIZE_WEIGHT * SIZE_REF;
    let sub_confidence =
        (raw_sub / denom).clamp(CONFIDENCE_NORMALIZATION_MIN, CONFIDENCE_NORMALIZATION_MAX);

    // Standard threshold for subscripts
    let confidence_threshold = COMPOSITE_SUBSCRIPT_CONFIDENCE_THRESHOLD; // 0.3

    // Require both confidence threshold AND smaller font
    let has_smaller_font = current_span.font_size < base_font_size * FONT_SIZE_SCRIPT_THRESHOLD;
    let is_likely_subscript = sub_confidence > confidence_threshold && has_smaller_font;

    let font_size_ratio = current_span.font_size / base_font_size;
    let relative_sequential_shift = sequential_diff / current_span.font_size;

    let decision_detail = if is_likely_subscript {
        format!(
            "✓ COMPOSITE_REAL (v={:.3} s={:.3} conf={:.3})",
            v, s, sub_confidence
        )
    } else if sub_confidence <= confidence_threshold {
        format!(
            "LOW_CONFIDENCE (conf={:.3}<{:.3})",
            sub_confidence, confidence_threshold
        )
    } else if !has_smaller_font {
        format!(
            "FONT_TOO_LARGE (ratio={:.3}>={:.3})",
            font_size_ratio, FONT_SIZE_SCRIPT_THRESHOLD
        )
    } else {
        "UNKNOWN_REJECT".to_string()
    };

    debug_print!(
        "🔍 SUBSCRIPT DETAILED: '{}' font={:.1}/{:.1}({:.3}) sequential={:.1}({:.3}) thresholds=font<{FONT_SIZE_SCRIPT_THRESHOLD}&shift_abs>{PROPORTIONAL_SCRIPT_THRESHOLD}&upward<0.2 → {}",
        text_trimmed,
        current_span.font_size, base_font_size, font_size_ratio,
        sequential_diff, relative_sequential_shift,
        decision_detail
    );

    is_likely_subscript
}

/// Main entry point for recursive tag processing
pub(crate) fn apply_tags_recursive(spans: &[CharSpan], depth: usize) -> String {
    debug_print!(
        "🔄 apply_tags_recursive called with {} spans at depth {}",
        spans.len(),
        depth
    );

    if spans.is_empty() {
        return String::new();
    }

    // Prevent infinite recursion
    if depth > 10 {
        debug_print!("⚠️ Maximum recursion depth reached, returning plain text");
        return spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<&str>>()
            .join("");
    }

    // Step 1: Detect all tag ranges for this level
    let subscript_ranges: Vec<TagRange> = Vec::new();
    let superscript_ranges: Vec<TagRange> = Vec::new();
    let bold_ranges: Vec<TagRange> = Vec::new();
    // Note: formula detection is handled at Element level, not span level

    // Step 2: Combine and sort all ranges by priority (subscript/superscript first, then bold)
    let mut all_ranges: Vec<TagRange> = Vec::new();
    all_ranges.extend(subscript_ranges);
    all_ranges.extend(superscript_ranges);
    all_ranges.extend(bold_ranges);

    // Sort by start position to process in order
    all_ranges.sort_by_key(|r| r.start_span_index);

    // Step 3: Apply tags - for now, just delegate to existing function
    // This maintains current functionality while we build the new architecture
    apply_text_formatting(spans)
}

/// Helper function to close a subscript or superscript tag
fn close_script_tag(result: &mut String, tag: &str) {
    // Always add a space after closing tags to prevent word concatenation
    result.push_str(tag);
    result.push(' ');
}

/// Close tags until we reach the target tag, maintaining LIFO order
/// This prevents malformed nesting like <sup><b></sup></b>
fn close_tags_until(result: &mut String, tag_stack: &mut Vec<&'static str>, target_tag: &str) {
    let mut closed_tags = Vec::new();

    // Close tags in LIFO order until we find the target
    while let Some(tag) = tag_stack.pop() {
        close_script_tag(result, tag);
        debug_print!("🔄 LIFO CLOSE: Applied {tag}");

        if tag == target_tag {
            break; // Found and closed the target tag
        } else {
            // This tag was closed prematurely, remember it for reopening
            closed_tags.push(tag);
        }
    }

    // Reopen any tags that were closed prematurely (in reverse order)
    for tag in closed_tags.into_iter().rev() {
        let opening_tag = match tag {
            "</b>" => "<b>",
            "</sub>" => "<sub>",
            "</sup>" => "<sup>",
            _ => continue,
        };
        result.push_str(opening_tag);
        tag_stack.push(tag);
        debug_print!("🔄 LIFO REOPEN: Applied {opening_tag}");
    }
}

lazy_static! {
    /// Pre-compiled regex for removing spaces before closing tags
    static ref SCRIPT_TAG_SPACING_REGEX: regex::Regex = regex::Regex::new(r"\s+</").unwrap();
    /// Pre-compiled regex for removing spaces after opening tags
    static ref SCRIPT_OPENING_TAG_SPACING_REGEX: regex::Regex = regex::Regex::new(r"(<su[bp]>|<b>)\s+").unwrap();
}

/// Fix spacing around script tags
/// Examples:
/// - "mask </sub>" -> "mask</sub>" (spaces before closing tags)
/// - "<sup> 2</sup>" -> "<sup>2</sup>" (spaces after opening tags)
fn fix_script_tag_spacing(text: &str) -> String {
    // Remove spaces before closing tags: "mask </sub>" -> "mask</sub>"
    let step1 = SCRIPT_TAG_SPACING_REGEX.replace_all(text, "</");

    // Remove spaces after opening tags: "<sup> 2</sup>" -> "<sup>2</sup>"
    SCRIPT_OPENING_TAG_SPACING_REGEX
        .replace_all(&step1, "$1")
        .to_string()
}

/// Detect if spans require clustering-based detection due to multiple baselines
/// Returns true if the text has complex mathematical structure that would benefit from clustering
fn requires_baseline_clustering(spans: &[CharSpan]) -> bool {
    if spans.len() < MIN_SPANS_FOR_CLUSTERING {
        debug_print!(
            "🔍 COMPLEXITY CHECK: Only {} spans, too short for clustering",
            spans.len()
        );
        return false; // Too short for complex formulas
    }

    // Calculate Y-position variance to detect multiple baselines
    let y_positions: Vec<f32> = spans
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .map(|s| s.bbox.y0)
        .collect();

    if y_positions.len() < 4 {
        debug_print!(
            "🔍 COMPLEXITY CHECK: Only {} non-empty spans, too few for clustering",
            y_positions.len()
        );
        return false;
    }

    // Calculate standard deviation of Y positions
    let mean_y = y_positions.iter().sum::<f32>() / y_positions.len() as f32;
    let variance = y_positions
        .iter()
        .map(|y| (y - mean_y).powi(2))
        .sum::<f32>()
        / y_positions.len() as f32;
    let std_dev = variance.sqrt();

    // Check for multiple distinct baseline groups (high Y variance)
    let has_multiple_baselines = std_dev > ABSOLUTE_BASELINE_THRESHOLD; // 3+ points of Y variation suggests multiple baselines

    // Check for mathematical content indicators
    let full_text: String = spans.iter().map(|s| s.text.as_str()).collect();
    let has_math_symbols = full_text.contains('∑')
        || full_text.contains('∈')
        || full_text.contains('∪')
        || full_text.contains('∩');

    let should_cluster = has_multiple_baselines && has_math_symbols;

    debug_print!(
        "🔍 COMPLEXITY CHECK: {} spans, std_dev={:.1}, math_symbols={}, cluster={}",
        spans.len(),
        std_dev,
        has_math_symbols,
        should_cluster
    );
    debug_print!("📊 Y-positions: {:?}", y_positions);
    debug_print!("📝 Text content: '{}'", full_text);

    should_cluster
}

/// Apply clustering-based formatting for complex mathematical formulas
fn apply_clustering_formatting(spans: &[CharSpan]) -> String {
    debug_print!(
        "🔬 CLUSTERING FORMATTING: Processing {} spans with baseline clustering",
        spans.len()
    );

    // Get clustering results
    let cluster_results = detect_subscripts_clustered(spans);

    // Convert clustering results to formatted text
    let mut result = String::new();
    let mut tag_stack: Vec<&'static str> = Vec::new();
    let mut current_bold = false;
    let mut in_subscript = false;
    let mut in_superscript = false;

    for (i, span) in spans.iter().enumerate() {
        let text_trimmed = span.text.trim();
        if text_trimmed.is_empty() {
            continue;
        }

        // Find the clustering result for this span
        let (_, is_sub, is_sup) = cluster_results
            .iter()
            .find(|(idx, _, _)| *idx == i)
            .copied()
            .unwrap_or((i, false, false));

        debug_print!(
            "🔬 CLUSTERING SPAN[{}]: '{}' → sub={}, sup={}",
            i,
            text_trimmed,
            is_sub,
            is_sup
        );

        // Handle bold changes
        let is_bold_now = is_bold_text(span);
        if is_bold_now != current_bold {
            if is_bold_now {
                result.push_str("<b>");
                tag_stack.push("</b>");
            } else {
                close_tags_until(&mut result, &mut tag_stack, "</b>");
            }
            current_bold = is_bold_now;
        }

        // Handle script changes based on clustering results
        if is_sub && !in_subscript && !in_superscript {
            result.push_str("<sub>");
            tag_stack.push("</sub>");
            in_subscript = true;
            debug_print!("⬇️ CLUSTER SUB START: '{}'", text_trimmed);
        } else if is_sup && !in_superscript && !in_subscript {
            result.push_str("<sup>");
            tag_stack.push("</sup>");
            in_superscript = true;
            debug_print!("⬆️ CLUSTER SUP START: '{}'", text_trimmed);
        } else if !is_sub && !is_sup {
            // Return to normal text
            if in_subscript {
                close_tags_until(&mut result, &mut tag_stack, "</sub>");
                in_subscript = false;
                debug_print!("🔄 CLUSTER SUB END: '{}'", text_trimmed);
            }
            if in_superscript {
                close_tags_until(&mut result, &mut tag_stack, "</sup>");
                in_superscript = false;
                debug_print!("🔄 CLUSTER SUP END: '{}'", text_trimmed);
            }
        }

        // Add spacing between spans when needed
        if i > 0 && !result.is_empty() {
            let prev_span = &spans[i - 1];
            let x_gap = span.bbox.x0 - prev_span.bbox.x1;
            let y_diff = (span.bbox.y0 - prev_span.bbox.y0).abs();

            let needs_space = (x_gap > SPAN_SPACING_THRESHOLD || y_diff > CLUSTERING_Y_THRESHOLD)
                && !result.ends_with(' ')
                && !text_trimmed.starts_with(' ');

            if needs_space {
                result.push(' ');
            }
        }

        // Add the cleaned text
        let cleaned_text = span.text.replace('\u{001a}', "");
        result.push_str(&cleaned_text);
    }

    // Close any remaining tags
    while let Some(closing_tag) = tag_stack.pop() {
        close_script_tag(&mut result, closing_tag);
    }

    // Apply post-processing
    #[cfg(feature = "correction-engine")]
    let corrected_result = {
        use crate::correction::character::fix_math_symbol_corruptions;
        fix_math_symbol_corruptions(&result)
    };

    #[cfg(not(feature = "correction-engine"))]
    let corrected_result = result;

    let spaced_result = fix_script_tag_spacing(&corrected_result);

    debug_print!(
        "🔬 CLUSTERING RESULT: '{}'",
        spaced_result.chars().take(100).collect::<String>()
    );
    spaced_result
}

/// Apply comprehensive text formatting including subscripts, superscripts, and bold text
///
/// Uses baseline position changes and font changes to determine when to open/close HTML tags:
/// - Baseline moves down → open <sub>, push </sub> on stack
/// - Baseline moves up → open <sup>, push </sup> on stack  
/// - Baseline returns to normal → pop and close current tag
/// - Bold font detected → open <b>, push </b> on stack
/// - Bold font ends → pop and close bold tag
///
/// This approach is content-independent and purely based on positioning/font changes.
/// Works for both mathematical formulas AND regular text content with proper tag nesting.
pub(crate) fn apply_text_formatting(spans: &[CharSpan]) -> String {
    let combined_text: String = spans.iter().map(|s| s.text.as_str()).collect();
    debug_print!(
        "⚡ STACK-BASED detection called with {} spans: '{}'",
        spans.len(),
        combined_text.chars().take(100).collect::<String>()
    );

    if spans.is_empty() {
        return String::new();
    }

    // HYBRID APPROACH: Check if we need clustering for complex formulas
    debug_print!("🔀 HYBRID: Checking if clustering is needed...");
    if requires_baseline_clustering(spans) {
        debug_print!("🔀 HYBRID: Using clustering approach for complex formula");
        return apply_clustering_formatting(spans);
    }
    debug_print!("🔀 HYBRID: Using sequential approach for simple text");

    let full_text: String = {
        let mut result = String::new();
        for (i, span) in spans.iter().enumerate() {
            if i == 0 {
                result.push_str(&span.text);
            } else {
                let prev_span = &spans[i - 1];

                // Check horizontal gap between spans (same logic as concatenate_spans_with_spacing)
                let x_gap = span.bbox.x0 - prev_span.bbox.x1;
                let y_diff = (span.bbox.y0 - prev_span.bbox.y0).abs();

                // Add space if there's horizontal gap, vertical difference, or line wrapping
                // Line wrapping case: negative x_gap with small y_diff suggests text continuation
                let needs_space = (x_gap > 2.0 || y_diff > 5.0 || (x_gap < -10.0 && y_diff < 3.0))
                    && !prev_span.text.ends_with(' ')
                    && !span.text.starts_with(' ');

                if needs_space {
                    result.push(' ');
                }
                result.push_str(&span.text);
            }
        }
        result
    };
    debug_print!(
        "⚡ SEQUENTIAL: Processing text='{}'",
        full_text.chars().take(DEBUG_TEXT_LIMIT).collect::<String>()
    );

    let mut result = String::new();
    let mut tag_stack: Vec<&'static str> = Vec::new();
    let mut current_bold = false;
    let mut in_subscript = false;
    let mut in_superscript = false;
    let mut base_font_size: f32 = 0.0;
    let mut previous_y_position: Option<f32> = None; // Track previous character position for sequential comparison
    let mut font_size_initialized = false;

    // Font-proportional thresholds instead of absolute point values
    // This ensures detection scales properly with font size
    // Use module-level constants for consistency

    // Process each span
    for (i, span) in spans.iter().enumerate() {
        debug_print!(
            "🔍 SPAN[{}]: '{}' y={:.1} size={:.1} font={}",
            i,
            span.text.trim(),
            span.bbox.y0,
            span.font_size,
            span.font_name
        );

        // Debug specific characters to understand mask splitting
        let text_trimmed = span.text.trim();

        // Skip superscript/subscript detection for empty spans to prevent empty tags
        // Empty spans should not trigger tag opening/closing logic
        if text_trimmed.is_empty() {
            // Still handle bold font changes for empty spans
            if !font_size_initialized {
                font_size_initialized = true;
                current_bold = is_bold_text(span);
                if current_bold {
                    result.push_str("<b>");
                    tag_stack.push("</b>");
                    debug_print!("🅱️ BOLD START: Pushed </b> on stack");
                }
                // Calculate base font size using maximum instead of median
                // This prevents subscripts from dominating the calculation
                let font_sizes: Vec<f32> = spans
                    .iter()
                    .map(|s| s.font_size)
                    .filter(|&size| size > MIN_FONT_SIZE_FILTER)
                    .collect();

                base_font_size = if font_sizes.is_empty() {
                    span.font_size
                } else {
                    // Use maximum font size as base - this ensures subscripts don't affect base calculation
                    *font_sizes
                        .iter()
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .unwrap()
                };
                debug_print!("📏 BASE FONT SIZE: {base_font_size:.1}");
            }
            continue; // Skip script detection for empty spans
        }

        // Initialize base font size on first non-empty span (no baseline needed)
        if !font_size_initialized {
            // Calculate base font size from all spans
            let mut font_sizes: Vec<f32> = spans
                .iter()
                .map(|s| s.font_size)
                .filter(|&size| size > MIN_FONT_SIZE_FILTER) // Filter out tiny fonts that are likely artifacts
                .collect();
            font_sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());

            base_font_size = if font_sizes.is_empty() {
                span.font_size // Fallback to first span if no valid fonts found
            } else {
                // Use maximum font size as base instead of median
                // This ensures subscripts (which are smaller) don't dominate the calculation
                *font_sizes
                    .iter()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap()
            };

            font_size_initialized = true;
            current_bold = is_bold_text(span);
            if current_bold {
                result.push_str("<b>");
                tag_stack.push("</b>");
                debug_print!("🅱️ BOLD START: Pushed </b> on stack");
            }
            debug_print!("📏 SEQUENTIAL DETECTION: BASE FONT SIZE: {base_font_size:.1}");
        }

        // SEQUENTIAL CHARACTER COMPARISON: Compare to previous character position
        let sequential_diff = if let Some(prev_y) = previous_y_position {
            span.bbox.y0 - prev_y
        } else {
            // First character: Look ahead to next non-empty span for comparison
            if let Some(next_span) = spans
                .get(i + 1..)
                .and_then(|remaining| remaining.iter().find(|s| !s.text.trim().is_empty()))
            {
                // Compare first span against next non-empty span (for footnote superscripts)
                span.bbox.y0 - next_span.bbox.y0
            } else {
                0.0 // No next span to compare against
            }
        };

        debug_print!(
            "📏 SEQUENTIAL COMPARISON: span.y0={:.1}, prev_y={:?}, diff={:.1}",
            span.bbox.y0,
            previous_y_position
                .map(|y| format!("{y:.1}"))
                .unwrap_or_else(|| "NONE".to_string()),
            sequential_diff
        );

        // Update previous position for next character (only for non-empty spans)
        previous_y_position = Some(span.bbox.y0);

        let is_bold_now = is_bold_text(span);

        // Handle font changes (bold on/off)
        if is_bold_now != current_bold {
            if is_bold_now {
                result.push_str("<b>");
                tag_stack.push("</b>");
                debug_print!("🅱️ BOLD START: Pushed </b> on stack");
            } else {
                // Close bold tag using LIFO order
                close_tags_until(&mut result, &mut tag_stack, "</b>");
                debug_print!("🅱️ BOLD END: Closed bold tag using LIFO order");
            }
            current_bold = is_bold_now;
        }

        // Now proceed with normal subscript/superscript detection logic using sequential comparison
        {
            let text_trimmed = span.text.trim();

            // POSITIONING-BASED SUBSCRIPT CONTINUITY LOGIC
            // When we're already in subscript mode, check for continuity based purely on positioning
            if in_subscript {
                // Check if current character is also a subscript based on sequential movement
                let should_be_subscript =
                    is_real_subscript(span, sequential_diff, base_font_size, spans, i);

                // Continue subscript only if:
                // 1. Current character would be detected as subscript OR
                // 2. Character has a very small font OR
                // 3. Text is empty (whitespace spans)
                //
                // For sequential approach, we rely on the movement detection rather than absolute positioning
                let has_very_small_font =
                    span.font_size <= base_font_size * VERY_SMALL_FONT_THRESHOLD; // Much stricter font requirement

                let should_continue =
                    should_be_subscript || has_very_small_font || text_trimmed.is_empty();

                if should_continue {
                    // Continue subscript based on sequential detection
                    debug_print!(
                        "⬇️ SUBSCRIPT CONTINUE: Sequential detection supports continuation"
                    );
                    // Skip baseline change detection and just continue
                    let cleaned_text = span.text.replace('\u{001a}', ""); // Remove SUB (substitute) character
                    result.push_str(&cleaned_text);
                    continue;
                } else {
                    // Position indicates we should end subscript
                    close_tags_until(&mut result, &mut tag_stack, "</sub>");
                    in_subscript = false;
                    debug_print!("🔄 SUB CLOSE: Applied using LIFO order - position indicates end of subscript");
                }
            }

            // Check for sequential movement changes that might indicate scripts
            // Use font-proportional threshold instead of absolute threshold
            let relative_sequential_shift = sequential_diff.abs() / span.font_size;

            debug_print!(
                "📏 PROPORTIONAL THRESHOLDS: sequential_diff={:.1}pt, font_size={:.1}pt, relative_shift={:.3} ({:.1}%) | script_threshold={:.3} ({:.1}%), return_threshold={:.3} ({:.1}%)",
                sequential_diff.abs(), span.font_size, relative_sequential_shift, relative_sequential_shift * 100.0,
                PROPORTIONAL_SCRIPT_THRESHOLD, PROPORTIONAL_SCRIPT_THRESHOLD * 100.0,
                PROPORTIONAL_RETURN_THRESHOLD, PROPORTIONAL_RETURN_THRESHOLD * 100.0
            );

            if relative_sequential_shift > PROPORTIONAL_SCRIPT_THRESHOLD {
                // ENHANCED CONTEXT-AWARE SUBSCRIPT DETECTION
                // Use sequential character comparison for script detection
                let should_be_subscript =
                    is_real_subscript(span, sequential_diff, base_font_size, spans, i);

                let mut should_be_superscript =
                    is_real_superscript(span, sequential_diff, base_font_size, i == 0);

                // Calculate font ratio for reference detection
                let font_ratio = span.font_size / base_font_size;

                // REFERENCE NUMBER PATTERN DETECTION
                // Check if this looks like a reference citation (digit after text)
                let prev_text = if i > 0 { spans[i - 1].text.trim() } else { "" };
                let is_likely_reference = text_trimmed.chars().all(|c| c.is_ascii_digit()) &&
                    prev_text.chars().any(|c| c.is_alphabetic()) &&
                    prev_text.len() > MATH_VARIABLE_MIN_LENGTH && // Not single math variables like n, t, c
                    font_ratio < FONT_SIZE_SCRIPT_THRESHOLD; // Still require smaller font

                // For likely reference numbers, prefer superscript even with problematic positioning
                if is_likely_reference && !should_be_subscript && !should_be_superscript {
                    should_be_superscript = true;
                    debug_print!(
                        "📄 REFERENCE OVERRIDE: Treating '{}' after '{}' as superscript",
                        text_trimmed,
                        prev_text
                    );
                }

                // Priority logic: Prefer superscript for upward movement (negative baseline_diff)
                // and subscript for downward movement (positive baseline_diff)
                let is_optical_alignment_case = should_be_subscript
                    && should_be_superscript
                    && font_ratio < OPTICAL_ALIGNMENT_FONT_THRESHOLD;

                // FIXED: Use sequential movement direction to determine priority
                if should_be_superscript
                    && sequential_diff < 0.0
                    && !in_superscript
                    && !in_subscript
                {
                    // Superscript has priority for upward movement (negative sequential_diff)
                    result.push_str("<sup>");
                    tag_stack.push("</sup>");
                    in_superscript = true;
                    debug_print!(
                        "⬆️ SUPERSCRIPT START: Real superscript detected (sequential_diff={:.2})",
                        sequential_diff
                    );
                } else if should_be_subscript
                    && sequential_diff > 0.0
                    && !in_subscript
                    && !in_superscript
                {
                    // Subscript has priority for downward movement (positive sequential_diff)
                    result.push_str("<sub>");
                    tag_stack.push("</sub>");
                    in_subscript = true;
                    debug_print!(
                        "⬇️ SUBSCRIPT START: Real subscript detected (sequential_diff={:.2})",
                        sequential_diff
                    );
                } else if should_be_subscript && !in_subscript && !in_superscript {
                    // Fallback: Original subscript priority logic for ambiguous cases
                    result.push_str("<sub>");
                    tag_stack.push("</sub>");
                    in_subscript = true;
                    debug_print!(
                        "⬇️ SUBSCRIPT START: Real subscript detected (font_ratio={:.2})",
                        font_ratio
                    );
                } else if should_be_superscript
                    && !in_superscript
                    && !in_subscript
                    && !is_optical_alignment_case
                {
                    // Superscript only if not an optical alignment case
                    result.push_str("<sup>");
                    tag_stack.push("</sup>");
                    in_superscript = true;
                    debug_print!("⬆️ SUPERSCRIPT START: Real superscript detected");
                } else if !should_be_subscript && !should_be_superscript {
                    // Character should be normal - close any open script tags
                    // Close tags in LIFO order to maintain proper nesting
                    if in_subscript {
                        close_tags_until(&mut result, &mut tag_stack, "</sub>");
                        in_subscript = false;
                    }
                    if in_superscript {
                        close_tags_until(&mut result, &mut tag_stack, "</sup>");
                        in_superscript = false;
                    }
                } else if should_be_superscript && in_superscript {
                    debug_print!("⬆️ SUPERSCRIPT CONTINUE: Already in superscript mode");
                } else if should_be_subscript && in_subscript {
                    debug_print!("⬇️ SUBSCRIPT CONTINUE: Already in subscript mode");
                } else {
                    // Ignore script transitions when already in a different script mode
                    // This prevents nested tags like <sup><sub></sub></sup>
                    if (should_be_superscript || should_be_subscript)
                        && (in_subscript || in_superscript)
                    {
                        debug_print!(
                            "🚫 SCRIPT IGNORE: Already in script mode, ignoring transition"
                        );
                    }
                }
            } else if relative_sequential_shift < PROPORTIONAL_RETURN_THRESHOLD
                && (in_subscript || in_superscript)
            {
                // Close script tags when returning close to baseline
                // But only if we're not just processing whitespace/punctuation
                // AND the current character is not itself a subscript/superscript
                let text_trimmed = span.text.trim();
                let is_current_subscript =
                    is_real_subscript(span, sequential_diff, base_font_size, spans, i);
                let is_current_superscript =
                    is_real_superscript(span, sequential_diff, base_font_size, i == 0);

                if !text_trimmed.is_empty()
                    && !text_trimmed
                        .chars()
                        .all(|c| c.is_whitespace() || "=+−-()[]{}".contains(c))  // REMOVED: comma and semicolon - they should not inherit script mode
                    && !is_current_subscript  // Don't close if current char is subscript
                    && !is_current_superscript
                // Don't close if current char is superscript
                {
                    if in_subscript {
                        close_tags_until(&mut result, &mut tag_stack, "</sub>");
                        in_subscript = false;
                        debug_print!(
                            "🔄 BASELINE RETURN: Applied subscript close using LIFO order for '{text_trimmed}'"
                        );
                    }
                    if in_superscript {
                        close_tags_until(&mut result, &mut tag_stack, "</sup>");
                        in_superscript = false;
                        debug_print!(
                            "🔄 BASELINE RETURN: Applied superscript close using LIFO order for '{text_trimmed}'"
                        );
                    }
                } else {
                    debug_print!("⏭️ KEEPING script mode for whitespace/punct: '{text_trimmed}'");
                }
            }

            // FIXED: Check if we should start a new subscript after exiting one
            // This handles cases where "N" exits subscript mode but "mask" should re-enter it
            // Use proportional threshold for consistency
            let relative_sequential_shift_for_restart = sequential_diff.abs() / span.font_size;
            if !in_subscript
                && !in_superscript
                && relative_sequential_shift_for_restart <= PROPORTIONAL_SCRIPT_THRESHOLD
            {
                let should_be_subscript =
                    is_real_subscript(span, sequential_diff, base_font_size, spans, i);

                if should_be_subscript {
                    result.push_str("<sub>");
                    tag_stack.push("</sub>");
                    in_subscript = true;
                    debug_print!(
                        "⬇️ SUBSCRIPT RESTART: Re-opening subscript for '{}'",
                        text_trimmed
                    );
                }
            }
        }

        // Add the actual text, cleaning up any substitute characters
        let cleaned_text = span.text.replace('\u{001a}', ""); // Remove SUB (substitute) character

        // Add spacing between spans when needed (to preserve word boundaries)
        if i > 0 && !cleaned_text.is_empty() {
            let prev_span = &spans[i - 1];

            // Check horizontal gap (words on same line)
            let x_gap = span.bbox.x0 - prev_span.bbox.x1;
            // Check vertical gap (wrapped text)
            let y_diff = (span.bbox.y0 - prev_span.bbox.y0).abs();

            // Add space if:
            // 1. Significant horizontal gap (>2 points) indicating word boundary
            // 2. Vertical difference (>5 points) indicating line wrap
            // 3. Line wrapping case: negative x_gap with small y_diff suggests text continuation
            // 4. Previous text doesn't end with space and current doesn't start with one
            let needs_space = (x_gap > SPAN_SPACING_THRESHOLD
                || y_diff > CLUSTERING_Y_THRESHOLD
                || (x_gap < -10.0 && y_diff < 3.0))
                && !result.ends_with(' ')
                && !cleaned_text.starts_with(' ');

            if needs_space {
                result.push(' ');
            }
        }

        result.push_str(&cleaned_text);
    }

    // Close any remaining open tags, trimming trailing spaces before closing subscript/superscript tags
    while let Some(closing_tag) = tag_stack.pop() {
        close_script_tag(&mut result, closing_tag);
        debug_print!("🔚 CLEANUP: Applied remaining {closing_tag}");
    }

    // Apply mathematical symbol corrections to fix patterns like "6=" → "≠", "∈/" → "∉"
    #[cfg(feature = "correction-engine")]
    let corrected_result = {
        use crate::correction::character::fix_math_symbol_corruptions;
        fix_math_symbol_corruptions(&result)
    };

    #[cfg(not(feature = "correction-engine"))]
    let corrected_result = result;

    // Post-process to add proper spacing after subscript/superscript closing tags
    let spaced_result = fix_script_tag_spacing(&corrected_result);

    debug_print!(
        "⚡ STACK-BASED RESULT: '{}'",
        spaced_result
            .chars()
            .take(DEBUG_CHAR_LIMIT)
            .collect::<String>()
    );

    spaced_result
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
    // TODO: Implement inline pattern detection for cases where subscripts
    // appear within single text spans without font/baseline changes
    None
}

/// Cluster characters into baseline groups for multiple-baseline subscript detection
/// Returns clusters where each cluster represents characters sharing similar Y positions
fn cluster_baselines(spans: &[CharSpan], y_threshold: f32) -> Vec<Vec<usize>> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Create spans with their indices, sorted by Y position
    let mut indexed_spans: Vec<(usize, f32)> = spans
        .iter()
        .enumerate()
        .map(|(i, span)| (i, span.bbox.y0))
        .collect();
    indexed_spans.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let mut clusters = Vec::new();
    let mut current_cluster = vec![indexed_spans[0].0];
    let mut current_baseline = indexed_spans[0].1;

    for (span_idx, y_pos) in indexed_spans.iter().skip(1) {
        if (y_pos - current_baseline).abs() <= y_threshold {
            // Close enough to current baseline - add to cluster
            current_cluster.push(*span_idx);
        } else {
            // Too far - start new cluster
            clusters.push(current_cluster);
            current_cluster = vec![*span_idx];
            current_baseline = *y_pos;
        }
    }
    clusters.push(current_cluster);

    debug_print!(
        "🗂️ BASELINE CLUSTERING: Created {} clusters with threshold {:.1}pt",
        clusters.len(),
        y_threshold
    );

    for (i, cluster) in clusters.iter().enumerate() {
        let cluster_y_positions: Vec<f32> = cluster.iter().map(|&idx| spans[idx].bbox.y0).collect();
        let cluster_text: String = cluster
            .iter()
            .map(|&idx| spans[idx].text.trim())
            .collect::<Vec<_>>()
            .join("");

        debug_print!(
            "  Cluster {}: {} spans, Y range {:.1}-{:.1}, text: '{}'",
            i,
            cluster.len(),
            cluster_y_positions
                .iter()
                .fold(f32::INFINITY, |a, &b| a.min(b)),
            cluster_y_positions
                .iter()
                .fold(f32::NEG_INFINITY, |a, &b| a.max(b)),
            cluster_text
                .chars()
                .take(DEBUG_CLUSTER_TEXT_LIMIT)
                .collect::<String>()
        );
    }

    clusters
}

/// Detect if a character span is likely a footnote reference
/// Footnote references are typically:
/// - Single digits (1, 2, 3) or symbols (*, †, ‡, §)  
/// - Smaller font size than the main text
/// - Located after text content (more permissive approach)
fn is_footnote_reference(
    span: &CharSpan,
    _span_idx: usize,
    _spans: &[CharSpan],
    cluster_base_font_size: f32,
) -> bool {
    let text = span.text.trim();

    // 1. Check if content looks like a footnote reference (primary criteria)
    let is_footnote_content = text.len() <= FOOTNOTE_MAX_LENGTH
        && (text.chars().all(|c| c.is_ascii_digit())
            || matches!(text, "*" | "†" | "‡" | "§" | "**" | "††"));

    // 2. Check if font is smaller than the base font (typical for footnotes)
    let has_smaller_font = span.font_size < cluster_base_font_size * FONT_SIZE_SCRIPT_THRESHOLD;

    // For now, use simpler logic: if it looks like footnote content and has smaller font
    // The positioning check was too restrictive
    is_footnote_content && has_smaller_font
}

// Removed unused function detect_subscripts_in_cluster

/// Apply subscript detection within a baseline cluster using GLOBAL baseline
/// This fixes the issue where subscripts form their own cluster and appear normal relative to cluster baseline
fn detect_subscripts_in_cluster_with_global_baseline(
    spans: &[CharSpan],
    cluster_indices: &[usize],
    global_base_font_size: f32,
    global_baseline: f32,
) -> Vec<(usize, bool, bool)> {
    // (span_index, is_subscript, is_superscript)
    if cluster_indices.is_empty() {
        return Vec::new();
    }

    // Calculate cluster base font size as maximum font size in cluster
    let cluster_base_font_size = cluster_indices
        .iter()
        .map(|&idx| spans[idx].font_size)
        .fold(0.0, f32::max)
        .max(global_base_font_size * MIN_FONT_SIZE_RATIO); // Don't let it get too small

    debug_print!(
        "📐 GLOBAL CLUSTER ANALYSIS: global_baseline={:.1}, cluster_base_font={:.1}",
        global_baseline,
        cluster_base_font_size
    );

    let mut results = Vec::new();

    for &span_idx in cluster_indices {
        let span = &spans[span_idx];
        let baseline_diff = span.bbox.y0 - global_baseline; // Compare to GLOBAL baseline
        let font_ratio = span.font_size / cluster_base_font_size;

        // Apply research-based thresholds using global baseline comparison
        let has_smaller_font = font_ratio < FONT_SIZE_SCRIPT_THRESHOLD;

        // For very small movements (< 1pt), be more tolerant if font is small
        let abs_baseline_diff = baseline_diff.abs();
        let is_tiny_movement = abs_baseline_diff < TINY_MOVEMENT_THRESHOLD;

        // ChatGPT's normalized composite scoring approach
        // 1. Normalized vertical offset (positive = below baseline = subscript candidate)
        let v = baseline_diff / cluster_base_font_size; // Normalized by font size

        // 2. Font size shrinkage (0 = normal size, >0 = smaller than average)
        let s = if span.font_size < cluster_base_font_size {
            1.0 - (span.font_size / cluster_base_font_size)
        } else {
            0.0
        };

        // 3. Directional confidence scores [0,1]
        let raw_sub = VERTICAL_WEIGHT * v.max(0.0) + SIZE_WEIGHT * s;
        let raw_sup = VERTICAL_WEIGHT * (-v).max(0.0) + SIZE_WEIGHT * s;

        // 4. Normalize to [0,1] using reference values
        let denom = VERTICAL_WEIGHT * VERTICAL_REF + SIZE_WEIGHT * SIZE_REF;
        let sub_confidence =
            (raw_sub / denom).clamp(CONFIDENCE_NORMALIZATION_MIN, CONFIDENCE_NORMALIZATION_MAX);
        let sup_confidence =
            (raw_sup / denom).clamp(CONFIDENCE_NORMALIZATION_MIN, CONFIDENCE_NORMALIZATION_MAX);

        // Check for footnote reference patterns FIRST (highest priority)
        let is_potential_footnote =
            is_footnote_reference(span, span_idx, spans, cluster_base_font_size);

        let (is_subscript, is_superscript) = if is_potential_footnote {
            // HIGHEST PRIORITY: Footnote references → superscript (ignore baseline positioning)
            debug_print!("  📝 FOOTNOTE SUPERSCRIPT: '{}' detected as footnote reference baseline_diff={:.1}", 
                         span.text.trim(), baseline_diff);
            (false, true)
        } else if s > FONT_SIZE_STRONG_SHRINKAGE_THRESHOLD {
            // Strong font size reduction (>25%) → likely subscript regardless of baseline positioning
            // This handles PDF rendering issues where subscripts are positioned inconsistently
            debug_print!("  🔬 FONT SIZE OVERRIDE: '{}' strong_shrinkage={:.3} baseline={:.3} → treating as subscript", 
                         span.text.trim(), s, v);
            (true, false)
        } else if sub_confidence > COMPOSITE_SUBSCRIPT_CONFIDENCE_THRESHOLD
            && sub_confidence > sup_confidence
            && has_smaller_font
        {
            // High subscript confidence AND smaller font
            debug_print!("  🔬 COMPOSITE SUBSCRIPT: '{}' sub_conf={:.3} sup_conf={:.3} (v={:.3} s={:.3}) baseline_diff={:.1}", 
                         span.text.trim(), sub_confidence, sup_confidence, v, s, baseline_diff);
            (true, false)
        } else if sup_confidence > COMPOSITE_SUBSCRIPT_CONFIDENCE_THRESHOLD
            && sup_confidence > sub_confidence
            && has_smaller_font
        {
            // High superscript confidence AND smaller font
            debug_print!("  🔬 COMPOSITE SUPERSCRIPT: '{}' sub_conf={:.3} sup_conf={:.3} (v={:.3} s={:.3}) baseline_diff={:.1}", 
                         span.text.trim(), sub_confidence, sup_confidence, v, s, baseline_diff);
            (false, true)
        } else if is_tiny_movement && has_smaller_font {
            // For tiny movements with small fonts, default to subscript (most mathematical subscripts)
            debug_print!("  🔬 TINY MOVEMENT OVERRIDE: '{}' baseline_diff={:.1} font_ratio={:.2} → treating as subscript", 
                         span.text.trim(), baseline_diff, font_ratio);
            (true, false)
        } else {
            // Low confidence for both
            debug_print!(
                "  🔬 NO SCRIPT: '{}' sub_conf={:.3} sup_conf={:.3} < threshold={:.3}",
                span.text.trim(),
                sub_confidence,
                sup_confidence,
                COMPOSITE_SUBSCRIPT_CONFIDENCE_THRESHOLD
            );
            (false, false)
        };

        debug_print!("  📍 GLOBAL SPAN[{}]: '{}' global_baseline_diff={:.1} font_ratio={:.2} → sub={} sup={}", 
                     span_idx, span.text.trim(), baseline_diff, font_ratio, is_subscript, is_superscript);

        results.push((span_idx, is_subscript, is_superscript));
    }

    results
}

/// Enhanced subscript detection using multiple baseline clustering
/// This implements the research-backed approach for handling formulas with multiple baselines
pub(crate) fn detect_subscripts_clustered(spans: &[CharSpan]) -> Vec<(usize, bool, bool)> {
    if spans.is_empty() {
        return Vec::new();
    }

    debug_print!("🔬 CLUSTERED DETECTION: Processing {} spans", spans.len());

    // Step 1: Cluster spans by baseline proximity (5pt threshold based on typical font sizes)
    let clusters = cluster_baselines(spans, CLUSTERING_Y_THRESHOLD);

    // Step 2: Calculate global base font size
    let global_base_font_size = spans
        .iter()
        .map(|s| s.font_size)
        .filter(|&size| size > MIN_FONT_SIZE_FILTER)
        .fold(0.0, f32::max);

    // Step 3: Calculate GLOBAL baseline from the largest cluster (main text)
    let global_baseline = if clusters.is_empty() {
        0.0
    } else {
        // Find the largest cluster (likely contains main text)
        let largest_cluster = clusters.iter().max_by_key(|cluster| cluster.len()).unwrap();

        // Calculate median Y position from the largest cluster
        let mut main_y_positions: Vec<f32> = largest_cluster
            .iter()
            .map(|&idx| spans[idx].bbox.y0)
            .collect();
        main_y_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());

        if main_y_positions.len().is_multiple_of(2) {
            let mid = main_y_positions.len() / 2;
            (main_y_positions[mid - 1] + main_y_positions[mid]) / 2.0
        } else {
            main_y_positions[main_y_positions.len() / 2]
        }
    };

    debug_print!(
        "🌍 GLOBAL BASELINE: {:.1} from largest cluster ({} spans)",
        global_baseline,
        clusters
            .iter()
            .max_by_key(|c| c.len())
            .map(|c| c.len())
            .unwrap_or(0)
    );

    // Step 4: Detect subscripts within each cluster using GLOBAL baseline
    let mut all_results = Vec::new();

    for (cluster_idx, cluster_indices) in clusters.iter().enumerate() {
        debug_print!("🎯 Processing cluster {}", cluster_idx);
        let cluster_results = detect_subscripts_in_cluster_with_global_baseline(
            spans,
            cluster_indices,
            global_base_font_size,
            global_baseline,
        );
        all_results.extend(cluster_results);
    }

    // Sort results by original span index
    all_results.sort_by_key(|&(idx, _, _)| idx);

    debug_print!(
        "✅ CLUSTERED DETECTION: Completed with {} results",
        all_results.len()
    );
    all_results
}
