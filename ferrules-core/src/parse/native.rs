use std::{ops::Range, sync::Arc, time::Instant};

use anyhow::Context;
use image::DynamicImage;
use pdfium_render::prelude::{
    PdfDocumentMetadataTagType, PdfPage, PdfPageTextChar, PdfRenderConfig, Pdfium,
};
use tracing::{instrument, Span};

use crate::{
    debug_print,
    entities::{BBox, CharSpan, Line, PageID},
    layout::model::ORTLayoutParser,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

const MAX_CONCURRENT_NATIVE_REQS: usize = 10;

/// Threshold for gap ratio below which a zero-width generated space is considered
/// a false word boundary from TJ kerning rather than a real space.
/// False kerning gaps are ~10% of font_size; real word gaps are ~17%+.
const TJ_KERNING_GAP_THRESHOLD: f32 = 0.15;

/// Determines whether a generated whitespace character should be skipped because
/// it represents a false word boundary inserted by pdfium's TJ kerning interpretation.
///
/// Returns `true` if the space should be skipped (false word boundary).
///
/// Three conditions must ALL be true:
/// 1. The space character has zero width (false kerning spaces from TJ operators)
/// 2. The next character is visible (non-whitespace) — gap is unreliable otherwise
/// 3. The gap between previous span end and next char start is non-negative and small
fn should_skip_generated_space(
    space_width: f32,
    next_char_is_whitespace: bool,
    gap: f32,
    font_size: f32,
) -> bool {
    // Only filter when next character is visible; when next is whitespace
    // (e.g. newline), the gap measurement is unreliable.
    if next_char_is_whitespace {
        return false;
    }

    // Real word-boundary spaces have width proportional to the font.
    // False kerning spaces from TJ operators have zero width.
    if space_width >= f32::EPSILON {
        return false;
    }

    // Only skip when gap is non-negative (negative = line wrap) and small
    // relative to font size (kerning artifact, not a real word gap).
    gap >= 0.0 && gap < font_size * TJ_KERNING_GAP_THRESHOLD
}

pub(crate) fn parse_text_spans<'a>(
    chars: impl Iterator<Item = PdfPageTextChar<'a>>,
    page_bbox: &BBox,
) -> Vec<CharSpan> {
    let mut spans: Vec<CharSpan> = Vec::new();
    let mut char_iter = chars.peekable();

    while let Some(char) = char_iter.next() {
        // Skip generated whitespace that represents false word boundaries from TJ kerning.
        // Pdfium marks ALL spaces as generated, so we distinguish kerning artifacts from
        // real word boundaries using space width and inter-character gap.
        if char.is_generated().unwrap_or(false)
            && char.unicode_char().unwrap_or_default().is_whitespace()
        {
            if let (Some(next_char), Some(prev_span)) = (char_iter.peek(), spans.last()) {
                let space_width = char
                    .loose_bounds()
                    .map(|b| b.right().value - b.left().value)
                    .unwrap_or(0.0);
                let next_char_is_whitespace =
                    next_char.unicode_char().unwrap_or_default().is_whitespace();
                let next_x0 = next_char
                    .loose_bounds()
                    .map(|b| b.left().value)
                    .unwrap_or(prev_span.bbox.x1);
                let gap = next_x0 - prev_span.bbox.x1;

                if should_skip_generated_space(
                    space_width,
                    next_char_is_whitespace,
                    gap,
                    prev_span.font_size,
                ) {
                    continue;
                }
            }
            // Real word boundaries or edge cases fall through to normal processing
        }

        let mut char_text = char.unicode_char().unwrap_or_default().to_string();
        let char_code = char.unicode_value();

        // Convert soft hyphen (U+00AD) to regular hyphen so it can be detected and removed later
        if char_code == 0x00AD {
            char_text = "-".to_string();
        }

        // U+0002 (STX - Start of Text) is used by some PDFs to mark hyphenated line breaks
        // When we see U+0002, it means the word continues on the next line without a hyphen
        // Convert U+0002 to a regular hyphen so downstream hyphen removal logic will join the words
        // Example: "eva" + U+0002 + "sion" becomes "eva-sion" which gets cleaned to "evasion"
        if char_code == 0x0002 {
            char_text = "-".to_string();
        }

        // Check for line-ending hyphen pattern: only regular hyphens should be removed
        // Em-dashes (—) and en-dashes (–) are legitimate punctuation and should be preserved
        if char_text == "-" || char_text == "‐" {
            // Only regular hyphens and soft hyphens
            debug_print!(
                "🔍 HYPHEN CHECK: Found '{}' (U+{:04X})",
                char_text,
                char.unicode_value()
            );

            if let Some(next_char) = char_iter.peek() {
                let next_char_text = next_char.unicode_char().unwrap_or_default().to_string();
                let next_char_code = next_char.unicode_value();

                debug_print!(
                    "🔍 HYPHEN CHECK: Found '{}' (U+{:04X}), next char '{}' (U+{:04X})",
                    char_text,
                    char.unicode_value(),
                    next_char_text,
                    next_char_code
                );

                // Check if next character is a line break or whitespace that suggests line continuation
                // This includes: newlines, carriage returns, form feeds, and other whitespace
                let is_line_break = next_char_text == "\n" || next_char_text == "\r" ||
                                  next_char_text == "\r\n" || next_char_code == 0x0C || // Form feed
                                  next_char_code == 0x0B || // Vertical tab
                                  (next_char_text.chars().next().map(|c| c.is_whitespace()).unwrap_or(false) &&
                                   next_char_text != " " && next_char_text != "\t");

                // Also check for the case where hyphen is directly followed by word characters
                // which might indicate the hyphen was incorrectly inserted instead of removed
                let next_is_word_char = next_char_text
                    .chars()
                    .next()
                    .map(|c| c.is_alphabetic())
                    .unwrap_or(false);

                if is_line_break {
                    debug_print!("🔍 HYPHEN SKIP: Found hyphen '{}' followed by line break '{}', skipping both",
                        char_text, next_char_text.chars().next().map(|c| format!("U+{:04X}", c as u32)).unwrap_or_default());

                    // Skip the line break character
                    char_iter.next();

                    // Continue to the next character without adding the hyphen or line break
                    continue;
                } else if next_is_word_char && !spans.is_empty() {
                    // Check if current span ends with a word character - this could be a broken hyphenation
                    // BUT preserve compound words like "state-of-the-art"
                    let current_span = &spans[spans.len() - 1];
                    let last_char_in_span = current_span.text.chars().last();
                    if let Some(last_char) = last_char_in_span {
                        if last_char.is_alphabetic() {
                            // Simple position-based approach: DON'T remove hyphens in mid-text
                            // Only the logic above (lines 48-66) handles '-\n' cases
                            // All other hyphens (including compound words) are preserved
                            debug_print!(
                                "🔍 HYPHEN KEEP: Preserving hyphen in compound word '{}'-'{}'",
                                current_span.text.trim(),
                                next_char_text
                            );
                        }
                    }
                }
            }
        }

        if spans.is_empty() {
            let span = CharSpan::new_from_char(&char, page_bbox);
            debug_print!("🔍 NEW SPAN[0]: '{}' at y={:.1}", span.text, span.bbox.y0);
            spans.push(span);
        } else {
            let current_y = spans.last().unwrap().bbox.y0;
            let can_append = {
                let span = spans.last_mut().unwrap();
                span.append(&char, page_bbox).is_some()
            };

            if can_append {
                let span = spans.last().unwrap();
                let new_y = span.bbox.y0;
                if (current_y - new_y).abs() > 5.0 {
                    debug_print!(
                        "🔍 CROSS-LINE SUCCESS: Added '{}' to span across lines, span now: '{}'",
                        char_text,
                        span.text.chars().take(20).collect::<String>()
                    );
                }
            } else {
                let prev_span = spans.last().unwrap();
                let prev_span_text = prev_span.text.chars().take(10).collect::<String>();

                // Check for potential Vi/jil patterns
                if (prev_span.text.contains("Vi") && char_text == "j")
                    || (prev_span.text.ends_with("-")
                        && char_text
                            .chars()
                            .next()
                            .map(|c| c.is_alphabetic())
                            .unwrap_or(false))
                {
                    debug_print!("🔍 POTENTIAL BREAK: Previous span '{}' next char '{}' - could be hyphen break",
                        prev_span_text, char_text);
                }

                let new_span = CharSpan::new_from_char(&char, page_bbox);
                debug_print!(
                    "🔍 NEW SPAN[{}]: '{}' at y={:.1} (prev was y={:.1}) - diff={:.1}pt",
                    spans.len(),
                    new_span.text,
                    new_span.bbox.y0,
                    current_y,
                    (current_y - new_span.bbox.y0).abs()
                );

                spans.push(new_span);
            }
        }
    }

    // Debug: Look for Vi-/Vifijil patterns in spans after creation
    debug_print!("🔍 SPAN ANALYSIS: Created {} spans total", spans.len());
    for (i, span) in spans.iter().enumerate() {
        if span.text.contains("Vi") {
            debug_print!("🔍 SPAN DEBUG[{}]: Found 'Vi' span: '{}'", i, span.text);
        }
        if span.text.contains("jil") {
            debug_print!("🔍 SPAN DEBUG[{}]: Found 'jil' span: '{}'", i, span.text);
        }
        if span.text.contains("Vifijil") {
            debug_print!(
                "🔍 SPAN DEBUG[{}]: Found 'Vifijil' span: '{}'",
                i,
                span.text
            );
        }
        if span.text.contains("Prompt Injection") {
            debug_print!(
                "🔍 SPAN DEBUG[{}]: Found 'Prompt Injection' span: '{}'",
                i,
                span.text
            );
        }
    }

    spans
}

pub(crate) fn parse_text_lines(spans: Vec<CharSpan>) -> Vec<Line> {
    let mut lines = Vec::new();
    for span in spans {
        if lines.is_empty() {
            let line = Line::new_from_span(span);
            lines.push(line);
        } else {
            let line = lines.last_mut().unwrap();
            if let Err(span) = line.append(span) {
                let line = Line::new_from_span(span);
                lines.push(line)
            }
        }
    }

    // Finalize all lines to apply comprehensive text processing
    for line in &mut lines {
        line.finalize();
    }

    lines
}

pub struct ParseNativeRequest {
    pub doc_data: std::sync::Arc<[u8]>,
    pub password: Option<String>,
    pub flatten: bool,
    pub page_range: Option<Range<usize>>,
    pub required_raster_width: u32,
    pub required_raster_height: u32,
    pub sender_tx: Sender<anyhow::Result<ParseNativePageResult>>,
    pub count_only: bool,
    pub debug_context: Option<crate::debug::DebugContext>,
}
impl ParseNativeRequest {
    pub fn new(
        data: &[u8],
        password: Option<&str>,
        flatten: bool,
        page_range: Option<Range<usize>>,
        sender_tx: Sender<anyhow::Result<ParseNativePageResult>>,
        debug_context: Option<crate::debug::DebugContext>,
    ) -> Self {
        ParseNativeRequest {
            doc_data: Arc::from(data),
            password: password.map(|p| p.to_string()),
            flatten,
            page_range,
            // TODO: should be global?
            required_raster_width: ORTLayoutParser::REQUIRED_WIDTH,
            required_raster_height: ORTLayoutParser::REQUIRED_HEIGHT,
            sender_tx,
            count_only: false,
            debug_context,
        }
    }

    pub fn new_count_only(
        data: &[u8],
        password: Option<&str>,
        sender_tx: Sender<anyhow::Result<ParseNativePageResult>>,
        debug_context: Option<crate::debug::DebugContext>,
    ) -> Self {
        ParseNativeRequest {
            doc_data: Arc::from(data),
            password: password.map(|p| p.to_string()),
            flatten: false,            // Not needed for counting
            page_range: None,          // Count all pages
            required_raster_width: 0,  // Not needed for counting
            required_raster_height: 0, // Not needed for counting
            sender_tx,
            count_only: true,
            debug_context,
        }
    }
}

#[derive(Debug)]
pub struct ParseNativeMetadata {
    pub parse_native_duration_ms: u128,
}

#[derive(Debug)]
pub struct ParseNativePageResult {
    // TODO: page_native_rotation
    pub page_id: PageID,
    pub text_lines: Vec<Line>,
    pub page_bbox: BBox,
    pub page_image: Arc<DynamicImage>,
    pub page_image_scale1: DynamicImage,
    pub downscale_factor: f32,
    pub metadata: ParseNativeMetadata,
    pub is_count_result: bool,
    pub total_page_count: Option<usize>,
    pub pdf_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParseNativeQueue {
    queue: Sender<(ParseNativeRequest, Span)>,
}

impl Default for ParseNativeQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ParseNativeQueue {
    pub fn new() -> Self {
        let (queue_sender, queue_receiver) = mpsc::channel(MAX_CONCURRENT_NATIVE_REQS);

        tokio::task::spawn_blocking(move || start_native_parser(queue_receiver));
        Self {
            queue: queue_sender,
        }
    }

    pub(crate) async fn push(&self, req: ParseNativeRequest) -> anyhow::Result<()> {
        let span = Span::current();
        self.queue
            .send((req, span))
            .await
            .context("error sending parse native request")
    }
}

#[instrument(skip(page))]
pub(crate) fn parse_page_native(
    page_id: PageID,
    page: &mut PdfPage,
    flatten_page: bool,
    required_raster_width: u32,
    required_raster_height: u32,
) -> anyhow::Result<ParseNativePageResult> {
    let start_time = Instant::now();
    if flatten_page {
        page.flatten()?;
    }
    let rescale_factor = {
        let scale_w = required_raster_width as f32 / page.width().value;
        let scale_h = required_raster_height as f32 / page.height().value;
        f32::min(scale_h, scale_w)
    };
    let downscale_factor = 1f32 / rescale_factor;

    let page_bbox = BBox {
        x0: 0f32,
        y0: 0f32,
        x1: page.width().value,
        y1: page.height().value,
    };
    let page_image = page
        .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(rescale_factor))
        .map(|bitmap| bitmap.as_image())?;

    let page_image_scale1 = page
        .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1f32))
        .map(|bitmap| bitmap.as_image())?;

    let text_spans = parse_text_spans(page.text()?.chars().iter(), &page_bbox);

    let text_lines = parse_text_lines(text_spans);

    let parse_native_duration_ms = start_time.elapsed().as_millis();
    tracing::debug!(
        "Parsing page {} using pdfium took {}ms",
        page_id,
        parse_native_duration_ms
    );
    Ok(ParseNativePageResult {
        page_id,
        text_lines,
        page_bbox,
        page_image: Arc::new(page_image),
        page_image_scale1,
        downscale_factor,
        metadata: ParseNativeMetadata {
            parse_native_duration_ms,
        },
        is_count_result: false,
        total_page_count: None,
        pdf_title: None,
    })
}

fn handle_parse_native_req(
    pdfium: &Pdfium,
    req: ParseNativeRequest,
    parent_span: Span,
) -> anyhow::Result<()> {
    // Reinter span
    let _guard = parent_span.enter();
    let ParseNativeRequest {
        doc_data,
        password,
        flatten,
        page_range,
        required_raster_width,
        required_raster_height,
        sender_tx,
        count_only,
        debug_context,
    } = req;

    // Set debug context for this thread if provided
    if let Some(context) = debug_context {
        crate::debug::set_debug_context(context.doc_name, Some(context.output_flags));
    }

    // Set document context for font corruption analysis
    #[cfg(feature = "correction-engine")]
    crate::font_analysis::set_document_context();

    // Use original PDF data directly - corrections are applied at character level during text extraction
    let processed_pdf_data = doc_data.to_vec();
    debug_print!(
        "🔧 DEBUG: Using original PDF without preprocessing - corrections applied at text level"
    );

    let mut document = pdfium.load_pdf_from_byte_slice(&processed_pdf_data, password.as_deref())?;

    // Extract PDF title from document metadata before taking mutable borrow for pages
    let pdf_title = document
        .metadata()
        .get(PdfDocumentMetadataTagType::Title)
        .map(|tag| tag.value().to_string())
        .filter(|s| !s.trim().is_empty());

    let mut pages: Vec<_> = document.pages_mut().iter().enumerate().collect();

    // If only counting pages, send the count and return early
    if count_only {
        let total_pages = pages.len();

        use image::{DynamicImage, ImageBuffer};
        let dummy_image = DynamicImage::ImageRgb8(ImageBuffer::new(1, 1));

        let count_result = ParseNativePageResult {
            page_id: 0,
            text_lines: Vec::new(),
            page_bbox: crate::entities::BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
            page_image: Arc::new(dummy_image.clone()),
            page_image_scale1: dummy_image,
            downscale_factor: 1.0,
            metadata: ParseNativeMetadata {
                parse_native_duration_ms: 0,
            },
            is_count_result: true,
            total_page_count: Some(total_pages),
            pdf_title,
        };
        sender_tx.blocking_send(Ok(count_result))?;
        return Ok(());
    }

    let pages = if let Some(range) = page_range {
        if range.end > pages.len() {
            anyhow::bail!(
                "Page range end ({}) exceeds document length ({})",
                range.end,
                pages.len()
            )
        }
        pages.drain(range).collect()
    } else {
        pages
    };
    tracing::debug!("Starting to process pages");
    for (page_id, mut page) in pages {
        let parsing_result = parse_page_native(
            page_id,
            &mut page,
            flatten,
            required_raster_width,
            required_raster_height,
        );
        sender_tx.blocking_send(parsing_result)?
    }
    tracing::debug!("Finished processing pages");

    // Clear document context after parsing is complete
    #[cfg(feature = "correction-engine")]
    crate::font_analysis::clear_document_context();

    Ok(())
}

pub fn start_native_parser(mut input_rx: Receiver<(ParseNativeRequest, Span)>) {
    let pdfium = Pdfium::new(
        Pdfium::bind_to_statically_linked_library().expect("can't load pdfiurm bindings"),
    );
    while let Some((req, parent_span)) = input_rx.blocking_recv() {
        match handle_parse_native_req(&pdfium, req, parent_span) {
            Ok(_) => {}
            Err(e) => debug_print!("error parsing request natively : {e:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Font sizes from real PDFs used in integration tests
    const FONT_10_91: f32 = 10.91; // test5 (mem0.pdf) body text
    const FONT_8_97: f32 = 8.97; // test2 (cag2025.pdf) body text

    // --- False kerning: should be skipped ---

    #[test]
    fn test_skip_false_kerning_zero_width_small_gap() {
        // "Al ice" in test5: zero-width space, gap ratio ~0.10
        assert!(should_skip_generated_space(
            0.0,               // zero-width space
            false,             // next char is visible
            FONT_10_91 * 0.10, // gap = 10% of font_size
            FONT_10_91,
        ));
    }

    #[test]
    fn test_skip_false_kerning_exact_zero_gap() {
        // Characters touching with zero gap
        assert!(should_skip_generated_space(0.0, false, 0.0, FONT_10_91));
    }

    #[test]
    fn test_skip_false_kerning_tiny_gap() {
        // Very small gap (ratio ~0.01)
        assert!(should_skip_generated_space(0.0, false, 0.13, FONT_10_91,));
    }

    #[test]
    fn test_skip_false_kerning_at_threshold_boundary() {
        // Gap just below threshold (ratio = 0.149...)
        let gap = FONT_10_91 * 0.15 - 0.01;
        assert!(should_skip_generated_space(0.0, false, gap, FONT_10_91));
    }

    // --- Real word boundaries: should NOT be skipped ---

    #[test]
    fn test_keep_real_space_nonzero_width() {
        // Real word-boundary space with width ~50% of font_size
        assert!(!should_skip_generated_space(
            5.46,              // space width ≈ 50% of font_size
            false,             // next char is visible
            FONT_10_91 * 0.10, // even with small gap
            FONT_10_91,
        ));
    }

    #[test]
    fn test_keep_real_space_large_gap() {
        // "fetch passages" in test2: zero-width space but large gap (ratio ~0.17)
        assert!(!should_skip_generated_space(
            0.0,
            false,
            FONT_8_97 * 0.172, // gap ratio 0.172, above threshold
            FONT_8_97,
        ));
    }

    #[test]
    fn test_keep_space_at_threshold() {
        // Gap exactly at threshold should NOT be skipped (strict less-than)
        let gap = FONT_10_91 * TJ_KERNING_GAP_THRESHOLD;
        assert!(!should_skip_generated_space(0.0, false, gap, FONT_10_91));
    }

    #[test]
    fn test_keep_space_next_is_whitespace() {
        // Space followed by newline — gap measurement unreliable
        assert!(!should_skip_generated_space(
            0.0, true, // next char IS whitespace
            0.0,  // zero gap (would otherwise be skipped)
            FONT_10_91,
        ));
    }

    #[test]
    fn test_keep_space_negative_gap() {
        // Negative gap indicates line wrap / column jump — always keep
        assert!(!should_skip_generated_space(
            0.0, false, -154.99, // large negative gap from line wrap
            FONT_10_91,
        ));
    }

    #[test]
    fn test_keep_space_small_negative_gap() {
        // Even slightly negative gaps should be kept
        assert!(!should_skip_generated_space(0.0, false, -0.01, FONT_10_91));
    }

    // --- Edge cases ---

    #[test]
    fn test_keep_space_very_small_font() {
        // Tiny font (1pt) — proportional threshold still applies
        assert!(!should_skip_generated_space(0.0, false, 0.2, 1.0));
    }

    #[test]
    fn test_skip_space_very_small_font_tiny_gap() {
        // Tiny font with proportionally tiny gap
        assert!(should_skip_generated_space(0.0, false, 0.1, 1.0));
    }

    #[test]
    fn test_keep_space_epsilon_width() {
        // Space width exactly at epsilon — treated as non-zero (real space)
        assert!(!should_skip_generated_space(
            f32::EPSILON,
            false,
            0.0,
            FONT_10_91,
        ));
    }

    #[test]
    fn test_keep_space_just_above_epsilon_width() {
        // Space width just above epsilon — real space
        assert!(!should_skip_generated_space(
            f32::EPSILON * 2.0,
            false,
            0.0,
            FONT_10_91,
        ));
    }
}
