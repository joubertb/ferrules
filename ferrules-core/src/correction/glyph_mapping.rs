use std::collections::HashMap;
use std::sync::LazyLock;

/// Adobe Glyph List mapping from glyph names to Unicode code points
/// Based on Adobe's official glyph list specification
/// https://github.com/adobe-type-tools/agl-specification
static ADOBE_GLYPH_LIST: LazyLock<HashMap<&'static str, u32>> = LazyLock::new(|| {
    let mut map = HashMap::new();

    // Mathematical symbols commonly found in corrupted PDFs
    map.insert("parenleft", 0x0028); // (
    map.insert("parenright", 0x0029); // )
    map.insert("braceleft", 0x007B); // {
    map.insert("braceright", 0x007D); // }
    map.insert("bracketleft", 0x005B); // [
    map.insert("bracketright", 0x005D); // ]

    // Common ligatures
    map.insert("fi", 0xFB01); // fi ligature
    map.insert("fl", 0xFB02); // fl ligature
    map.insert("ff", 0xFB00); // ff ligature
    map.insert("ffi", 0xFB03); // ffi ligature
    map.insert("ffl", 0xFB04); // ffl ligature

    // Basic Latin letters (for reference)
    map.insert("h", 0x0068); // h
    map.insert("i", 0x0069); // i
    map.insert("a", 0x0061); // a
    map.insert("b", 0x0062); // b
    map.insert("c", 0x0063); // c
    map.insert("d", 0x0064); // d
    map.insert("e", 0x0065); // e
    map.insert("f", 0x0066); // f
    map.insert("g", 0x0067); // g
    map.insert("j", 0x006A); // j
    map.insert("k", 0x006B); // k
    map.insert("l", 0x006C); // l
    map.insert("m", 0x006D); // m
    map.insert("n", 0x006E); // n
    map.insert("o", 0x006F); // o
    map.insert("p", 0x0070); // p
    map.insert("q", 0x0071); // q
    map.insert("r", 0x0072); // r
    map.insert("s", 0x0073); // s
    map.insert("t", 0x0074); // t
    map.insert("u", 0x0075); // u
    map.insert("v", 0x0076); // v
    map.insert("w", 0x0077); // w
    map.insert("x", 0x0078); // x
    map.insert("y", 0x0079); // y
    map.insert("z", 0x007A); // z

    map
});

/// Font glyph name resolution system
/// Implements the PDF viewer approach: code → glyph → glyph name → Unicode
pub struct GlyphNameResolver {
    // Cache for resolved glyph names to avoid repeated system font queries
    glyph_cache: HashMap<(String, u32), Option<String>>,
}

impl GlyphNameResolver {
    pub fn new() -> Self {
        Self {
            glyph_cache: HashMap::new(),
        }
    }

    /// Resolves Unicode value using glyph name lookup
    /// This mirrors what PDF viewers do for correct rendering
    pub fn resolve_unicode_from_glyph_name(
        &mut self,
        font_name: &str,
        unicode_value: u32,
        original_char: char,
    ) -> Option<char> {
        // First check if we need glyph name resolution
        if !self.needs_glyph_resolution(font_name, unicode_value) {
            return None;
        }

        // Get glyph name from system font (this is what PDF viewers do)
        let glyph_name = self.get_system_font_glyph_name(font_name, unicode_value)?;

        // Look up correct Unicode from glyph name using Adobe Glyph List
        let correct_unicode = ADOBE_GLYPH_LIST.get(glyph_name.as_str())?;

        // Convert to character
        let correct_char = char::from_u32(*correct_unicode)?;

        // Only return if it's different from original
        if correct_char != original_char {
            eprintln!(
                "🔧 GLYPH NAME CORRECTION: Font '{font_name}' - Code 0x{unicode_value:04X} '{original_char}' → Glyph '{glyph_name}' → Unicode 0x{correct_unicode:04X} '{correct_char}'"
            );
            Some(correct_char)
        } else {
            None
        }
    }

    /// Determines if a font/unicode combination needs glyph name resolution
    fn needs_glyph_resolution(&self, font_name: &str, unicode_value: u32) -> bool {
        // Mathematical symbol fonts are most likely to be corrupted
        let is_symbol_font = font_name.contains("CMSY") || font_name.contains("CMEX");

        // Subset fonts (contain '+') often have corrupted mappings, but exclude NimbusRomNo9L
        let is_subset_font = font_name.contains('+') && !font_name.contains("NimbusRomNo9L");

        // Focus on characters that are commonly corrupted in mathematical contexts
        let is_suspicious_unicode = matches!(
            unicode_value,
            0x0068 | 0x0069 |  // h, i (often corrupted to parentheses in SYMBOL fonts only)
            0x007B | 0x007D |  // {, } (often corrupted ligatures)  
            0x005B | 0x005D // [, ] (bracket corruptions)
        );

        // Special case: CMSY fonts - both 'h' and 'i' need correction (these are symbol fonts)
        if font_name.contains("CMSY") {
            return matches!(unicode_value, 0x0068 | 0x0069); // 'h' → '(', 'i' → ')'
        }

        // CMMI fonts are mathematical italic fonts - system font fallback gives correct results
        // Do NOT apply glyph resolution to CMMI fonts
        if font_name.contains("CMMI") {
            return false;
        }

        (is_symbol_font || is_subset_font) && is_suspicious_unicode
    }

    /// Gets glyph name from system font
    /// This simulates what PDF viewers do when font glyph lookup fails
    fn get_system_font_glyph_name(
        &mut self,
        font_name: &str,
        unicode_value: u32,
    ) -> Option<String> {
        // Check cache first
        let cache_key = (font_name.to_string(), unicode_value);
        if let Some(cached) = self.glyph_cache.get(&cache_key) {
            return cached.clone();
        }

        // For now, implement a knowledge-based mapping based on our observations
        // In a full implementation, this would query actual system fonts
        let glyph_name = self.get_known_glyph_mapping(font_name, unicode_value);

        // Cache the result
        self.glyph_cache.insert(cache_key, glyph_name.clone());

        glyph_name
    }

    /// Knowledge-based glyph mapping from our analysis
    /// This represents what we know system fonts return for these character codes
    fn get_known_glyph_mapping(&self, font_name: &str, unicode_value: u32) -> Option<String> {
        // CMSY fonts: both 'h' and 'i' are corrupted
        if font_name.contains("CMSY") {
            return match unicode_value {
                0x0068 => Some("parenleft".to_string()),  // 'h' → '('
                0x0069 => Some("parenright".to_string()), // 'i' → ')'
                _ => None,
            };
        }

        // CMMI fonts are Computer Modern Math Italic - when system font fallback occurs,
        // system fonts would return "i" for 0x69, not "parenright"
        // These are mathematical variables, not symbols, so we should NOT correct them

        // REMOVED: NimbusRomNo9L glyph mappings - these are regular text fonts, not corrupted

        // Default: assume standard glyph names for regular fonts
        match unicode_value {
            0x0068 => Some("h".to_string()),
            0x0069 => Some("i".to_string()),
            0x007B => Some("braceleft".to_string()),
            0x007D => Some("braceright".to_string()),
            0x005B => Some("bracketleft".to_string()),
            0x005D => Some("bracketright".to_string()),
            _ => None,
        }
    }
}

impl Default for GlyphNameResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adobe_glyph_list() {
        assert_eq!(ADOBE_GLYPH_LIST.get("parenleft"), Some(&0x0028));
        assert_eq!(ADOBE_GLYPH_LIST.get("parenright"), Some(&0x0029));
        assert_eq!(ADOBE_GLYPH_LIST.get("fi"), Some(&0xFB01));
    }

    #[test]
    fn test_cmsy_font_correction() {
        let mut resolver = GlyphNameResolver::new();

        // CMSY font with corrupted parentheses - pure glyph name mapping
        let result = resolver.resolve_unicode_from_glyph_name("CMSY10", 0x0068, 'h');
        assert_eq!(result, Some('('));

        let result = resolver.resolve_unicode_from_glyph_name("CMSY10", 0x0069, 'i');
        assert_eq!(result, Some(')'));
    }

    #[test]
    fn test_no_correction_for_regular_fonts() {
        let mut resolver = GlyphNameResolver::new();

        // Regular font should not be corrected
        let result = resolver.resolve_unicode_from_glyph_name("Times-Roman", 0x0068, 'h');
        assert_eq!(result, None);
    }
}
