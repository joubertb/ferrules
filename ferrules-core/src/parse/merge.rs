use std::collections::HashMap;

use crate::{
    blocks::{Block, BlockType, FormulaBlock, ImageBlock, List, TextBlock, Title, TitleLevel},
    debug_print,
    entities::{Element, ElementID, ElementType, Line, PageID},
    layout::model::LayoutBBox,
    sentence_detection::detect_sentence_ends,
};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Pre-compiled regex for figure caption pattern detection
    static ref FIGURE_CAPTION_REGEX: Regex = Regex::new(r"^(?i)(Figure|Fig\.|Image)\s+[A-Za-z0-9]+[A-Za-z]?\s*[:.]").unwrap();

    /// Pre-compiled regex for footer/footnote pattern detection
    static ref FOOTER_PATTERN_REGEX: Regex = Regex::new(r"^(\d+\s|<sup>\d+</sup>\s?)|(https?://)").unwrap();
}

/// Apply word-level font corrections to text
fn apply_corrections_to_text(text: String) -> String {
    // Apply font corrections first
    crate::font_analysis::correct_assembled_text(&text)
}

/// Join lines with smart word-break handling
///
/// When joining lines, handles two cases:
/// 1. Word split across lines without hyphen (e.g., "ye" + "t" → "yet")
/// 2. Hyphenated line breaks (e.g., "No-" + "tably" → "Notably")
/// 3. Multi-fragment hyphenated words (e.g., "conver- sa tions" → "conversations")
#[cfg(feature = "correction-engine")]
fn join_lines_smart(lines: &[String]) -> String {
    use crate::font_analysis::dictionary::SmartCorrector;

    if lines.is_empty() {
        return String::new();
    }

    let mut result = lines[0].clone();

    for line in lines.iter().skip(1) {
        if line.is_empty() {
            continue;
        }

        // Get the last word fragment of current result and first word fragment of next line
        let result_word_start = result
            .rfind(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
            .map(|i| i + 1)
            .unwrap_or(0);
        let word_before = &result[result_word_start..];

        let line_word_end = line
            .find(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
            .unwrap_or(line.len());
        let word_after = &line[..line_word_end];

        // Case 1: Check for hyphenated line break (e.g., "No-" + "tably" → "Notably")
        let hyphen_join = if word_before.ends_with('-')
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
            // Try joining without the hyphen
            let word_before_no_hyphen = &word_before[..word_before.len() - 1];
            let joined = format!("{}{}", word_before_no_hyphen, word_after);
            if joined.len() <= 20 && SmartCorrector::is_valid_word(&joined) {
                Some(word_before_no_hyphen.len())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(chars_to_remove) = hyphen_join {
            // Remove the hyphen from result and join without space
            let remove_from = result.len() - (word_before.len() - chars_to_remove);
            result.truncate(remove_from);
            result.push_str(line);
            continue;
        }

        // Case 2: Check for word split without hyphen (e.g., "ye" + "t" → "yet")
        let should_join_without_space = !word_before.is_empty()
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
            && crate::spacing::should_join_fragments(word_before, word_after);

        if should_join_without_space {
            // Join without space - word was split across lines
            result.push_str(line);
        } else {
            // Normal join with space
            result.push(' ');
            result.push_str(line);
        }
    }

    // Post-process: fix hyphenated multi-fragment words (e.g., "conver- sa tions" → "conversations")
    fix_hyphenated_fragments(&mut result);

    result
}

/// Fix hyphenated multi-fragment words in text
/// Handles patterns like "word- x y" where "wordxy" is a valid word
#[cfg(feature = "correction-engine")]
fn fix_hyphenated_fragments(text: &mut String) {
    use crate::font_analysis::dictionary::SmartCorrector;

    // Find patterns like "word- " (hyphen followed by space)
    let chars: Vec<char> = text.chars().collect();
    let mut new_text = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        // Look for "X- " pattern where X is alphabetic
        if chars[i] == '-'
            && i > 0
            && chars[i - 1].is_alphabetic()
            && i + 1 < chars.len()
            && chars[i + 1] == ' '
        {
            // Find the word before the hyphen
            let word_start = new_text
                .rfind(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
                .map(|idx| idx + 1)
                .unwrap_or(0);
            let word_before = &new_text[word_start..];

            // Find fragments after the hyphen (up to 3 fragments)
            let mut fragments: Vec<String> = Vec::new();
            let mut j = i + 2; // Skip "- "
            while j < chars.len() && fragments.len() < 3 {
                if chars[j].is_whitespace() {
                    j += 1;
                    continue;
                }
                // Collect a fragment
                let frag_start = j;
                while j < chars.len()
                    && !chars[j].is_whitespace()
                    && !matches!(chars[j], ',' | '.' | ';' | ':')
                {
                    j += 1;
                }
                if j > frag_start {
                    fragments.push(chars[frag_start..j].iter().collect());
                }
                // Stop at punctuation (but not whitespace)
                if j < chars.len() && matches!(chars[j], ',' | '.' | ';' | ':') {
                    break;
                }
            }

            // Try joining word_before with fragments
            if !fragments.is_empty() {
                let joined: String = format!("{}{}", word_before, fragments.join(""));
                if joined.len() <= 25 && SmartCorrector::is_valid_word(&joined) {
                    // Replace word_before with joined word and skip the hyphen and fragments
                    new_text.truncate(word_start);
                    new_text.push_str(&joined);
                    // Calculate how many chars to skip
                    let skip_len: usize =
                        2 + fragments.iter().map(|f| f.len()).sum::<usize>() + fragments.len() - 1;
                    i += 1 + skip_len;
                    continue;
                }
            }
        }

        new_text.push(chars[i]);
        i += 1;
    }

    *text = new_text;
}

#[cfg(not(feature = "correction-engine"))]
fn join_lines_smart(lines: &[String]) -> String {
    lines.join(" ")
}

/// Concatenate CharSpans within a line with proper spacing
/// Adds spaces between spans when there's a horizontal or vertical gap
/// Also prevents spurious spaces within words using dictionary validation
fn concatenate_spans_with_spacing(line_spans: &[crate::entities::CharSpan]) -> String {
    if line_spans.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    // Track the start of the current word (index in result where last space/punctuation was)
    let mut word_start_idx: usize = 0;

    for (i, span) in line_spans.iter().enumerate() {
        if i == 0 {
            // First span - just add the text
            result.push_str(&span.text);
        } else {
            let prev_span = &line_spans[i - 1];

            // Check horizontal gap (words on same line)
            let _x_gap = span.bbox.x0 - prev_span.bbox.x1;
            // Check vertical gap (wrapped text)
            let _y_diff = (span.bbox.y0 - prev_span.bbox.y0).abs();

            // Use common font-aware spacing logic
            let needs_space = crate::spacing::should_add_space_between_spans(prev_span, span, 5.0);

            if needs_space {
                // Before adding a space, check if joining would create a valid word.
                // Only apply word-join logic for same-font spans. Different fonts indicate
                // different semantic entities (e.g., math variable "D" in italic vs body
                // text "is" in roman) — the space between them is intentional.
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

                let should_skip_space =
                    same_font && should_skip_space_for_word_join(&result, word_start_idx, span);

                if should_skip_space {
                    // Don't add space - join the word fragments
                    result.push_str(&span.text);
                } else {
                    result.push(' ');
                    word_start_idx = result.len(); // New word starts after space
                    result.push_str(&span.text);
                }
            } else {
                result.push_str(&span.text);
            }
        }

        // Update word_start_idx if span ends with whitespace or punctuation
        if let Some(last_boundary) = span
            .text
            .rfind(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
        {
            // Adjust word_start_idx to point to after the boundary in the result
            word_start_idx = result.len() - span.text.len() + last_boundary + 1;
        }
    }

    result
}

/// Check if we should skip adding a space because joining creates a valid word.
/// Extracts word fragments from context and delegates to `should_join_fragments`.
#[cfg(feature = "correction-engine")]
fn should_skip_space_for_word_join(
    result: &str,
    word_start_idx: usize,
    next_span: &crate::entities::CharSpan,
) -> bool {
    // Get the word fragment accumulated so far
    let word_so_far = if word_start_idx < result.len() {
        &result[word_start_idx..]
    } else {
        return false;
    };

    // Get the first word fragment from the next span
    let next_word_start = next_span
        .text
        .split(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';')
        .next()
        .unwrap_or(&next_span.text);

    crate::spacing::should_join_fragments(word_so_far, next_word_start)
}

#[cfg(not(feature = "correction-engine"))]
fn should_skip_space_for_word_join(
    _result: &str,
    _word_start_idx: usize,
    _next_span: &crate::entities::CharSpan,
) -> bool {
    false
}

/// This constant defines the minimum required intersection ratio between the bounding box of an
/// OCR-detected text line and a text block detected through layout analysis.
/// This approach ensures that only text lines significantly overlapping with a layout block are
/// paired, thus improving the accuracy of OCR-text and layout alignment.
const MIN_INTERSECTION_LAYOUT: f32 = 0.5;

/// Weights used for calculating distances between bounding boxes in layout analysis
/// X_WEIGHT is weighted higher (5.0) to prioritize horizontal alignment
/// Y_WEIGHT is weighted lower (1.0) to be more lenient with vertical spacing
const LAYOUT_DISTANCE_X_WEIGHT: f32 = 5.0;
const LAYOUT_DISTANCE_Y_WEIGHT: f32 = 1.0;

/// Maximum allowable distance between a text line and a layout block for assignment.
/// If the weighted distance (using X_WEIGHT and Y_WEIGHT) between a text line and
/// the nearest layout block exceeds this threshold, the line will not be assigned to any block.
/// This helps prevent incorrect assignments of text lines that are too far from layout blocks.
const MAXIMUM_ASSIGNMENT_DISTANCE: f32 = 20.0;

/// Maximum distance (in points) that an unmatched line can start BEFORE the last element's
/// right edge and still be considered a continuation. A small negative value allows for minor
/// overlap due to bbox imprecision.
const UNMATCHED_LINE_X_OVERLAP_TOLERANCE: f32 = -5.0;

/// Maximum distance (in points) that an unmatched line can start PAST the last element's
/// right edge and still be considered a continuation. Limits how far right a text fragment
/// can overflow a layout block boundary before being treated as unrelated.
const UNMATCHED_LINE_MAX_X_OVERSHOOT: f32 = 100.0;

/// Detects if text starts with a figure caption pattern
/// TODO: WORKAROUND - The ONNX model should classify these as ElementType::Caption
/// This should be removed once the model is retrained to properly identify captions
fn is_figure_caption(text: &str) -> bool {
    // Use regex to match actual figure captions (with colon or period after label)
    // This prevents false positives like "Figure 1 presents..." which are text references
    // Valid patterns: "Figure 1:", "Fig. A:", "Figure 2B.", "Image 3:", etc.
    FIGURE_CAPTION_REGEX.is_match(text.trim())
}

/// Detects text blocks that are likely embedded within a figure
/// TODO: WORKAROUND - The ONNX model should detect complete figure boundaries
/// including embedded text, not just the formula/image portion
fn is_likely_figure_embedded_text(element: &Element) -> bool {
    // Check if text block is spatially between main columns (figure area)
    // Main text columns typically at x ≈ 50-75 or x ≈ 314
    // Figure text often at x ≈ 327-483
    let block_x = element.bbox.x0;
    let is_between_columns = block_x > 300.0 && block_x < 550.0;

    // Check for attribution patterns
    let text = &element.text_block.text;
    let has_attribution = text.contains("(from ") || text.contains("(source:");

    is_between_columns || has_attribution
}

/// Detects if a text block is likely embedded within a figure (works on Blocks)
fn is_likely_figure_embedded_text_from_block(block: &Block) -> bool {
    let block_x = block.bbox.x0;
    let is_between_columns = block_x > 300.0 && block_x < 550.0;

    if let crate::blocks::BlockType::TextBlock(text) = &block.kind {
        let has_attribution = text.text.contains("(from ") || text.text.contains("(source:");
        is_between_columns || has_attribution
    } else {
        false
    }
}

fn merge_or_create_elements(
    elements: &mut Vec<Element>,
    line: &Line,
    line_layout_block: &LayoutBBox,
    page_id: PageID,
) {
    if elements.is_empty() {
        let mut el = Element::from_layout_block(0, line_layout_block, page_id);
        el.push_line(line);
        elements.push(el);
        return;
    }

    // let last_el = elements.last_mut().unwrap();
    let matched_element = elements
        .iter_mut()
        .find(|e| e.layout_block_id == line_layout_block.id);

    match matched_element {
        Some(el) => {
            el.push_line(line);
        }
        None => {
            let mut element =
                Element::from_layout_block(elements.len() + 1, line_layout_block, page_id);
            element.push_line(line);
            elements.push(element);
        }
    }
}
/// Merges lines into blocks based on their layout, maintaining the order of lines.
///
/// This function takes a list of text boxes representing layout bounding boxes that contain text,
/// and a list of lines (which could be obtained from OCR or  PDF library pdfium2,
/// and merges these lines into blocks. The merging is done based on the intersection
/// of each line with the layout bounding boxes.
///
/// NOTE: The function iterates through lines to maintain global layout order as both OCR and pdfium return lines
/// in correct order
pub(crate) fn merge_lines_layout(
    layout_boxes: &[LayoutBBox],
    lines: &[Line],
    page_id: usize,
) -> anyhow::Result<Vec<Element>> {
    let line_block_iterator = lines.iter().map(|line| {
        // TODO: the max here is sometimes very far away from the line.
        // ex: megatrends.pdf, header is categorized as text-block but the intersection  happens
        //
        // Get max intersection block for the line
        let max_intersection_bbox = layout_boxes.iter().max_by(|a, b| {
            let a_intersection = a.bbox.intersection(&line.bbox);
            let b_intersection = b.bbox.intersection(&line.bbox);

            a_intersection.partial_cmp(&b_intersection).unwrap()
        });
        // Get min distance block for the line
        let min_distance_block = layout_boxes.iter().min_by(|a, b| {
            let a_intersection = a.bbox.distance(
                &line.bbox,
                LAYOUT_DISTANCE_X_WEIGHT,
                LAYOUT_DISTANCE_Y_WEIGHT,
            );
            let b_intersection = b.bbox.distance(
                &line.bbox,
                LAYOUT_DISTANCE_X_WEIGHT,
                LAYOUT_DISTANCE_Y_WEIGHT,
            );
            a_intersection.partial_cmp(&b_intersection).unwrap()
        });
        let max_intersection_bbox = max_intersection_bbox.and_then(|b| {
            if line.bbox.intersection(&b.bbox) / line.bbox.area() > MIN_INTERSECTION_LAYOUT {
                Some(b)
            } else {
                None
            }
        });
        // Compare based on distance
        let matched_block = if max_intersection_bbox.is_none() {
            min_distance_block.and_then(|b| {
                if b.bbox.distance(
                    &line.bbox,
                    LAYOUT_DISTANCE_X_WEIGHT,
                    LAYOUT_DISTANCE_Y_WEIGHT,
                ) < MAXIMUM_ASSIGNMENT_DISTANCE
                {
                    Some(b)
                } else {
                    None
                }
            })
        } else {
            max_intersection_bbox
        };
        (line, matched_block)
    });

    let mut headers = Vec::new();
    let mut elements = Vec::new();
    let mut footers = Vec::new();
    for (line, layout_block) in line_block_iterator {
        match &layout_block.as_ref() {
            Some(&line_layout_block) => match line_layout_block.label {
                "Page-header" => {
                    merge_or_create_elements(&mut headers, line, line_layout_block, page_id);
                }
                "Page-footer" => {
                    merge_or_create_elements(&mut footers, line, line_layout_block, page_id);
                }
                _ => {
                    merge_or_create_elements(&mut elements, line, line_layout_block, page_id);
                }
            },
            // Line is detected but isn't assignable to any layout element.
            // This happens when text fragments extend past a layout block's
            // bounding box (e.g., long lines where the tail overflows the
            // ONNX-detected block boundary). Append to the last element if
            // the line overlaps vertically AND starts near/past the element's
            // right edge, indicating a text continuation.
            None => {
                if let Some(last_el) = elements.last_mut() {
                    let y_overlaps =
                        line.bbox.y0 < last_el.bbox.y1 && line.bbox.y1 > last_el.bbox.y0;
                    // How far past the element's right edge the line starts
                    let x_past_right = line.bbox.x0 - last_el.bbox.x1;
                    if y_overlaps
                        && x_past_right > UNMATCHED_LINE_X_OVERLAP_TOLERANCE
                        && x_past_right < UNMATCHED_LINE_MAX_X_OVERSHOOT
                    {
                        last_el.push_line(line);
                    }
                }
            }
        }
    }
    elements.append(&mut footers);

    headers.append(&mut elements);
    Ok(headers)
}

pub(crate) fn merge_remaining(
    elements: &mut Vec<Element>,
    remaining: &[&LayoutBBox],
    page_id: PageID,
) {
    for layout_box in remaining {
        let closest_block = elements
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let a_intersection = a.bbox.distance(
                    &layout_box.bbox,
                    LAYOUT_DISTANCE_X_WEIGHT,
                    LAYOUT_DISTANCE_Y_WEIGHT,
                );
                let b_intersection = b.bbox.distance(
                    &layout_box.bbox,
                    LAYOUT_DISTANCE_X_WEIGHT,
                    LAYOUT_DISTANCE_Y_WEIGHT,
                );
                a_intersection.partial_cmp(&b_intersection).unwrap()
            })
            .map(|(index, _)| index)
            .unwrap_or(elements.len());

        elements.insert(
            closest_block,
            Element::from_layout_block(elements.len(), layout_box, page_id),
        );
    }
}

/// Post-process blocks to create Figure blocks from Image blocks with nearby embedded text
/// This handles cases where the ONNX model correctly identifies images but misses embedded text
fn post_process_figure_blocks(blocks: &mut Vec<Block>) {
    let mut indices_to_remove = Vec::new();
    let mut figures_to_add = Vec::new();

    // Find Image blocks and check for nearby embedded text
    for (i, block) in blocks.iter().enumerate() {
        if let crate::blocks::BlockType::Image(image_block) = &block.kind {
            let mut figure_elements = Vec::new();
            let mut figure_bbox = block.bbox.clone();
            let mut embedded_indices = Vec::new();

            // Look backwards for embedded text blocks
            for j in (0..i).rev() {
                if let crate::blocks::BlockType::TextBlock(text_block) = &blocks[j].kind {
                    // Check if this text block should be part of the figure
                    if is_likely_figure_embedded_text_from_block(&blocks[j]) {
                        figure_elements.insert(0, text_block.text.clone());
                        figure_bbox.merge(&blocks[j].bbox);
                        embedded_indices.push(j);
                    } else {
                        // Stop when we hit regular text that's not in the figure area
                        break;
                    }
                } else {
                    // Stop at any other block type
                    break;
                }
            }

            // If we found embedded text, create a Figure block
            if !figure_elements.is_empty() {
                let figure_block = Block {
                    id: block.id,
                    kind: crate::blocks::BlockType::Figure(crate::blocks::FigureBlock {
                        id: image_block.id,
                        embedded_texts: figure_elements,
                        image_bbox: Some(block.bbox.clone()),
                        caption: image_block.caption.clone(),
                        image_path: None,
                    }),
                    pages_id: block.pages_id.clone(),
                    bbox: figure_bbox,
                };

                figures_to_add.push((i, figure_block));
                indices_to_remove.push(i); // Remove the original Image block
                indices_to_remove.extend(embedded_indices); // Remove the embedded text blocks
            }
        }
    }

    // Sort indices in reverse order for safe removal
    indices_to_remove.sort_by(|a, b| b.cmp(a));
    indices_to_remove.dedup();

    // Remove blocks (in reverse order to maintain indices)
    for &index in &indices_to_remove {
        if index < blocks.len() {
            blocks.remove(index);
        }
    }

    // Add the Figure blocks
    for (original_index, figure_block) in figures_to_add {
        // Calculate where to insert (accounting for removed blocks)
        let removed_before = indices_to_remove
            .iter()
            .filter(|&&idx| idx < original_index)
            .count();
        let insert_index = original_index.saturating_sub(removed_before);

        if insert_index <= blocks.len() {
            blocks.insert(insert_index, figure_block);
        } else {
            blocks.push(figure_block);
        }
    }
}

pub(crate) fn merge_elements_into_blocks(
    elements: Vec<Element>,
    title_level: HashMap<(PageID, ElementID), TitleLevel>,
    page_heights: HashMap<PageID, f32>,
) -> anyhow::Result<Vec<Block>> {
    // FIXME: This function contains WORKAROUNDS for ONNX model limitations
    // The layout detection model has three major issues:
    // 1. It incorrectly classifies figure captions as "Text" instead of "Caption"
    // 2. It fails to detect complete figure boundaries when figures contain embedded text
    // 3. It misclassifies footnote references at page bottom as "Text" instead of "Page-footer"
    //
    // Proper fix: Retrain the ONNX model to:
    // - Correctly classify "Figure N:" patterns as Caption type
    // - Detect complete figure boundaries including ALL embedded text
    // - Properly associate captions with their corresponding images
    // - Recognize footnote patterns and bottom positioning as Page-footer elements
    //
    // This heuristic-based workaround should be removed once the model is fixed
    // Track at: https://github.com/[repo]/issues/[TODO: create issue]

    debug_print!(
        "🔧 merge_elements_into_blocks CALLED with {} elements",
        elements.len()
    );
    debug_print!("🔧 TEST: Debug is working in merge function");
    let mut element_it = elements.into_iter().peekable();

    let mut blocks: Vec<Block> = Vec::new();
    let mut block_id = 0;
    let mut image_id = 0;
    while let Some(mut curr_el) = element_it.next() {
        // Debug all elements to see their types
        debug_print!(
            "🔧 Processing element {}: type={:?}, text='{}', line_spans={}",
            curr_el.id,
            curr_el.kind,
            curr_el.text_block.text.chars().take(50).collect::<String>(),
            curr_el.line_spans.len()
        );

        if matches!(curr_el.kind, crate::entities::ElementType::Image) {
            debug_print!("🖼️ Found Image element!");
        }

        match &mut curr_el.kind {
            ElementType::Text => {
                debug_print!(
                    "📄 TEXT ELEMENT: {}",
                    curr_el.text_block.text.chars().take(50).collect::<String>()
                );

                // WORKAROUND: Enhanced figure detection logic
                // Check if this is a figure caption, or if we should start collecting figure elements
                if is_figure_caption(&curr_el.text_block.text) {
                    debug_print!(
                        "🖼️ DETECTED figure caption: {}",
                        &curr_el.text_block.text.chars().take(80).collect::<String>()
                    );

                    // Look backward for elements that might be part of this figure
                    let mut figure_elements = Vec::new();
                    let mut figure_bbox = curr_el.bbox.clone();
                    let mut figure_image_bbox = None;

                    // Collect recent blocks that might be part of this figure
                    let mut blocks_to_remove = Vec::new();
                    for (i, existing_block) in blocks.iter().enumerate().rev() {
                        match &existing_block.kind {
                            crate::blocks::BlockType::Image(img_block) => {
                                // Skip ImageBlocks that already have captions - they were processed by merge logic
                                if img_block.caption.is_some() {
                                    debug_print!("🖼️ Skipping Image block that already has caption - processed by merge logic");
                                    break; // Stop search - don't include caption-complete ImageBlocks in Figure blocks
                                } else {
                                    debug_print!("🖼️ Found Image block to include in figure");
                                    figure_image_bbox = Some(existing_block.bbox.clone());
                                    figure_bbox.merge(&existing_block.bbox);
                                    blocks_to_remove.push(i);
                                    break; // Stop after finding the image
                                }
                            }
                            crate::blocks::BlockType::TextBlock(text) => {
                                // Check if this text block is in the figure area (using spatial heuristics)
                                let text_x = existing_block.bbox.x0;
                                let is_in_figure_area = text_x > 300.0 && text_x < 550.0;
                                let has_attribution =
                                    text.text.contains("(from ") || text.text.contains("(source:");

                                if is_in_figure_area || has_attribution {
                                    debug_print!(
                                        "🖼️ Including text block in figure: {}",
                                        text.text.chars().take(50).collect::<String>()
                                    );
                                    figure_elements.insert(0, text.text.clone()); // Insert at beginning to maintain order
                                    figure_bbox.merge(&existing_block.bbox);
                                    blocks_to_remove.push(i);
                                } else {
                                    // Stop when we hit regular text that's not in the figure area
                                    break;
                                }
                            }
                            _ => break, // Stop at any other block type
                        }
                    }

                    // Remove the blocks that we're incorporating into the figure (in reverse order to maintain indices)
                    for &i in blocks_to_remove.iter().rev() {
                        blocks.remove(i);
                    }

                    // Process the figure caption text
                    let caption_text = if !curr_el.line_spans.is_empty() {
                        debug_print!(
                            "🖼️ Processing figure caption with {} line_spans",
                            curr_el.line_spans.len()
                        );
                        let original_text = curr_el
                            .line_spans
                            .iter()
                            .map(|line_spans| concatenate_spans_with_spacing(line_spans))
                            .collect::<Vec<String>>()
                            .join(" ");
                        crate::modtext::process_text_with_spans(&original_text, &curr_el.line_spans)
                    } else {
                        apply_corrections_to_text(curr_el.text_block.text.clone())
                    };

                    // Create a comprehensive Figure block
                    let figure_block = Block {
                        id: block_id,
                        kind: crate::blocks::BlockType::Figure(crate::blocks::FigureBlock {
                            id: image_id,
                            embedded_texts: figure_elements,
                            image_bbox: figure_image_bbox,
                            caption: Some(caption_text),
                            image_path: None,
                        }),
                        pages_id: vec![curr_el.page_id],
                        bbox: figure_bbox,
                    };

                    debug_print!("🖼️ Created Figure block with {} embedded texts", {
                        if let crate::blocks::BlockType::Figure(ref fig) = figure_block.kind {
                            fig.embedded_texts.len()
                        } else {
                            0
                        }
                    });

                    image_id += 1;
                    block_id += 1;
                    blocks.push(figure_block);
                    continue;
                } else if is_likely_figure_embedded_text(&curr_el) {
                    // This might be embedded figure text, but we need to look ahead to see if there's a figure caption
                    // For now, we'll process it as regular text and let the caption detection handle it
                    debug_print!("🖼️ Potential figure embedded text detected (will be handled by caption detection)");
                }

                // Apply subscript detection to TEXT elements with line_spans (mathematical content)
                // Get truly original text by reconstructing from raw CharSpans (before any HTML tag processing)
                let original_text = if !curr_el.line_spans.is_empty() {
                    // Reconstruct original text from CharSpans
                    let lines: Vec<String> = curr_el
                        .line_spans
                        .iter()
                        .map(|line_spans| {
                            // Space-aware concatenation of spans within a line
                            concatenate_spans_with_spacing(line_spans)
                        })
                        .collect();

                    // Smart line joining that handles word breaks across lines
                    join_lines_smart(&lines)
                } else {
                    // Fallback to current text if no spans available
                    curr_el.text_block.text.clone()
                };

                let processed_text = if !curr_el.line_spans.is_empty() {
                    debug_print!(
                        "🎯 TEXT WITH SPANS: Processing {} line_spans for subscript detection",
                        curr_el.line_spans.len()
                    );

                    // Apply subscript detection using CharSpans but WITHOUT formula wrapper (for TEXT elements)
                    crate::modtext::process_text_with_spans(&original_text, &curr_el.line_spans)
                } else {
                    // No spans available, use basic text correction only
                    apply_corrections_to_text(original_text.clone())
                };

                if block_id == 54 {
                    debug_print!("🎯 BLOCK 54 BEFORE processing: '{}'", original_text);
                    debug_print!("🎯 BLOCK 54 AFTER processing: '{}'", processed_text);
                    debug_print!(
                        "🎯 BLOCK 54 Text changed: {}",
                        original_text != processed_text
                    );
                }

                let corrected_text = processed_text;

                let mut text_block = Block {
                    id: block_id,
                    kind: crate::blocks::BlockType::TextBlock(TextBlock {
                        text: corrected_text.clone(),
                        // Always set fertext to preserve original text for char_span alignment
                        // char_spans are indexed to the original text, so fertext must always exist
                        fertext: Some(original_text.clone()),
                        char_spans: curr_el.get_serializable_char_spans(),
                        sentence_ends: Vec::new(), // Will be computed after merging
                    }),
                    pages_id: vec![curr_el.page_id],
                    bbox: curr_el.bbox,
                };
                // Check to see if we have another text block that is close
                loop {
                    let should_merge = if let Some(next_el) = element_it.peek() {
                        matches!(next_el.kind, crate::entities::ElementType::Text)
                            && (text_block.bbox.distance(&next_el.bbox, 1.0, 1.0)
                                < MAXIMUM_ASSIGNMENT_DISTANCE)
                    } else {
                        false
                    };

                    if should_merge {
                        let next_el = element_it.next().unwrap();
                        if let BlockType::TextBlock(text_content) = &mut text_block.kind {
                            // Get current text length for char_span offset adjustment
                            let current_len = text_content.text.chars().count();
                            text_content.text.push('\n');
                            text_content.text.push_str(&next_el.text_block.text);

                            // Merge fertext for accurate sentence detection
                            // next_el.text_block.text is the original text before corrections
                            let next_original_text = &next_el.text_block.text;
                            let fertext_len = text_content
                                .fertext
                                .as_ref()
                                .map(|f| f.chars().count())
                                .unwrap_or(current_len);
                            if let Some(ref mut fertext) = text_content.fertext {
                                fertext.push('\n');
                                fertext.push_str(next_original_text);
                            } else {
                                // Current has no fertext - create one from current text + next original
                                let current_original = text_content.text.clone();
                                let mut merged = current_original;
                                merged.push('\n');
                                merged.push_str(next_original_text);
                                text_content.fertext = Some(merged);
                            }

                            // Collect char_spans from next element with adjusted offsets
                            let offset = fertext_len + 1; // +1 for newline
                            for span in next_el.get_serializable_char_spans() {
                                text_content.char_spans.push(
                                    crate::entities::SerializableCharSpan {
                                        bbox: span.bbox,
                                        text: span.text,
                                        char_start: span.char_start + offset,
                                        char_end: span.char_end + offset,
                                        page_id: span.page_id,
                                    },
                                );
                            }
                        }
                        text_block.bbox.merge(&next_el.bbox);
                    } else {
                        break;
                    }
                }

                // Compute sentence end positions for the final merged text
                if let BlockType::TextBlock(text_content) = &mut text_block.kind {
                    // Use fertext (original text) for sentence detection since char_spans map to it
                    let text_for_detection =
                        text_content.fertext.as_ref().unwrap_or(&text_content.text);
                    text_content.sentence_ends = detect_sentence_ends(text_for_detection);
                }

                block_id += 1;
                blocks.push(text_block);
            }
            ElementType::Formula => {
                // Get raw original text from CharSpans or text_block
                let original_text = if !curr_el.line_spans.is_empty() {
                    curr_el
                        .line_spans
                        .iter()
                        .map(|line_spans| concatenate_spans_with_spacing(line_spans))
                        .collect::<Vec<String>>()
                        .join(" ")
                } else {
                    curr_el.text_block.text.clone()
                };

                let formula_block = Block {
                    id: block_id,
                    kind: BlockType::Formula(FormulaBlock {
                        text: original_text,
                        formula_img: Some(format!("figures/formula_{}.png", block_id)),
                    }),
                    pages_id: vec![curr_el.page_id],
                    bbox: curr_el.bbox,
                };
                block_id += 1;
                blocks.push(formula_block);
            }
            ElementType::ListItem => {
                // Process first list item with HTML tag detection
                let (first_item_text, first_item_original) = if !curr_el.line_spans.is_empty() {
                    debug_print!(
                        "📋 LIST ITEM WITH SPANS: Processing first list item with {} line_spans for HTML tag detection",
                        curr_el.line_spans.len()
                    );
                    let original_text = curr_el
                        .line_spans
                        .iter()
                        .map(|line_spans| concatenate_spans_with_spacing(line_spans))
                        .collect::<Vec<String>>()
                        .join(" ");
                    let processed = crate::modtext::process_text_with_spans(
                        &original_text,
                        &curr_el.line_spans,
                    );
                    (processed, original_text)
                } else {
                    let original = curr_el.text_block.text.clone();
                    (apply_corrections_to_text(original.clone()), original)
                };

                let first_item_char_spans = curr_el.get_serializable_char_spans();
                let mut list_block = Block {
                    id: block_id,
                    kind: BlockType::ListBlock(List {
                        items: vec![crate::blocks::ListItem {
                            text: first_item_text,
                            fertext: Some(first_item_original),
                            char_spans: first_item_char_spans,
                        }],
                    }),
                    pages_id: vec![curr_el.page_id],
                    bbox: curr_el.bbox,
                };

                while let Some(next_el) = element_it.peek() {
                    // TODO: add constraint on gap between bounding boxes on all dimensions (l,r,b,t)
                    if matches!(next_el.kind, crate::entities::ElementType::ListItem) {
                        let next_el = element_it.next().unwrap();

                        // Process additional list item with HTML tag detection before merging
                        let (processed_item_text, item_original) = if !next_el.line_spans.is_empty()
                        {
                            debug_print!(
                                "📋 LIST ITEM WITH SPANS: Processing additional list item with {} line_spans for HTML tag detection",
                                next_el.line_spans.len()
                            );
                            let original_text = next_el
                                .line_spans
                                .iter()
                                .map(|line_spans| concatenate_spans_with_spacing(line_spans))
                                .collect::<Vec<String>>()
                                .join(" ");
                            let processed = crate::modtext::process_text_with_spans(
                                &original_text,
                                &next_el.line_spans,
                            );
                            (processed, original_text)
                        } else {
                            let original = next_el.text_block.text.clone();
                            (apply_corrections_to_text(original.clone()), original)
                        };

                        // Manually add the processed text to the list instead of using merge
                        if let BlockType::ListBlock(list) = &mut list_block.kind {
                            let item_char_spans = next_el.get_serializable_char_spans();
                            list.items.push(crate::blocks::ListItem {
                                text: processed_item_text,
                                fertext: Some(item_original),
                                char_spans: item_char_spans,
                            });
                        }
                        list_block.bbox.merge(&next_el.bbox);
                    } else {
                        break;
                    }
                }
                block_id += 1;
                blocks.push(list_block);
            }
            ElementType::FootNote | ElementType::Caption => {
                // We find the closest image and create and image block
                loop {
                    match element_it.peek() {
                        None => {
                            // last element -> transform to txt block and break
                            let original_text = curr_el.text_block.text.clone();
                            let char_spans = curr_el.get_serializable_char_spans();
                            let processed_text = apply_corrections_to_text(curr_el.text_block.text);
                            let sentence_ends = detect_sentence_ends(&original_text);

                            let text_block = Block {
                                id: block_id,
                                kind: crate::blocks::BlockType::TextBlock(TextBlock {
                                    text: processed_text.clone(),
                                    // Always set fertext for char_span alignment
                                    fertext: Some(original_text),
                                    char_spans,
                                    sentence_ends,
                                }),
                                pages_id: vec![curr_el.page_id],
                                bbox: curr_el.bbox,
                            };
                            element_it.next();
                            block_id += 1;
                            blocks.push(text_block);
                            break;
                        }
                        Some(next_el) => {
                            match &next_el.kind {
                                crate::entities::ElementType::FootNote
                                | crate::entities::ElementType::Caption => {
                                    // Merge this with a the caption
                                    curr_el.text_block.append_line(&next_el.text_block.text);
                                    element_it.next();
                                }
                                crate::entities::ElementType::Image => {
                                    curr_el.bbox.merge(&next_el.bbox);

                                    // FIXED: Apply script detection to Image caption (Caption→Image case)
                                    let caption_text = if !curr_el.line_spans.is_empty() {
                                        debug_print!("🖼️ IMAGE CAPTION: Processing caption with {} line_spans", curr_el.line_spans.len());
                                        let original_text = curr_el
                                            .line_spans
                                            .iter()
                                            .map(|line_spans| {
                                                concatenate_spans_with_spacing(line_spans)
                                            })
                                            .collect::<Vec<String>>()
                                            .join(" ");
                                        crate::modtext::process_text_with_spans(
                                            &original_text,
                                            &curr_el.line_spans,
                                        )
                                    } else {
                                        apply_corrections_to_text(curr_el.text_block.text.clone())
                                    };

                                    let img_block = Block {
                                        id: block_id,
                                        kind: BlockType::Image(ImageBlock {
                                            id: image_id,
                                            caption: Some(caption_text),
                                            image_path: None,
                                        }),
                                        pages_id: vec![next_el.page_id],
                                        bbox: curr_el.bbox,
                                    };
                                    image_id += 1;
                                    block_id += 1;
                                    blocks.push(img_block);
                                    element_it.next();
                                    break;
                                }
                                _ => {
                                    // This caption isn't associated with Image/Table, transform to textblock
                                    let original_text = curr_el.text_block.text.clone();
                                    let processed_text =
                                        apply_corrections_to_text(curr_el.text_block.text);
                                    let sentence_ends = detect_sentence_ends(&original_text);

                                    let text_block = Block {
                                        id: block_id,
                                        kind: crate::blocks::BlockType::TextBlock(TextBlock {
                                            text: processed_text.clone(),
                                            // Always set fertext for char_span alignment
                                            fertext: Some(original_text),
                                            char_spans: Vec::new(),
                                            sentence_ends,
                                        }),
                                        pages_id: vec![curr_el.page_id],
                                        bbox: curr_el.bbox,
                                    };
                                    block_id += 1;
                                    blocks.push(text_block);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            ElementType::Image => {
                debug_print!("🖼️ Processing Image element - checking for embedded text");

                // CAPTION→IMAGE MERGE: Check if there are any recent ImageBlocks with caption: None
                // that should be updated with a caption, or if there's a Caption-created block nearby
                let mut found_merge_target = false;

                // Check the last few blocks for potential merge targets
                for i in (0..blocks.len()).rev().take(3) {
                    // Check last 3 blocks
                    if let crate::blocks::BlockType::Image(img_block) = &blocks[i].kind {
                        if img_block.caption.is_none() {
                            // Check if this caption-less ImageBlock is close to current Image element
                            let vertical_distance = (curr_el.bbox.y0 - blocks[i].bbox.y1).abs();
                            if vertical_distance < 500.0 {
                                // Allow large gap - same page figures can be far apart
                                debug_print!(
                                    "🖼️ MERGE TARGET: Found ImageBlock (id={}) with caption: None, checking for nearby caption",
                                    blocks[i].id
                                );

                                // This Image element should replace the existing caption-less ImageBlock
                                // but first check if there's a Caption element following this Image
                                if let Some(next_el) = element_it.peek() {
                                    if let crate::entities::ElementType::Caption = &next_el.kind {
                                        if is_figure_caption(&next_el.text_block.text) {
                                            debug_print!(
                                                "🖼️ MERGE: Removing existing ImageBlock (id={}) and creating new one with caption",
                                                blocks[i].id
                                            );

                                            // Consume the Caption element
                                            let caption_el = element_it.next().unwrap();

                                            // Process the caption text with script detection
                                            let caption_text = if !caption_el.line_spans.is_empty()
                                            {
                                                debug_print!(
                                                    "🖼️ MERGE COMPLETE: Processing caption with {} line_spans",
                                                    caption_el.line_spans.len()
                                                );
                                                let original_text = caption_el
                                                    .line_spans
                                                    .iter()
                                                    .map(|line_spans| {
                                                        concatenate_spans_with_spacing(line_spans)
                                                    })
                                                    .collect::<Vec<String>>()
                                                    .join(" ");
                                                crate::modtext::process_text_with_spans(
                                                    &original_text,
                                                    &caption_el.line_spans,
                                                )
                                            } else {
                                                apply_corrections_to_text(
                                                    caption_el.text_block.text.clone(),
                                                )
                                            };

                                            // Remove the caption-less ImageBlock
                                            blocks.remove(i);

                                            // Create new ImageBlock with both Image and Caption
                                            let complete_block = Block {
                                                id: block_id,
                                                kind: crate::blocks::BlockType::Image(ImageBlock {
                                                    id: image_id,
                                                    caption: Some(caption_text),
                                                    image_path: None,
                                                }),
                                                pages_id: vec![curr_el.page_id],
                                                bbox: curr_el.bbox.clone(),
                                            };

                                            let merged_block_id = complete_block.id;
                                            image_id += 1;
                                            block_id += 1;
                                            blocks.push(complete_block);

                                            debug_print!(
                                                "🖼️ MERGE COMPLETE: Created ImageBlock (id={}) with caption from removed block",
                                                merged_block_id
                                            );

                                            found_merge_target = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        } else if img_block.caption.is_some() {
                            // Found Caption-created ImageBlock - merge logic as before
                            let vertical_distance = (curr_el.bbox.y0 - blocks[i].bbox.y1).abs();
                            if vertical_distance < 100.0 {
                                debug_print!(
                                    "🖼️ MERGE: Found Caption-created ImageBlock (id={}) with caption, merging with Image element",
                                    blocks[i].id
                                );

                                // Extract the caption from the incomplete block
                                let caption = img_block.caption.clone();

                                // Remove the incomplete Caption-created block
                                blocks.remove(i);

                                // Create complete ImageBlock with both caption and image
                                let complete_block = Block {
                                    id: block_id,
                                    kind: crate::blocks::BlockType::Image(ImageBlock {
                                        id: image_id,
                                        caption, // From Caption element
                                        image_path: None,
                                    }),
                                    pages_id: vec![curr_el.page_id],
                                    bbox: curr_el.bbox.clone(), // From Image element (correct)
                                };

                                let merged_block_id = complete_block.id;
                                image_id += 1;
                                block_id += 1;
                                blocks.push(complete_block);

                                debug_print!(
                                    "🖼️ MERGE COMPLETE: Created merged ImageBlock (id={}) with caption and correct bbox",
                                    merged_block_id
                                );

                                found_merge_target = true;
                                break;
                            }
                        }
                    }
                }

                if found_merge_target {
                    continue; // Skip normal Image processing
                }

                // Check if we should create a Figure block by looking for nearby embedded text
                let mut figure_elements = Vec::new();
                let mut figure_bbox = curr_el.bbox.clone();

                // Look backward for elements that might be part of this figure
                // We'll collect the indices but only remove them when we actually create a Figure block
                let blocks_to_check: Vec<_> = blocks.iter().enumerate().rev().collect();
                let mut blocks_to_remove = Vec::new();

                for (i, existing_block) in blocks_to_check {
                    debug_print!(
                        "🖼️ Checking block {} ({}): {:?}",
                        existing_block.id,
                        i,
                        &existing_block.kind
                    );
                    match &existing_block.kind {
                        crate::blocks::BlockType::TextBlock(text) => {
                            debug_print!(
                                "🖼️ TextBlock {} at x={:.1}: {}",
                                existing_block.id,
                                existing_block.bbox.x0,
                                text.text.chars().take(50).collect::<String>()
                            );
                            // Check if this text block is likely embedded in the figure
                            if is_likely_figure_embedded_text_from_block(existing_block) {
                                debug_print!(
                                    "🖼️ ✅ MATCHED - Found embedded text for figure: {}",
                                    text.text.chars().take(50).collect::<String>()
                                );
                                figure_elements.insert(0, text.text.clone()); // Insert at beginning to maintain order
                                figure_bbox.merge(&existing_block.bbox);
                                blocks_to_remove.push(i);
                                debug_print!(
                                    "🖼️ Added block {} to removal list at index {}",
                                    existing_block.id,
                                    i
                                );
                            } else {
                                debug_print!("🖼️ ❌ NO MATCH - Not embedded text, stopping search");
                                // Stop when we hit regular text that's not in the figure area
                                break;
                            }
                        }
                        _ => {
                            debug_print!("🖼️ Non-TextBlock, stopping search");
                            break; // Stop at any other block type
                        }
                    }
                }

                match element_it.peek() {
                    None => {
                        // Check if we found embedded text to create a Figure block
                        if !figure_elements.is_empty() {
                            debug_print!(
                                "🖼️ Removing {} blocks: {:?}",
                                blocks_to_remove.len(),
                                blocks_to_remove
                            );
                            // Remove the blocks that we're incorporating into the figure (in reverse order)
                            for &i in blocks_to_remove.iter().rev() {
                                if i < blocks.len() {
                                    debug_print!("🖼️ Removing block at index {}", i);
                                    blocks.remove(i);
                                } else {
                                    debug_print!(
                                        "🖼️ WARNING: Index {} out of bounds for removal",
                                        i
                                    );
                                }
                            }

                            let embedded_count = figure_elements.len();
                            debug_print!(
                                "🖼️ Created Figure block with {} embedded texts",
                                embedded_count
                            );
                            // Create Figure block with embedded texts
                            let block = Block {
                                id: block_id,
                                kind: crate::blocks::BlockType::Figure(
                                    crate::blocks::FigureBlock {
                                        id: image_id,
                                        embedded_texts: figure_elements,
                                        image_bbox: Some(curr_el.bbox.clone()),
                                        caption: None,
                                        image_path: None,
                                    },
                                ),
                                pages_id: vec![curr_el.page_id],
                                bbox: figure_bbox,
                            };
                            image_id += 1;
                            block_id += 1;
                            blocks.push(block);
                        } else {
                            // Regular Image block without embedded text
                            let block = Block {
                                id: block_id,
                                kind: crate::blocks::BlockType::Image(ImageBlock {
                                    id: image_id,
                                    caption: None,
                                    image_path: None,
                                }),
                                pages_id: vec![curr_el.page_id],
                                bbox: curr_el.bbox,
                            };
                            image_id += 1;
                            block_id += 1;
                            blocks.push(block);
                        }
                    }
                    Some(next_el) => {
                        match &next_el.kind {
                            // WORKAROUND: Check if next Text element is actually a figure caption
                            crate::entities::ElementType::Text => {
                                if is_figure_caption(&next_el.text_block.text) {
                                    // This Text is actually a figure caption that was misclassified
                                    let next_el = element_it.next().unwrap();
                                    curr_el.bbox.merge(&next_el.bbox);
                                    figure_bbox.merge(&next_el.bbox);

                                    // Process the figure caption with script detection
                                    let caption_text = if !next_el.line_spans.is_empty() {
                                        debug_print!(
                                            "🖼️ FIGURE CAPTION (misclassified Text): Processing caption with {} line_spans",
                                            next_el.line_spans.len()
                                        );
                                        let original_text = next_el
                                            .line_spans
                                            .iter()
                                            .map(|line_spans| {
                                                concatenate_spans_with_spacing(line_spans)
                                            })
                                            .collect::<Vec<String>>()
                                            .join(" ");
                                        crate::modtext::process_text_with_spans(
                                            &original_text,
                                            &next_el.line_spans,
                                        )
                                    } else {
                                        apply_corrections_to_text(next_el.text_block.text.clone())
                                    };

                                    // Check if we found embedded text to create a Figure block
                                    let block = if !figure_elements.is_empty() {
                                        // Remove the blocks that we're incorporating into the figure (in reverse order)
                                        debug_print!(
                                            "🖼️ Removing {} blocks, current blocks.len()={}",
                                            blocks_to_remove.len(),
                                            blocks.len()
                                        );
                                        for &i in blocks_to_remove.iter().rev() {
                                            debug_print!(
                                                "🖼️ Attempting to remove block at index {}",
                                                i
                                            );
                                            if i < blocks.len() {
                                                blocks.remove(i);
                                            } else {
                                                debug_print!("🖼️ WARNING: Index {} out of bounds for blocks.len()={}", i, blocks.len());
                                            }
                                        }

                                        let embedded_count = figure_elements.len();
                                        debug_print!("🖼️ Created Figure block with {} embedded texts and caption", embedded_count);
                                        // Create Figure block with embedded texts and caption
                                        Block {
                                            id: block_id,
                                            kind: crate::blocks::BlockType::Figure(
                                                crate::blocks::FigureBlock {
                                                    id: image_id,
                                                    embedded_texts: figure_elements,
                                                    image_bbox: Some(curr_el.bbox.clone()),
                                                    caption: Some(caption_text),
                                                    image_path: None,
                                                },
                                            ),
                                            pages_id: vec![curr_el.page_id],
                                            bbox: figure_bbox,
                                        }
                                    } else {
                                        // Regular Image block with caption
                                        Block {
                                            id: block_id,
                                            kind: crate::blocks::BlockType::Image(ImageBlock {
                                                id: image_id,
                                                caption: Some(caption_text),
                                                image_path: None,
                                            }),
                                            pages_id: vec![curr_el.page_id],
                                            bbox: curr_el.bbox,
                                        }
                                    };

                                    image_id += 1;
                                    block_id += 1;
                                    blocks.push(block);
                                } else {
                                    // Check if we found embedded text to create a Figure block
                                    let block = if !figure_elements.is_empty() {
                                        // Remove the blocks that we're incorporating into the figure (in reverse order)
                                        debug_print!(
                                            "🖼️ Removing {} blocks, current blocks.len()={}",
                                            blocks_to_remove.len(),
                                            blocks.len()
                                        );
                                        for &i in blocks_to_remove.iter().rev() {
                                            debug_print!(
                                                "🖼️ Attempting to remove block at index {}",
                                                i
                                            );
                                            if i < blocks.len() {
                                                blocks.remove(i);
                                            } else {
                                                debug_print!("🖼️ WARNING: Index {} out of bounds for blocks.len()={}", i, blocks.len());
                                            }
                                        }

                                        let embedded_count = figure_elements.len();
                                        debug_print!(
                                            "🖼️ Created Figure block with {} embedded texts",
                                            embedded_count
                                        );
                                        // Create Figure block with embedded texts
                                        Block {
                                            id: block_id,
                                            kind: crate::blocks::BlockType::Figure(
                                                crate::blocks::FigureBlock {
                                                    id: image_id,
                                                    embedded_texts: figure_elements,
                                                    image_bbox: Some(curr_el.bbox.clone()),
                                                    caption: None,
                                                    image_path: None,
                                                },
                                            ),
                                            pages_id: vec![curr_el.page_id],
                                            bbox: figure_bbox,
                                        }
                                    } else {
                                        // Regular Image block without embedded text
                                        Block {
                                            id: block_id,
                                            kind: crate::blocks::BlockType::Image(ImageBlock {
                                                id: image_id,
                                                caption: None,
                                                image_path: None,
                                            }),
                                            pages_id: vec![curr_el.page_id],
                                            bbox: curr_el.bbox,
                                        }
                                    };
                                    image_id += 1;
                                    block_id += 1;
                                    blocks.push(block);
                                }
                            }
                            crate::entities::ElementType::FootNote
                            | crate::entities::ElementType::Caption => {
                                // TODO: check if there is a case where there is multiple caption associated with the same image
                                let next_el = element_it.next().unwrap();
                                curr_el.bbox.merge(&next_el.bbox);
                                figure_bbox.merge(&next_el.bbox);

                                // FIXED: Apply script detection to Image caption (Image→Caption case)
                                let caption_text = if !next_el.line_spans.is_empty() {
                                    debug_print!(
                                        "🖼️ IMAGE CAPTION: Processing caption with {} line_spans",
                                        next_el.line_spans.len()
                                    );
                                    let original_text = next_el
                                        .line_spans
                                        .iter()
                                        .map(|line_spans| {
                                            concatenate_spans_with_spacing(line_spans)
                                        })
                                        .collect::<Vec<String>>()
                                        .join(" ");
                                    crate::modtext::process_text_with_spans(
                                        &original_text,
                                        &next_el.line_spans,
                                    )
                                } else {
                                    apply_corrections_to_text(next_el.text_block.text.clone())
                                };

                                // Check if we found embedded text to create a Figure block
                                let block = if !figure_elements.is_empty() {
                                    // Remove the blocks that we're incorporating into the figure (in reverse order)
                                    for &i in blocks_to_remove.iter().rev() {
                                        if i < blocks.len() {
                                            blocks.remove(i);
                                        } else {
                                            debug_print!("🖼️ WARNING: Index {} out of bounds for blocks.len()={}", i, blocks.len());
                                        }
                                    }

                                    debug_print!("🖼️ Created Figure block with {} embedded texts and caption", figure_elements.len());
                                    // Create Figure block with embedded texts and caption
                                    Block {
                                        id: block_id,
                                        kind: crate::blocks::BlockType::Figure(
                                            crate::blocks::FigureBlock {
                                                id: image_id,
                                                embedded_texts: figure_elements,
                                                image_bbox: Some(curr_el.bbox.clone()),
                                                caption: Some(caption_text),
                                                image_path: None,
                                            },
                                        ),
                                        pages_id: vec![curr_el.page_id],
                                        bbox: figure_bbox,
                                    }
                                } else {
                                    // Regular Image block with caption
                                    Block {
                                        id: block_id,
                                        kind: crate::blocks::BlockType::Image(ImageBlock {
                                            id: image_id,
                                            caption: Some(caption_text),
                                            image_path: None,
                                        }),
                                        pages_id: vec![curr_el.page_id],
                                        bbox: curr_el.bbox,
                                    }
                                };

                                image_id += 1;
                                block_id += 1;
                                blocks.push(block);
                            }
                            _ => {
                                // Check if we found embedded text to create a Figure block
                                let block = if !figure_elements.is_empty() {
                                    // Remove the blocks that we're incorporating into the figure (in reverse order)
                                    for &i in blocks_to_remove.iter().rev() {
                                        if i < blocks.len() {
                                            blocks.remove(i);
                                        } else {
                                            debug_print!("🖼️ WARNING: Index {} out of bounds for blocks.len()={}", i, blocks.len());
                                        }
                                    }

                                    let embedded_count = figure_elements.len();
                                    debug_print!(
                                        "🖼️ Created Figure block with {} embedded texts",
                                        embedded_count
                                    );
                                    // Create Figure block with embedded texts
                                    Block {
                                        id: block_id,
                                        kind: crate::blocks::BlockType::Figure(
                                            crate::blocks::FigureBlock {
                                                id: image_id,
                                                embedded_texts: figure_elements,
                                                image_bbox: Some(curr_el.bbox.clone()),
                                                caption: None,
                                                image_path: None,
                                            },
                                        ),
                                        pages_id: vec![curr_el.page_id],
                                        bbox: figure_bbox,
                                    }
                                } else {
                                    // Regular Image block
                                    Block {
                                        id: block_id,
                                        kind: crate::blocks::BlockType::Image(ImageBlock {
                                            id: image_id,
                                            caption: None,
                                            image_path: None,
                                        }),
                                        pages_id: vec![curr_el.page_id],
                                        bbox: curr_el.bbox,
                                    }
                                };

                                image_id += 1;
                                block_id += 1;
                                blocks.push(block);
                            }
                        }
                    }
                }
            }
            ElementType::Header => {
                // Get truly original text by reconstructing from raw CharSpans (before any HTML tag processing)
                let original_text = if !curr_el.line_spans.is_empty() {
                    // Reconstruct original text from CharSpans
                    curr_el
                        .line_spans
                        .iter()
                        .map(|line_spans| {
                            // Space-aware concatenation of spans within a line
                            concatenate_spans_with_spacing(line_spans)
                        })
                        .collect::<Vec<String>>()
                        .join(" ")
                } else {
                    // Fallback to current text if no spans available
                    curr_el.text_block.text.clone()
                };

                let processed_text = if !curr_el.line_spans.is_empty() {
                    debug_print!(
                        "📰 HEADER WITH SPANS: Processing header with {} line_spans for HTML tag detection",
                        curr_el.line_spans.len()
                    );
                    // Apply HTML tag detection using CharSpans but WITHOUT formula wrapper (for HEADER elements)
                    crate::modtext::process_text_with_spans(&original_text, &curr_el.line_spans)
                } else {
                    // No spans available, use basic text correction only
                    apply_corrections_to_text(original_text.clone())
                };

                let mut header_block = Block {
                    id: block_id,
                    kind: BlockType::Header(TextBlock {
                        text: processed_text.clone(),
                        // Always set fertext for char_span alignment
                        fertext: Some(original_text),
                        char_spans: Vec::new(),
                        sentence_ends: Vec::new(),
                    }),
                    pages_id: vec![curr_el.page_id],
                    bbox: curr_el.bbox,
                };

                while let Some(next_el) = element_it.peek() {
                    if matches!(next_el.kind, crate::entities::ElementType::Header) {
                        let next_el = element_it.next().unwrap();
                        header_block.merge(next_el)?;
                    } else {
                        break;
                    }
                }
                block_id += 1;
                blocks.push(header_block);
            }
            ElementType::Footer => {
                // Get truly original text by reconstructing from raw CharSpans (before any HTML tag processing)
                let original_text = if !curr_el.line_spans.is_empty() {
                    // Reconstruct original text from CharSpans
                    curr_el
                        .line_spans
                        .iter()
                        .map(|line_spans| {
                            // Space-aware concatenation of spans within a line
                            concatenate_spans_with_spacing(line_spans)
                        })
                        .collect::<Vec<String>>()
                        .join(" ")
                } else {
                    // Fallback to current text if no spans available
                    curr_el.text_block.text.clone()
                };

                let processed_text =
                    crate::modtext::process_text_with_spans(&original_text, &curr_el.line_spans);

                let mut footer_block = Block {
                    id: block_id,
                    kind: BlockType::Footer(TextBlock {
                        text: processed_text.clone(),
                        // Always set fertext for char_span alignment
                        fertext: Some(original_text),
                        char_spans: Vec::new(),
                        sentence_ends: Vec::new(),
                    }),
                    pages_id: vec![curr_el.page_id],
                    bbox: curr_el.bbox,
                };

                while let Some(next_el) = element_it.peek() {
                    if matches!(next_el.kind, ElementType::Footer) {
                        let next_el = element_it.next().unwrap();
                        footer_block.merge(next_el)?;
                    } else {
                        break;
                    }
                }
                block_id += 1;
                blocks.push(footer_block);
            }
            ElementType::Title | ElementType::Subtitle => {
                let lvl = title_level
                    .get(&(curr_el.page_id, curr_el.id))
                    .unwrap_or(&0u8);

                // Get truly original text by reconstructing from raw CharSpans (before any HTML tag processing)
                let original_text = if !curr_el.line_spans.is_empty() {
                    // Reconstruct original text from CharSpans
                    curr_el
                        .line_spans
                        .iter()
                        .map(|line_spans| {
                            // Space-aware concatenation of spans within a line
                            concatenate_spans_with_spacing(line_spans)
                        })
                        .collect::<Vec<String>>()
                        .join(" ")
                } else {
                    // Fallback to current text if no spans available
                    curr_el.text_block.text.clone()
                };

                let processed_text = if !curr_el.line_spans.is_empty() {
                    debug_print!(
                        "📜 TITLE WITH SPANS: Processing title with {} line_spans for HTML tag detection",
                        curr_el.line_spans.len()
                    );
                    // Apply HTML tag detection using CharSpans but WITHOUT formula wrapper (for TITLE elements)
                    crate::modtext::process_text_with_spans(&original_text, &curr_el.line_spans)
                } else {
                    // No spans available, use basic text correction only
                    apply_corrections_to_text(original_text.clone())
                };

                let title = Block {
                    id: block_id,
                    kind: BlockType::Title(Title {
                        level: *lvl,
                        text: processed_text.clone(),
                        // Always set fertext for char_span alignment
                        fertext: Some(original_text),
                        char_spans: curr_el.get_serializable_char_spans(),
                        sentence_ends: Vec::new(),
                    }),
                    pages_id: vec![curr_el.page_id],
                    bbox: curr_el.bbox,
                };
                block_id += 1;
                blocks.push(title);
            }
            _ => {
                continue;
            }
        }
    }

    // Post-process to create Figure blocks from Image blocks with nearby embedded text
    post_process_figure_blocks(&mut blocks);

    // Remove TextBlocks that have content matching embedded texts in Figure blocks
    let mut text_blocks_to_remove = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if let crate::blocks::BlockType::TextBlock(text_block) = &block.kind {
            // Check if this text matches any embedded text in Figure blocks
            for other_block in blocks.iter() {
                if let crate::blocks::BlockType::Figure(figure) = &other_block.kind {
                    for embedded_text in &figure.embedded_texts {
                        // Normalize both texts for comparison
                        let text_normalized =
                            text_block.text.trim().replace(&['\r', '\n', '\t'][..], " ");
                        let text_normalized = text_normalized
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        let embedded_normalized =
                            embedded_text.trim().replace(&['\r', '\n', '\t'][..], " ");
                        let embedded_normalized = embedded_normalized
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");

                        if text_normalized == embedded_normalized {
                            text_blocks_to_remove.push(i);
                            break;
                        }
                    }
                    if text_blocks_to_remove.contains(&i) {
                        break;
                    }
                }
            }
        }
    }

    // Remove duplicate TextBlocks in reverse order
    for &index in text_blocks_to_remove.iter().rev() {
        blocks.remove(index);
    }

    // Apply text corrections to all blocks after assembly is complete
    debug_print!(
        "🔧 MERGE OUTPUT: About to apply corrections to {} blocks",
        blocks.len()
    );
    crate::font_analysis::correct_blocks(&mut blocks);
    debug_print!("🔧 MERGE OUTPUT: After corrections applied");

    // WORKAROUND: Reclassify TextBlocks as Footers based on position and content
    // This must be done AFTER all text merging is complete
    for block in blocks.iter_mut() {
        if let BlockType::TextBlock(text_block) = &block.kind {
            // Get page height for this block
            if let Some(page_id) = block.pages_id.first() {
                if let Some(&page_height) = page_heights.get(page_id) {
                    let distance_from_bottom = page_height - block.bbox.y1;

                    if distance_from_bottom <= 120.0 {
                        let text = &text_block.text;
                        if FOOTER_PATTERN_REGEX.is_match(text.trim()) {
                            debug_print!(
                                "🔧 Post-merge: Reclassifying block {} as Footer: '{}'",
                                block.id,
                                text.chars().take(60).collect::<String>()
                            );
                            block.kind = BlockType::Footer(TextBlock {
                                text: text_block.text.clone(),
                                fertext: text_block.fertext.clone(),
                                char_spans: text_block.char_spans.clone(),
                                sentence_ends: text_block.sentence_ends.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(blocks)
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::entities::BBox;
    use crate::entities::ElementText;

    fn create_text_element(id: usize, page_id: usize, text: &str, bbox: BBox) -> Element {
        Element {
            id,
            layout_block_id: 0,
            kind: ElementType::Text,
            text_block: ElementText {
                text: text.to_owned(),
            },
            page_id,
            bbox,
            line_spans: Vec::new(),
        }
    }

    fn create_list_element(id: usize, page_id: usize, text: &str, bbox: BBox) -> Element {
        Element {
            id,
            layout_block_id: 0,
            kind: ElementType::ListItem,
            text_block: ElementText {
                text: text.to_string(),
            },
            page_id,
            bbox,
            line_spans: Vec::new(),
        }
    }

    fn create_caption_element(id: usize, page_id: usize, text: &str, bbox: BBox) -> Element {
        Element {
            id,
            layout_block_id: 0,
            kind: ElementType::Caption,
            text_block: ElementText {
                text: text.to_string(),
            },
            page_id,
            bbox,
            line_spans: Vec::new(),
        }
    }

    fn create_footnote_element(id: usize, page_id: usize, text: &str, bbox: BBox) -> Element {
        Element {
            id,
            layout_block_id: 0,
            kind: ElementType::FootNote,
            text_block: ElementText {
                text: text.to_string(),
            },
            page_id,
            bbox,
            line_spans: Vec::new(),
        }
    }
    fn create_image_element(id: usize, page_id: usize, bbox: BBox) -> Element {
        Element {
            id,
            layout_block_id: 0,
            kind: ElementType::Image,
            text_block: ElementText::default(),
            page_id,
            bbox,
            line_spans: Vec::new(),
        }
    }

    #[test]
    fn test_merge_adjacent_text_blocks() -> anyhow::Result<()> {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 0.0,
            y0: 2.1,
            x1: 2.0,
            y1: 4.1,
        };

        let elements = vec![
            create_text_element(0, 1, "First paragraph", bbox1),
            create_text_element(1, 1, "Second paragraph", bbox2),
        ];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 1);
        if let BlockType::TextBlock(text) = &blocks[0].kind {
            assert!(text.text.contains("First paragraph"));
            assert!(text.text.contains("Second paragraph"));
        } else {
            panic!("Expected TextBlock");
        }
        Ok(())
    }

    #[test]
    fn test_merge_list_items() -> anyhow::Result<()> {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 0.0,
            y0: 2.1,
            x1: 2.0,
            y1: 4.1,
        };

        let elements = vec![
            create_list_element(0, 1, "First item", bbox1),
            create_list_element(1, 1, "Second item", bbox2.clone()),
            create_text_element(2, 1, "Random text", bbox2),
        ];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 2);
        if let BlockType::ListBlock(list) = &blocks[0].kind {
            assert_eq!(list.items.len(), 2);
            assert_eq!(list.items[0].text, "First item");
            assert_eq!(list.items[1].text, "Second item");
        } else {
            panic!("Expected ListItem");
        }
        Ok(())
    }

    #[test]
    fn test_merge_caption_with_image() -> anyhow::Result<()> {
        let caption_bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let image_bbox = BBox {
            x0: 0.0,
            y0: 2.1,
            x1: 2.0,
            y1: 4.1,
        };

        let elements = vec![
            create_caption_element(0, 1, "Image caption", caption_bbox),
            create_image_element(1, 1, image_bbox),
        ];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 1);
        if let BlockType::Image(image) = &blocks[0].kind {
            assert_eq!(image.caption, Some("Image caption".to_string()));
        } else {
            panic!("Expected Image");
        }
        Ok(())
    }

    #[test]
    fn test_merge_orphan_caption_becomes_text() -> anyhow::Result<()> {
        let caption_bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };

        let elements = vec![create_caption_element(0, 1, "Orphan caption", caption_bbox)];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 1);
        if let BlockType::TextBlock(text) = &blocks[0].kind {
            assert_eq!(text.text, "Orphan caption");
        } else {
            panic!("Expected TextBlock");
        }
        Ok(())
    }

    #[test]
    fn test_merge_distant_text_blocks_not_merged() -> anyhow::Result<()> {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 0.0,
            y0: 20.0, // Far away
            x1: 2.0,
            y1: 22.0,
        };

        let elements = vec![
            create_text_element(0, 1, "First paragraph", bbox1),
            create_text_element(1, 1, "Distant paragraph", bbox2),
        ];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 2);
        Ok(())
    }

    #[test]
    fn test_merge_image_last_element() -> anyhow::Result<()> {
        let image_bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };

        let elements = vec![create_image_element(0, 1, image_bbox)];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 1);
        if let BlockType::Image(image) = &blocks[0].kind {
            assert_eq!(image.caption, None);
        } else {
            panic!("Expected Image block");
        }
        Ok(())
    }

    #[test]
    fn test_merge_image_with_following_caption() -> anyhow::Result<()> {
        let image_bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let caption_bbox = BBox {
            x0: 0.0,
            y0: 2.1,
            x1: 2.0,
            y1: 4.1,
        };

        let elements = vec![
            create_image_element(0, 1, image_bbox),
            create_caption_element(1, 1, "Image Description", caption_bbox),
        ];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 1);
        if let BlockType::Image(image) = &blocks[0].kind {
            assert_eq!(image.caption, Some("Image Description".to_string()));
        } else {
            panic!("Expected Image block with caption");
        }
        Ok(())
    }

    #[test]
    fn test_merge_image_with_following_non_caption() -> anyhow::Result<()> {
        let image_bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let text_bbox = BBox {
            x0: 0.0,
            y0: 2.1,
            x1: 2.0,
            y1: 4.1,
        };

        let elements = vec![
            create_image_element(0, 1, image_bbox),
            create_text_element(1, 1, "Regular text", text_bbox),
        ];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 2);
        if let BlockType::Image(image) = &blocks[0].kind {
            assert_eq!(image.caption, None);
        } else {
            panic!("Expected Image block without caption");
        }

        if let BlockType::TextBlock(text) = &blocks[1].kind {
            assert_eq!(text.text, "Regular text");
        } else {
            panic!("Expected Text block");
        }
        Ok(())
    }

    #[test]
    fn test_merge_image_with_footnote() -> anyhow::Result<()> {
        let image_bbox = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let footnote_bbox = BBox {
            x0: 0.0,
            y0: 2.1,
            x1: 2.0,
            y1: 4.1,
        };

        let elements = vec![
            create_image_element(0, 1, image_bbox),
            create_footnote_element(1, 1, "Image Footnote", footnote_bbox),
        ];

        let blocks = merge_elements_into_blocks(elements, HashMap::new(), HashMap::new())?;

        assert_eq!(blocks.len(), 1);
        if let BlockType::Image(image) = &blocks[0].kind {
            assert_eq!(image.caption, Some("Image Footnote".to_string()));
        } else {
            panic!("Expected Image block with footnote as caption");
        }
        Ok(())
    }
}
