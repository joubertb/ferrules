use anyhow::{anyhow, Result};
use lopdf::{Document, Object, ObjectId, Stream};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Font analysis using lopdf to access low-level font dictionaries and encoding tables
#[derive(Debug, Clone)]
pub struct FontAnalysis {
    pub font_name: String,
    pub is_subset: bool,
    pub has_tounicode: bool,
    pub tounicode_mappings: HashMap<u16, String>,
    pub encoding_issues: Vec<String>,
    pub font_type: FontType,
}

#[derive(Debug, Clone)]
pub enum FontType {
    Type1,
    Type1C,
    TrueType,
    Type0,
    Type3,
    Unknown,
}

#[derive(Debug)]
pub struct FontCorruptionDetector {
    document: Document,
    font_cache: HashMap<ObjectId, FontAnalysis>,
}

impl FontCorruptionDetector {
    pub fn new(pdf_path: &str) -> Result<Self> {
        let document = Document::load(pdf_path)?;
        Ok(Self {
            document,
            font_cache: HashMap::new(),
        })
    }

    /// Analyze all fonts in the PDF document
    pub fn analyze_all_fonts(&mut self) -> Result<Vec<FontAnalysis>> {
        let mut fonts = Vec::new();

        // Get all pages and their resources
        for page_id in self.document.get_pages().values() {
            if let Ok(page) = self.document.get_object(*page_id) {
                if let Ok(resources) = self.extract_page_resources(page) {
                    for font_ref in resources {
                        if let Ok(analysis) = self.analyze_font(font_ref) {
                            fonts.push(analysis);
                        }
                    }
                }
            }
        }

        Ok(fonts)
    }

    /// Extract font references from page resources
    fn extract_page_resources(&self, page: &Object) -> Result<Vec<ObjectId>> {
        let mut font_refs = Vec::new();

        if let Object::Dictionary(ref dict) = page {
            // Try direct resources access first
            if let Ok(Object::Dictionary(ref resources)) = dict.get(b"Resources") {
                if let Ok(Object::Dictionary(ref font_dict)) = resources.get(b"Font") {
                    for (_, font_obj) in font_dict.iter() {
                        if let Object::Reference(font_ref) = font_obj {
                            font_refs.push(*font_ref);
                        }
                    }
                }
            }
            // Try indirect resources access
            else if let Ok(Object::Reference(resources_ref)) = dict.get(b"Resources") {
                if let Ok(Object::Dictionary(ref resources)) =
                    self.document.get_object(*resources_ref)
                {
                    if let Ok(Object::Dictionary(ref font_dict)) = resources.get(b"Font") {
                        for (_, font_obj) in font_dict.iter() {
                            if let Object::Reference(font_ref) = font_obj {
                                font_refs.push(*font_ref);
                            }
                        }
                    }
                    // Try indirect font dict access
                    else if let Ok(Object::Reference(font_dict_ref)) = resources.get(b"Font") {
                        if let Ok(Object::Dictionary(ref font_dict)) =
                            self.document.get_object(*font_dict_ref)
                        {
                            for (_, font_obj) in font_dict.iter() {
                                if let Object::Reference(font_ref) = font_obj {
                                    font_refs.push(*font_ref);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(font_refs)
    }

    /// Analyze a specific font object
    pub fn analyze_font(&mut self, font_ref: ObjectId) -> Result<FontAnalysis> {
        // Check cache first
        if let Some(cached) = self.font_cache.get(&font_ref) {
            return Ok(cached.clone());
        }

        let font_obj = self.document.get_object(font_ref)?;
        let font_dict = font_obj
            .as_dict()
            .map_err(|_| anyhow!("Font object is not a dictionary"))?;

        // Extract basic font information
        let base_font = font_dict
            .get(b"BaseFont")
            .ok()
            .and_then(|obj| obj.as_name_str().ok())
            .unwrap_or("Unknown");

        let font_type = self.determine_font_type(font_dict)?;
        let is_subset = base_font.contains('+');

        info!(
            "🔍 Analyzing font: {} (Type: {:?}, Subset: {})",
            base_font, font_type, is_subset
        );

        // Check for ToUnicode mapping
        let (has_tounicode, tounicode_mappings, encoding_issues) =
            self.analyze_tounicode_mapping(font_dict)?;

        let analysis = FontAnalysis {
            font_name: base_font.to_string(),
            is_subset,
            has_tounicode,
            tounicode_mappings,
            encoding_issues,
            font_type,
        };

        // Cache the analysis
        self.font_cache.insert(font_ref, analysis.clone());

        Ok(analysis)
    }

    /// Determine the type of font
    fn determine_font_type(&self, font_dict: &lopdf::Dictionary) -> Result<FontType> {
        if let Ok(subtype) = font_dict.get(b"Subtype") {
            match subtype.as_name_str()? {
                "Type1" => Ok(FontType::Type1),
                "Type1C" => Ok(FontType::Type1C),
                "TrueType" => Ok(FontType::TrueType),
                "Type0" => Ok(FontType::Type0),
                "Type3" => Ok(FontType::Type3),
                _ => Ok(FontType::Unknown),
            }
        } else {
            Ok(FontType::Unknown)
        }
    }

    /// Analyze ToUnicode mapping for character corruption detection
    fn analyze_tounicode_mapping(
        &self,
        font_dict: &lopdf::Dictionary,
    ) -> Result<(bool, HashMap<u16, String>, Vec<String>)> {
        let mut encoding_issues = Vec::new();
        let mut tounicode_mappings = HashMap::new();

        // Check for ToUnicode entry
        if let Ok(tounicode_obj) = font_dict.get(b"ToUnicode") {
            debug!("📋 ToUnicode mapping found");

            match tounicode_obj {
                Object::Reference(tounicode_ref) => {
                    if let Ok(tounicode_stream) = self.document.get_object(*tounicode_ref) {
                        if let Object::Stream(ref stream) = tounicode_stream {
                            match self.parse_tounicode_cmap(stream) {
                                Ok(mappings) => {
                                    tounicode_mappings = mappings;
                                    info!(
                                        "✅ Parsed {} ToUnicode mappings",
                                        tounicode_mappings.len()
                                    );
                                }
                                Err(e) => {
                                    encoding_issues
                                        .push(format!("Failed to parse ToUnicode CMap: {}", e));
                                    warn!("⚠️  ToUnicode parsing failed: {}", e);
                                }
                            }
                        }
                    }
                }
                _ => {
                    encoding_issues.push("ToUnicode is not a stream reference".to_string());
                }
            }

            Ok((true, tounicode_mappings, encoding_issues))
        } else {
            debug!("❌ No ToUnicode mapping found");
            encoding_issues.push("Missing ToUnicode mapping".to_string());
            Ok((false, tounicode_mappings, encoding_issues))
        }
    }

    /// Parse ToUnicode CMap to extract character mappings
    fn parse_tounicode_cmap(&self, stream: &Stream) -> Result<HashMap<u16, String>> {
        let mut mappings = HashMap::new();

        // Get raw stream data and decode it
        let raw_data = &stream.content;
        let content_str = String::from_utf8_lossy(raw_data);

        debug!(
            "🗺️  ToUnicode CMap content (first 500 chars): {}",
            &content_str.chars().take(500).collect::<String>()
        );

        // Parse CMap format - look for bfchar and bfrange sections
        self.parse_bfchar_mappings(&content_str, &mut mappings)?;
        self.parse_bfrange_mappings(&content_str, &mut mappings)?;

        Ok(mappings)
    }

    /// Parse bfchar mappings (single character mappings)
    fn parse_bfchar_mappings(
        &self,
        content: &str,
        mappings: &mut HashMap<u16, String>,
    ) -> Result<()> {
        let mut in_bfchar = false;

        for line in content.lines() {
            let line = line.trim();

            if line.contains("beginbfchar") {
                in_bfchar = true;
                debug!("📝 Started parsing bfchar section");
                continue;
            }

            if line.contains("endbfchar") {
                in_bfchar = false;
                debug!("✅ Finished parsing bfchar section");
                continue;
            }

            if in_bfchar && !line.is_empty() {
                if let Ok((char_code, unicode_value)) = self.parse_bfchar_line(line) {
                    mappings.insert(char_code, unicode_value);
                }
            }
        }

        Ok(())
    }

    /// Parse a single bfchar mapping line
    fn parse_bfchar_line(&self, line: &str) -> Result<(u16, String)> {
        // Expected format: <XX> <YYYY> where XX is hex char code, YYYY is hex Unicode
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let char_code_str = parts[0].trim_matches(['<', '>']);
            let unicode_str = parts[1].trim_matches(['<', '>']);

            let char_code = u16::from_str_radix(char_code_str, 16)?;

            // Convert hex Unicode to actual character
            let unicode_value = if unicode_str.len() == 4 {
                let code_point = u32::from_str_radix(unicode_str, 16)?;
                if let Some(ch) = char::from_u32(code_point) {
                    ch.to_string()
                } else {
                    format!("U+{}", unicode_str.to_uppercase())
                }
            } else {
                format!("U+{}", unicode_str.to_uppercase())
            };

            debug!(
                "📍 Mapping: {} -> {} ({})",
                char_code, unicode_value, unicode_str
            );
            Ok((char_code, unicode_value))
        } else {
            Err(anyhow!("Invalid bfchar line format: {}", line))
        }
    }

    /// Parse bfrange mappings (range mappings)
    fn parse_bfrange_mappings(
        &self,
        content: &str,
        mappings: &mut HashMap<u16, String>,
    ) -> Result<()> {
        let mut in_bfrange = false;

        for line in content.lines() {
            let line = line.trim();

            if line.contains("beginbfrange") {
                in_bfrange = true;
                debug!("📝 Started parsing bfrange section");
                continue;
            }

            if line.contains("endbfrange") {
                in_bfrange = false;
                debug!("✅ Finished parsing bfrange section");
                continue;
            }

            if in_bfrange && !line.is_empty() {
                if let Ok(range_mappings) = self.parse_bfrange_line(line) {
                    for (char_code, unicode_value) in range_mappings {
                        mappings.insert(char_code, unicode_value);
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse a single bfrange mapping line
    fn parse_bfrange_line(&self, line: &str) -> Result<Vec<(u16, String)>> {
        // Expected format: <XX> <YY> <ZZZZ> where XX-YY is range, ZZZZ is starting Unicode
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let start_str = parts[0].trim_matches(['<', '>']);
            let end_str = parts[1].trim_matches(['<', '>']);
            let unicode_str = parts[2].trim_matches(['<', '>']);

            let start_code = u16::from_str_radix(start_str, 16)?;
            let end_code = u16::from_str_radix(end_str, 16)?;
            let start_unicode = u32::from_str_radix(unicode_str, 16)?;

            let mut range_mappings = Vec::new();
            for i in 0..=(end_code - start_code) {
                let char_code = start_code + i;
                let unicode_code = start_unicode + i as u32;

                let unicode_value = if let Some(ch) = char::from_u32(unicode_code) {
                    ch.to_string()
                } else {
                    format!("U+{:04X}", unicode_code)
                };

                debug!(
                    "📍 Range mapping: {} -> {} (U+{:04X})",
                    char_code, unicode_value, unicode_code
                );
                range_mappings.push((char_code, unicode_value));
            }

            Ok(range_mappings)
        } else {
            Err(anyhow!("Invalid bfrange line format: {}", line))
        }
    }

    /// Detect character corruption based on font analysis
    pub fn detect_corruption(
        &self,
        char_code: u16,
        extracted_char: char,
        font_analysis: &FontAnalysis,
    ) -> Option<String> {
        // Check if character mapping exists in ToUnicode
        if let Some(expected_unicode) = font_analysis.tounicode_mappings.get(&char_code) {
            let extracted_str = extracted_char.to_string();
            if &extracted_str != expected_unicode {
                warn!(
                    "🚨 Corruption detected: Code {} extracted as '{}' but ToUnicode maps to '{}'",
                    char_code, extracted_char, expected_unicode
                );
                return Some(expected_unicode.clone());
            }
        }

        // Check for common corruption patterns based on font type and subset status
        if font_analysis.is_subset {
            match extracted_char {
                '(' => return Some("h".to_string()),
                ')' => return Some("i".to_string()),
                '[' => return Some("fi".to_string()),
                ']' => return Some("fl".to_string()),
                '{' => return Some("ff".to_string()),
                '}' => return Some("ffi".to_string()),
                _ => {}
            }
        }

        None
    }

    /// Print comprehensive font analysis report
    pub fn print_font_report(&self, analyses: &[FontAnalysis]) {
        info!("📊 FONT ANALYSIS REPORT");
        info!("=====================");
        info!("Total fonts analyzed: {}", analyses.len());

        for analysis in analyses {
            info!("🔤 Font: {}", analysis.font_name);
            info!("  Type: {:?}", analysis.font_type);
            info!("  Subset: {}", analysis.is_subset);
            info!("  Has ToUnicode: {}", analysis.has_tounicode);
            info!(
                "  ToUnicode mappings: {}",
                analysis.tounicode_mappings.len()
            );

            if !analysis.encoding_issues.is_empty() {
                info!("  ⚠️  Issues:");
                for issue in &analysis.encoding_issues {
                    info!("    - {}", issue);
                }
            }

            if !analysis.tounicode_mappings.is_empty() {
                info!("  📋 Sample mappings:");
                for (code, unicode) in analysis.tounicode_mappings.iter().take(10) {
                    info!("    {} -> {}", code, unicode);
                }
                if analysis.tounicode_mappings.len() > 10 {
                    info!(
                        "    ... and {} more",
                        analysis.tounicode_mappings.len() - 10
                    );
                }
            }

            info!("");
        }
    }
}
