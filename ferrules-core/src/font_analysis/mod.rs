//! Universal font corruption detection and correction
//!
//! This module provides automatic detection and correction of font corruption
//! in PDF documents by analyzing glyph names and mapping them to correct
//! Unicode characters using the Adobe Glyph List standard.

pub mod adobe_glyph_list;
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
pub fn correct_character_with_universal_corrector(char_code: u32, font_name: &str) -> Option<char> {
    let corrector = GLOBAL_CORRECTOR.get()?;
    let guard = corrector.lock().ok()?;
    guard.as_ref()?.correct_character(char_code, font_name)
}
