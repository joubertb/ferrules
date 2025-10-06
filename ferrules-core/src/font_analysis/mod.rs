//! Universal font corruption detection and correction
//!
//! This module provides automatic detection and correction of font corruption
//! in PDF documents by analyzing glyph names and mapping them to correct
//! Unicode characters using the Adobe Glyph List standard.
//!
//! This module also provides text-level corrections that complement the
//! font-level corrections, offering a complete text correction solution.

pub mod adobe_glyph_list;
pub mod dictionary;
pub mod text_corrections;
pub mod universal_corrector;

pub use universal_corrector::UniversalFontCorrector;

use std::sync::{Mutex, OnceLock};

/// Global Universal Font Corrector instance
static GLOBAL_CORRECTOR: OnceLock<Mutex<Option<UniversalFontCorrector>>> = OnceLock::new();

/// Initialize the global font corrector with PDF data
pub fn initialize_universal_corrector(pdf_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let corrector = GLOBAL_CORRECTOR.get_or_init(|| Mutex::new(None));
    let mut guard = corrector
        .lock()
        .map_err(|_| "Failed to lock corrector mutex")?;

    let mut universal_corrector = UniversalFontCorrector::new();
    universal_corrector.analyze_pdf(pdf_data)?;
    *guard = Some(universal_corrector);

    Ok(())
}

/// Get access to the global universal corrector for character correction
pub fn correct_character_with_universal_corrector(
    char_code: u32,
    font_name: &str,
) -> Option<String> {
    let corrector = GLOBAL_CORRECTOR.get()?;
    let guard = corrector.lock().ok()?;
    guard.as_ref()?.correct_character(char_code, font_name)
}

/// Get character correction using encoding differences (primary method with Adobe Glyph List)
pub fn correct_character_with_encoding_differences(
    char_code: u32,
    font_name: &str,
) -> Option<String> {
    let corrector = GLOBAL_CORRECTOR.get()?;
    let guard = corrector.lock().ok()?;
    guard
        .as_ref()?
        .correct_character_with_encoding_differences(char_code, font_name)
}

// ============================================================================
// Wrapper Functions for Compatibility with Previous Correction Module
// ============================================================================

/// Apply text corrections to assembled text
///
/// This is a compatibility wrapper that provides the same API as the previous
/// correction module's correct_assembled_text function.
pub fn correct_assembled_text(text: &str) -> String {
    text_corrections::correct_assembled_text(text)
}

/// Apply word-level corrections to assembled text (in-place)
///
/// This is a compatibility wrapper that provides the same API as the previous
/// correction module's apply_word_corrections function.
pub fn apply_word_corrections(text: &mut String) {
    let corrected = correct_assembled_text(text);
    if corrected != *text {
        *text = corrected;
    }
}

/// Apply dictionary corrections directly to spans (modifies span text in place)
///
/// This fixes character insertion issues like 'sysfitems' → 'systems' at the span level
/// before HTML processing occurs, ensuring corrections are preserved in the final output.
pub fn correct_spans_with_dictionary(spans: &mut [crate::entities::CharSpan]) {
    text_corrections::correct_spans_with_dictionary(spans)
}

/// Apply all text corrections to a block
///
/// This is a compatibility wrapper that provides the same API as the previous
/// correction module's correct_block function.
pub fn correct_block(block: &mut crate::blocks::Block) {
    use crate::blocks::BlockType;

    match &mut block.kind {
        BlockType::TextBlock(text_block) => {
            apply_word_corrections(&mut text_block.text);
            if let Some(ref mut fertext) = text_block.fertext {
                apply_word_corrections(fertext);
            }
        }
        BlockType::ListBlock(list_block) => {
            for item in &mut list_block.items {
                apply_word_corrections(item);
            }
        }
        BlockType::Title(title) => {
            apply_word_corrections(&mut title.text);
            if let Some(ref mut fertext) = title.fertext {
                apply_word_corrections(fertext);
            }
        }
        BlockType::Header(header) => {
            apply_word_corrections(&mut header.text);
        }
        BlockType::Footer(footer) => {
            apply_word_corrections(&mut footer.text);
        }
        BlockType::Formula(_) => {
            // No text corrections applied to formulas - kept as raw pdfium output
        }
        BlockType::Image(_) => {
            // No text to correct in image blocks
        }
        BlockType::Table => {
            // Table structure is not directly accessible from Block
            // Table text is handled during merge operations
        }
        BlockType::Figure(_) => {
            // No text to correct in figure blocks
        }
    }
}

/// Apply text corrections to multiple blocks efficiently
///
/// This is a compatibility wrapper that provides the same API as the previous
/// correction module's correct_blocks function.
pub fn correct_blocks(blocks: &mut [crate::blocks::Block]) {
    if blocks.is_empty() {
        return;
    }

    tracing::debug!("🔤 Applying text corrections to {} blocks", blocks.len());

    for block in blocks {
        correct_block(block);
    }
}

/// Apply character-level corrections to text
///
/// This is a compatibility wrapper that provides the same API as the previous
/// correction module's correct_characters function.
pub fn correct_characters(text: &str) -> String {
    text_corrections::filter_control_characters(text)
}

/// Apply character-level corrections using the default corrector
///
/// This is a compatibility wrapper that provides the same API as the previous
/// correction module's apply_character_corrections function.
pub fn apply_character_corrections(text: &str) -> String {
    text_corrections::apply_character_corrections(text)
}

/// Initialize the correction system
///
/// This is a compatibility wrapper that provides the same API as the previous
/// correction module's initialize function.
pub fn initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Font analysis correction system doesn't require explicit initialization
    // beyond what happens during PDF processing
    Ok(())
}

/// Fix character encoding corruption (legacy compatibility function)
///
/// This function provides backwards compatibility for existing code that calls
/// fix_character_encoding_corruption. It delegates to the text correction system.
pub fn fix_character_encoding_corruption(text: &str) -> String {
    text_corrections::filter_control_characters(text)
}

/// Fix character encoding corruption with font information (legacy compatibility)
///
/// This function provides backwards compatibility for existing code.
/// Font-specific corrections are now handled internally by the unified correction system.
pub fn fix_character_encoding_corruption_with_font(text: &str, _font_name: Option<&str>) -> String {
    fix_character_encoding_corruption(text)
}

/// Sets up document context for font corruption analysis (legacy compatibility)
///
/// This is now handled automatically by the universal corrector during PDF processing.
pub fn set_document_context() {
    // Universal corrector handles document context automatically
}

/// Clears document context after parsing is complete (legacy compatibility)
///
/// This is now handled automatically by the universal corrector.
pub fn clear_document_context() {
    // Universal corrector handles cleanup automatically
}

/// Initialize correction engine with proper error handling for CLI applications
///
/// This provides a CLI-friendly interface for initialization with proper error reporting.
/// Returns an error that can be displayed to users if initialization fails.
pub fn initialize_for_cli() -> Result<(), Box<dyn std::error::Error>> {
    // Font analysis correction system doesn't require explicit initialization
    // beyond what happens during PDF processing
    Ok(())
}

/// Display correction configuration information for CLI verbose mode
///
/// Shows cache configuration and correction engine status for CLI applications.
/// This replaces the feature flag duplication in CLI main.rs files.
pub fn display_cli_config_info() {
    #[cfg(feature = "correction-engine")]
    {
        println!("📋 Font Analysis Correction Settings");
        println!("  Universal font corrector: enabled");
        println!("  Mathematical symbol corrections: enabled");
        println!("  Character filtering: enabled");
        println!();
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        println!("🔧 Font analysis correction engine disabled at compile time");
        println!();
    }
}
