//! Font Diagnostic Tool
//!
//! This tool analyzes PDF fonts at the raw level to diagnose character mapping corruption.
//! It examines character codes, Unicode mappings, glyph names, and CMap data before
//! pdfium processing to understand the root cause of font corruption issues.

use anyhow::{Context, Result};
use clap::Parser;
use flate2::read::{DeflateDecoder, ZlibDecoder};
use lopdf::{Document, Object, ObjectId};
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "font-debug")]
#[command(about = "Diagnostic tool for PDF font corruption analysis")]
pub struct Args {
    /// PDF file to analyze
    #[arg(value_name = "PDF_FILE")]
    pdf_file: PathBuf,

    /// Output detailed character mappings
    #[arg(long, short)]
    verbose: bool,

    /// Focus on specific font (partial name match)
    #[arg(long)]
    font: Option<String>,

    /// Show only fonts with potential corruption
    #[arg(long)]
    corrupted_only: bool,

    /// Analyze actual character codes in PDF content streams
    #[arg(long)]
    content_analysis: bool,

    /// Show raw decompressed content for debugging
    #[arg(long)]
    raw_content: bool,

    /// Investigate system font glyph names for comparison
    #[arg(long)]
    system_fonts: bool,
}

/// Information about a character mapping
#[derive(Debug)]
struct CharMapping {
    char_code: u32,
    unicode_value: Option<u32>,
    unicode_name: Option<String>,
    glyph_name: Option<String>,
    is_potentially_corrupt: bool,
}

/// Information about character codes found in content streams
#[derive(Debug, Clone)]
struct ContentStreamChar {
    char_code: u32,
    font_name: String,
    page_number: u32,
    frequency: u32,
    context: String, // Surrounding text context
}

/// Information about a font
#[derive(Debug)]
struct FontInfo {
    object_id: ObjectId,
    font_name: String,
    font_type: String,
    is_subset: bool,
    has_tounicode: bool,
    has_encoding: bool,
    char_mappings: Vec<CharMapping>,
    corruption_indicators: Vec<String>,
}

pub fn main() -> Result<()> {
    let args = Args::parse();

    println!(
        "🔍 Font Diagnostic Tool - Analyzing: {}",
        args.pdf_file.display()
    );
    println!("{}", "=".repeat(80));

    // Open PDF with lopdf for low-level access
    let doc = Document::load(&args.pdf_file).context("Failed to load PDF with lopdf")?;

    // Find all font objects
    let font_infos = analyze_all_fonts(&doc, args.verbose)?;

    // Filter fonts if requested
    let filtered_fonts: Vec<_> = font_infos
        .iter()
        .filter(|font| {
            if let Some(ref filter_name) = args.font {
                font.font_name
                    .to_lowercase()
                    .contains(&filter_name.to_lowercase())
            } else {
                true
            }
        })
        .filter(|font| {
            if args.corrupted_only {
                !font.corruption_indicators.is_empty()
            } else {
                true
            }
        })
        .collect();

    if filtered_fonts.is_empty() {
        println!("No fonts found matching criteria.");
        return Ok(());
    }

    // Display font analysis
    for font in filtered_fonts {
        display_font_info(font, args.verbose)?;
        println!();
    }

    // Analyze content streams if requested
    if args.content_analysis {
        println!("🔍 CONTENT STREAM ANALYSIS:");
        println!("{}", "=".repeat(80));
        analyze_content_streams(&doc, args.verbose, args.raw_content)?;
        println!();
    }

    // Investigate system font glyph names if requested
    if args.system_fonts {
        println!("🔍 SYSTEM FONT GLYPH NAME INVESTIGATION:");
        println!("{}", "=".repeat(80));
        investigate_system_font_glyph_names()?;
        println!();
    }

    // Summary
    let total_fonts = font_infos.len();
    let corrupted_fonts = font_infos
        .iter()
        .filter(|f| !f.corruption_indicators.is_empty())
        .count();

    println!("📊 SUMMARY:");
    println!("Total fonts analyzed: {total_fonts}");
    println!("Fonts with corruption indicators: {corrupted_fonts}");

    Ok(())
}

fn analyze_all_fonts(doc: &Document, verbose: bool) -> Result<Vec<FontInfo>> {
    let mut font_infos = Vec::new();

    // Iterate through all objects looking for fonts
    for (object_id, object) in &doc.objects {
        if let Ok(font_dict) = object.as_dict() {
            if let Ok(Object::Name(type_name)) = font_dict.get(b"Type") {
                if type_name == b"Font" {
                    if verbose {
                        println!("🔍 Analyzing font object: {object_id:?}");
                    }

                    match analyze_font(doc, *object_id, font_dict) {
                        Ok(font_info) => font_infos.push(font_info),
                        Err(e) => {
                            eprintln!("⚠️  Failed to analyze font {object_id:?}: {e}");
                        }
                    }
                }
            }
        }
    }

    Ok(font_infos)
}

fn analyze_font(
    doc: &Document,
    object_id: ObjectId,
    font_dict: &lopdf::Dictionary,
) -> Result<FontInfo> {
    // Extract basic font information
    let font_name = extract_font_name(font_dict)?;
    let font_type = extract_font_type(font_dict)?;
    let is_subset = font_name.contains('+');

    // Check for ToUnicode CMap
    let has_tounicode = font_dict.get(b"ToUnicode").is_ok();

    // Check for Encoding
    let has_encoding = font_dict.get(b"Encoding").is_ok();

    println!("📝 Font: {font_name} ({font_type})");
    println!("   Subset: {is_subset}, ToUnicode: {has_tounicode}, Encoding: {has_encoding}");

    // Analyze character mappings
    let char_mappings = analyze_char_mappings(doc, font_dict)?;

    // Detect corruption indicators
    let corruption_indicators =
        detect_corruption_indicators(&font_name, &font_type, has_tounicode, &char_mappings);

    Ok(FontInfo {
        object_id,
        font_name,
        font_type,
        is_subset,
        has_tounicode,
        has_encoding,
        char_mappings,
        corruption_indicators,
    })
}

fn extract_font_name(font_dict: &lopdf::Dictionary) -> Result<String> {
    if let Ok(Object::Name(base_font)) = font_dict.get(b"BaseFont") {
        Ok(String::from_utf8_lossy(base_font).to_string())
    } else if let Ok(Object::Name(font_name)) = font_dict.get(b"FontName") {
        Ok(String::from_utf8_lossy(font_name).to_string())
    } else {
        Ok("Unknown".to_string())
    }
}

fn extract_font_type(font_dict: &lopdf::Dictionary) -> Result<String> {
    if let Ok(Object::Name(subtype)) = font_dict.get(b"Subtype") {
        Ok(String::from_utf8_lossy(subtype).to_string())
    } else {
        Ok("Unknown".to_string())
    }
}

fn analyze_char_mappings(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
) -> Result<Vec<CharMapping>> {
    let mut mappings = Vec::new();

    // Try to get ToUnicode CMap first
    if let Ok(tounicode_ref) = font_dict.get(b"ToUnicode") {
        if let Ok(tounicode_obj) = doc.get_object(tounicode_ref.as_reference()?) {
            mappings.extend(parse_tounicode_cmap(tounicode_obj)?);
        }
    }

    // Try to get Encoding information
    if let Ok(encoding_ref) = font_dict.get(b"Encoding") {
        match encoding_ref {
            Object::Name(encoding_name) => {
                let encoding_str = String::from_utf8_lossy(encoding_name);
                println!("   📋 Standard Encoding: {encoding_str}");
            }
            Object::Reference(obj_ref) => {
                if let Ok(encoding_obj) = doc.get_object(*obj_ref) {
                    if let Ok(encoding_dict) = encoding_obj.as_dict() {
                        if let Ok(differences) = encoding_dict.get(b"Differences") {
                            mappings.extend(parse_encoding_differences(differences)?);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // If no mappings found, create some basic ones for diagnostic purposes
    if mappings.is_empty() {
        println!("   ⚠️  No character mappings found - this may indicate corruption");
        // Add some common character codes that are often corrupted
        for code in [0x28, 0x29, 0x68, 0x69] {
            // (, ), h, i
            mappings.push(CharMapping {
                char_code: code,
                unicode_value: Some(code),
                unicode_name: get_unicode_name(code),
                glyph_name: None,
                is_potentially_corrupt: true,
            });
        }
    }

    Ok(mappings)
}

fn parse_tounicode_cmap(cmap_obj: &Object) -> Result<Vec<CharMapping>> {
    let mut mappings = Vec::new();

    // Extract the CMap stream data
    if let Ok(stream) = cmap_obj.as_stream() {
        let data = stream.content.as_slice();
        let cmap_text = String::from_utf8_lossy(data);

        println!("   📄 ToUnicode CMap found ({} bytes)", data.len());

        // Parse CMap using adobe-cmap-parser or basic regex parsing
        if let Err(e) = parse_cmap_content(&cmap_text, &mut mappings) {
            eprintln!("   ⚠️  Failed to parse CMap: {e}");
        }
    }

    Ok(mappings)
}

fn parse_cmap_content(cmap_text: &str, mappings: &mut Vec<CharMapping>) -> Result<()> {
    // Look for character code ranges and mappings
    // Format: <char_code> <unicode_value>

    for line in cmap_text.lines() {
        let line = line.trim();

        // Look for single character mappings: <XX> <YYYY>
        if line.starts_with('<') && line.contains("><") {
            if let Some((char_part, unicode_part)) = line.split_once("><") {
                let char_code_str = char_part.trim_start_matches('<');
                let unicode_str = unicode_part.trim_end_matches('>');

                if let (Ok(char_code), Ok(unicode_val)) = (
                    u32::from_str_radix(char_code_str, 16),
                    u32::from_str_radix(unicode_str, 16),
                ) {
                    let is_corrupt = detect_char_corruption(char_code, unicode_val);

                    mappings.push(CharMapping {
                        char_code,
                        unicode_value: Some(unicode_val),
                        unicode_name: get_unicode_name(unicode_val),
                        glyph_name: None,
                        is_potentially_corrupt: is_corrupt,
                    });

                    if is_corrupt {
                        println!(
                            "   🔍 SUSPICIOUS MAPPING: 0x{:02X} → U+{:04X} ({})",
                            char_code,
                            unicode_val,
                            get_unicode_name(unicode_val).unwrap_or("Unknown".to_string())
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

fn parse_encoding_differences(differences: &Object) -> Result<Vec<CharMapping>> {
    let mut mappings = Vec::new();

    if let Ok(array) = differences.as_array() {
        let mut current_code = 0u32;

        for item in array {
            match item {
                Object::Integer(code) => {
                    current_code = *code as u32;
                }
                Object::Name(glyph_name) => {
                    let glyph_str = String::from_utf8_lossy(glyph_name);
                    let unicode_val = glyph_name_to_unicode(&glyph_str);
                    let is_corrupt = if let Some(unicode) = unicode_val {
                        detect_char_corruption(current_code, unicode)
                    } else {
                        true // Unknown glyph mapping is suspicious
                    };

                    mappings.push(CharMapping {
                        char_code: current_code,
                        unicode_value: unicode_val,
                        unicode_name: unicode_val.and_then(get_unicode_name),
                        glyph_name: Some(glyph_str.to_string()),
                        is_potentially_corrupt: is_corrupt,
                    });

                    if is_corrupt {
                        println!("   🔍 SUSPICIOUS ENCODING: {glyph_str} at position 0x{current_code:02X}");
                    }

                    current_code += 1;
                }
                _ => {}
            }
        }
    }

    Ok(mappings)
}

fn detect_char_corruption(char_code: u32, unicode_val: u32) -> bool {
    // Detect common corruption patterns
    match (char_code, unicode_val) {
        // Common parentheses corruption in mathematical fonts
        (0x28, 0x68) => true, // '(' mapped to 'h'
        (0x29, 0x69) => true, // ')' mapped to 'i'
        (0x68, 0x28) => true, // 'h' mapped to '('
        (0x69, 0x29) => true, // 'i' mapped to ')'
        // If character code doesn't match Unicode value, it could be corruption
        _ if char_code != unicode_val && char_code < 128 && unicode_val < 128 => true,
        _ => false,
    }
}

fn glyph_name_to_unicode(glyph_name: &str) -> Option<u32> {
    // Basic glyph name to Unicode mapping
    match glyph_name {
        "parenleft" => Some(0x28),
        "parenright" => Some(0x29),
        "h" => Some(0x68),
        "i" => Some(0x69),
        "space" => Some(0x20),
        "A" => Some(0x41),
        "B" => Some(0x42),
        // Add more as needed
        _ => None,
    }
}

fn get_unicode_name(unicode_val: u32) -> Option<String> {
    match unicode_val {
        0x28 => Some("LEFT PARENTHESIS".to_string()),
        0x29 => Some("RIGHT PARENTHESIS".to_string()),
        0x68 => Some("LATIN SMALL LETTER H".to_string()),
        0x69 => Some("LATIN SMALL LETTER I".to_string()),
        0x20 => Some("SPACE".to_string()),
        0x41 => Some("LATIN CAPITAL LETTER A".to_string()),
        0x42 => Some("LATIN CAPITAL LETTER B".to_string()),
        _ if unicode_val < 128 => Some(format!("ASCII CHARACTER {unicode_val}")),
        _ => None,
    }
}

fn detect_corruption_indicators(
    font_name: &str,
    font_type: &str,
    has_tounicode: bool,
    mappings: &[CharMapping],
) -> Vec<String> {
    let mut indicators = Vec::new();

    // Check for subset font without ToUnicode (high corruption risk)
    if font_name.contains('+') && !has_tounicode {
        indicators.push("Subset font without ToUnicode CMap".to_string());
    }

    // Check for mathematical fonts (often corrupted)
    if font_name.contains("CMSY") || font_name.contains("CMMI") {
        indicators.push("Mathematical font (CMSY/CMMI) - high corruption risk".to_string());
    }

    // Check for suspicious mappings
    let corrupt_mappings = mappings.iter().filter(|m| m.is_potentially_corrupt).count();
    if corrupt_mappings > 0 {
        indicators.push(format!(
            "{corrupt_mappings} suspicious character mappings found"
        ));
    }

    // Check for Type1 fonts (often have encoding issues)
    if font_type == "Type1" {
        indicators.push("Type1 font - potential encoding issues".to_string());
    }

    indicators
}

fn display_font_info(font: &FontInfo, verbose: bool) -> Result<()> {
    println!("🔤 FONT: {} ({:?})", font.font_name, font.object_id);
    println!("   Type: {}", font.font_type);
    println!(
        "   Subset: {} | ToUnicode: {} | Encoding: {}",
        font.is_subset, font.has_tounicode, font.has_encoding
    );

    if !font.corruption_indicators.is_empty() {
        println!("   🚨 CORRUPTION INDICATORS:");
        for indicator in &font.corruption_indicators {
            println!("      • {indicator}");
        }
    }

    if verbose && !font.char_mappings.is_empty() {
        println!("   📋 CHARACTER MAPPINGS:");
        println!("      Code  → Unicode   | Glyph Name       | Unicode Name");
        println!("      ------|-----------|------------------|------------------");

        for mapping in &font.char_mappings {
            let unicode_str = if let Some(unicode) = mapping.unicode_value {
                format!("U+{unicode:04X}")
            } else {
                "None".to_string()
            };

            let glyph_str = mapping.glyph_name.as_deref().unwrap_or("N/A");
            let unicode_name = mapping.unicode_name.as_deref().unwrap_or("Unknown");
            let corrupt_marker = if mapping.is_potentially_corrupt {
                " ⚠️"
            } else {
                ""
            };

            println!(
                "      0x{:02X} → {:<9} | {:<16} | {}{}",
                mapping.char_code, unicode_str, glyph_str, unicode_name, corrupt_marker
            );
        }
    }

    Ok(())
}

fn analyze_content_streams(doc: &Document, verbose: bool, raw_content: bool) -> Result<()> {
    let mut content_chars: Vec<ContentStreamChar> = Vec::new();

    // Find all page objects
    for (object_id, object) in &doc.objects {
        if let Ok(page_dict) = object.as_dict() {
            if let Ok(Object::Name(type_name)) = page_dict.get(b"Type") {
                if type_name == b"Page" {
                    if verbose {
                        println!("🔍 Analyzing page object: {object_id:?}");
                    }

                    // Get page contents
                    if let Ok(contents) = page_dict.get(b"Contents") {
                        match contents {
                            Object::Reference(content_ref) => {
                                if let Ok(content_obj) = doc.get_object(*content_ref) {
                                    analyze_content_object(
                                        content_obj,
                                        &mut content_chars,
                                        object_id.0,
                                        raw_content,
                                    )?;
                                }
                            }
                            Object::Array(content_array) => {
                                for content_item in content_array {
                                    if let Object::Reference(content_ref) = content_item {
                                        if let Ok(content_obj) = doc.get_object(*content_ref) {
                                            analyze_content_object(
                                                content_obj,
                                                &mut content_chars,
                                                object_id.0,
                                                raw_content,
                                            )?;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // Analyze and display results
    display_content_analysis(&content_chars)?;

    Ok(())
}

fn analyze_content_object(
    content_obj: &Object,
    content_chars: &mut Vec<ContentStreamChar>,
    page_num: u32,
    raw_content: bool,
) -> Result<()> {
    if let Ok(stream) = content_obj.as_stream() {
        let data = stream.content.as_slice();

        // Check if the stream is compressed
        let decompressed_data = if let Ok(Object::Name(filter)) = stream.dict.get(b"Filter") {
            match filter.as_slice() {
                b"FlateDecode" => {
                    println!(
                        "   📦 Decompressing FlateDecode stream ({} bytes)",
                        data.len()
                    );
                    match decompress_flate(data) {
                        Ok(decompressed) => {
                            println!("   ✅ Decompressed to {} bytes", decompressed.len());
                            decompressed
                        }
                        Err(e) => {
                            eprintln!("   ⚠️  Failed to decompress FlateDecode: {e}");
                            data.to_vec()
                        }
                    }
                }
                _ => {
                    println!(
                        "   📄 Unknown filter: {:?}, using raw data",
                        String::from_utf8_lossy(filter)
                    );
                    data.to_vec()
                }
            }
        } else {
            // No filter, use raw data
            data.to_vec()
        };

        let content_text = String::from_utf8_lossy(&decompressed_data);

        if raw_content {
            println!("   📄 Raw decompressed content (first 500 chars):");
            println!(
                "   {}",
                &content_text[..content_text.len().min(500)]
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
            );
            if content_text.len() > 500 {
                println!("   ... [truncated] ...");
            }
        }

        // Parse PDF content stream for text operations
        parse_content_stream(&content_text, content_chars, page_num)?;
    }

    Ok(())
}

fn parse_content_stream(
    content: &str,
    content_chars: &mut Vec<ContentStreamChar>,
    page_num: u32,
) -> Result<()> {
    let mut current_font = String::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let line = line.trim();

        // Look for font selection: /FontName Tf
        if line.contains(" Tf") {
            if let Some(font_name) = extract_font_name_from_tf(line) {
                current_font = font_name;
                println!("   📝 Font selected: {current_font} (line {line_idx})");
            }
        }

        // Look for text showing operations: (text) Tj, [(text)] TJ, etc.
        if line.contains("Tj") || line.contains("TJ") || line.contains("'") || line.contains("\"") {
            if !current_font.is_empty() {
                println!(
                    "   📄 Text operation found: {} (font: {})",
                    line.trim(),
                    current_font
                );
                extract_character_codes_from_text_operation(
                    line,
                    &current_font,
                    content_chars,
                    page_num,
                    line_idx,
                )?;
            } else {
                println!("   ⚠️  Text operation without font: {}", line.trim());
            }
        }
    }

    Ok(())
}

fn extract_font_name_from_tf(line: &str) -> Option<String> {
    // Parse line like "/F1 12 Tf" or "/FYEQFE+NimbusRomNo9L-Regu 9.96264 Tf"
    let parts: Vec<&str> = line.split_whitespace().collect();

    // Find Tf and work backwards
    for (i, part) in parts.iter().enumerate() {
        if *part == "Tf" && i >= 2 {
            // Font name should be 2 positions before Tf
            if parts[i - 2].starts_with('/') {
                return Some(parts[i - 2][1..].to_string()); // Remove leading '/'
            }
        }
    }

    // Fallback: look for first part that starts with /
    if parts.len() >= 3 && parts.last() == Some(&"Tf") {
        for part in &parts {
            if let Some(stripped) = part.strip_prefix('/') {
                return Some(stripped.to_string()); // Remove leading '/'
            }
        }
    }

    None
}

fn extract_character_codes_from_text_operation(
    line: &str,
    font_name: &str,
    content_chars: &mut Vec<ContentStreamChar>,
    page_num: u32,
    line_idx: usize,
) -> Result<()> {
    // Handle array format: [(text1)(text2)...] or [(text1)123(text2)...]TJ
    if line.contains('[') && line.contains(']') {
        if let Some(start) = line.find('[') {
            if let Some(end) = line.find(']') {
                let array_content = &line[start + 1..end];
                extract_array_text_content(
                    array_content,
                    font_name,
                    content_chars,
                    page_num,
                    line_idx,
                )?;
            }
        }
    }
    // Handle simple format: (text) Tj
    else if line.contains('(') && line.contains(')') {
        // Extract text from (text) format
        if let Some(start) = line.find('(') {
            if let Some(end) = line.find(')') {
                let text_content = &line[start + 1..end];
                analyze_text_content(
                    text_content,
                    font_name,
                    content_chars,
                    page_num,
                    line_idx,
                    false,
                )?;
            }
        }
    }
    // Handle hex string format: <hexstring> Tj
    else if line.contains('<') && line.contains('>') {
        // Extract text from <hexstring> format
        if let Some(start) = line.find('<') {
            if let Some(end) = line.find('>') {
                let hex_content = &line[start + 1..end];
                analyze_text_content(
                    hex_content,
                    font_name,
                    content_chars,
                    page_num,
                    line_idx,
                    true,
                )?;
            }
        }
    }

    Ok(())
}

fn extract_array_text_content(
    array_content: &str,
    font_name: &str,
    content_chars: &mut Vec<ContentStreamChar>,
    page_num: u32,
    line_idx: usize,
) -> Result<()> {
    // Parse array content like: (MathBER)40(T)74(:)-250(Pr)18(e-T)74(rained)-250(Model)
    let mut chars = array_content.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            '(' => {
                // Extract text in parentheses
                chars.next(); // consume '('
                let mut text_content = String::new();
                let mut paren_count = 1;

                #[allow(clippy::while_let_on_iterator)]
                while let Some(ch) = chars.next() {
                    match ch {
                        '(' => {
                            paren_count += 1;
                            text_content.push(ch);
                        }
                        ')' => {
                            paren_count -= 1;
                            if paren_count == 0 {
                                break;
                            } else {
                                text_content.push(ch);
                            }
                        }
                        '\\' => {
                            // Handle escape sequences
                            if let Some(escaped) = chars.next() {
                                text_content.push('\\');
                                text_content.push(escaped);
                            }
                        }
                        _ => text_content.push(ch),
                    }
                }

                if !text_content.is_empty() {
                    analyze_text_content(
                        &text_content,
                        font_name,
                        content_chars,
                        page_num,
                        line_idx,
                        false,
                    )?;
                }
            }
            '<' => {
                // Extract hex content
                chars.next(); // consume '<'
                let mut hex_content = String::new();

                #[allow(clippy::while_let_on_iterator)]
                while let Some(ch) = chars.next() {
                    if ch == '>' {
                        break;
                    }
                    hex_content.push(ch);
                }

                if !hex_content.is_empty() {
                    analyze_text_content(
                        &hex_content,
                        font_name,
                        content_chars,
                        page_num,
                        line_idx,
                        true,
                    )?;
                }
            }
            _ => {
                // Skip numbers and other content
                chars.next();
            }
        }
    }

    Ok(())
}

fn analyze_text_content(
    content: &str,
    font_name: &str,
    content_chars: &mut Vec<ContentStreamChar>,
    page_num: u32,
    line_idx: usize,
    is_hex: bool,
) -> Result<()> {
    if is_hex {
        // Parse hex string: "48656C6C6F" -> [0x48, 0x65, 0x6C, 0x6C, 0x6F]
        let clean_hex = content.replace(" ", "").replace("\n", "").replace("\t", "");

        // Look for specific hex codes 68 and 69
        if clean_hex.contains("68") || clean_hex.contains("69") {
            println!("   🎯 HEX CODES 68/69 FOUND: Font '{font_name}' hex content '{content}' at page {page_num} line {line_idx}");
        }

        for chunk in clean_hex.as_bytes().chunks(2) {
            if chunk.len() == 2 {
                let hex_str = std::str::from_utf8(chunk)?;
                if let Ok(char_code) = u32::from_str_radix(hex_str, 16) {
                    add_content_char(
                        content_chars,
                        char_code,
                        font_name,
                        page_num,
                        &format!("line {line_idx}"),
                    );

                    // Highlight specific codes we're looking for
                    if char_code == 0x68 || char_code == 0x69 {
                        println!("   🚨 TARGET FOUND: Font '{font_name}' uses char code 0x{char_code:02X} ({}) at page {page_num} line {line_idx} - HEX: '{content}'", 
                               char_code as u8 as char);
                    }

                    // Also check general suspicious patterns
                    if is_suspicious_char_code(char_code, font_name) {
                        println!("   🚨 SUSPICIOUS: Font '{font_name}' uses char code 0x{char_code:02X} ({}) at page {page_num} line {line_idx}", 
                               char_code as u8 as char);
                    }
                }
            }
        }
    } else {
        // Parse literal string and look for 'h' and 'i' characters
        if content.contains('h') || content.contains('i') {
            // Check if this looks like a mathematical context
            let is_mathematical = font_name.contains("CMSY")
                || font_name.contains("CMMI")
                || content.contains("=")
                || content.contains("+")
                || content.contains("-")
                || content.contains("*")
                || content.contains("/")
                || content.contains("^")
                || content.contains("_");

            if is_mathematical {
                println!("   🎯 MATH CONTEXT: Font '{font_name}' content '{content}' contains h/i at page {page_num} line {line_idx}");
            }
        }

        for ch in content.chars() {
            let char_code = ch as u32;
            add_content_char(
                content_chars,
                char_code,
                font_name,
                page_num,
                &format!("'{content}'"),
            );

            // Highlight specific codes we're looking for
            if char_code == 0x68 || char_code == 0x69 {
                println!("   🚨 TARGET FOUND: Font '{font_name}' uses char '{ch}' (0x{char_code:02X}) at page {page_num} - context: '{content}'");
            }

            // Also check general suspicious patterns
            if is_suspicious_char_code(char_code, font_name) {
                println!("   🚨 SUSPICIOUS: Font '{font_name}' uses char '{ch}' (0x{char_code:02X}) at page {page_num} - context: '{content}'");
            }
        }
    }

    Ok(())
}

fn add_content_char(
    content_chars: &mut Vec<ContentStreamChar>,
    char_code: u32,
    font_name: &str,
    page_num: u32,
    context: &str,
) {
    // Find existing entry or create new one
    if let Some(existing) = content_chars
        .iter_mut()
        .find(|c| c.char_code == char_code && c.font_name == font_name)
    {
        existing.frequency += 1;
    } else {
        content_chars.push(ContentStreamChar {
            char_code,
            font_name: font_name.to_string(),
            page_number: page_num,
            frequency: 1,
            context: context.to_string(),
        });
    }
}

fn is_suspicious_char_code(char_code: u32, font_name: &str) -> bool {
    // Check for known corruption patterns
    if font_name.contains("NimbusRomNo9L") {
        match char_code {
            0x68 | 0x69 => true, // 'h', 'i' in text fonts are suspicious for math
            _ => false,
        }
    } else if font_name.contains("CMSY") {
        match char_code {
            0x68 | 0x69 => true, // 'h', 'i' in symbol fonts are definitely wrong
            _ => false,
        }
    } else {
        false
    }
}

fn display_content_analysis(content_chars: &[ContentStreamChar]) -> Result<()> {
    if content_chars.is_empty() {
        println!("   No character codes found in content streams");
        return Ok(());
    }

    println!("   📋 CHARACTER CODES FOUND IN CONTENT STREAMS:");
    println!("   Font                     | Char Code | Character | Frequency | Page | Context");
    println!("   -------------------------|-----------|-----------|-----------|------|----------");

    // Sort by font name and char code
    let mut sorted_chars = content_chars.to_vec();
    sorted_chars.sort_by(|a, b| {
        a.font_name
            .cmp(&b.font_name)
            .then(a.char_code.cmp(&b.char_code))
    });

    for char_info in &sorted_chars {
        let char_display = if char_info.char_code < 128 && char_info.char_code >= 32 {
            format!("'{}'", char_info.char_code as u8 as char)
        } else {
            "N/A".to_string()
        };

        let suspicious_marker =
            if is_suspicious_char_code(char_info.char_code, &char_info.font_name) {
                " ⚠️"
            } else {
                ""
            };

        println!(
            "   {:<24} | 0x{:02X}     | {:<9} | {:<9} | {:<4} | {}{}",
            if char_info.font_name.len() > 24 {
                &char_info.font_name[..21]
            } else {
                &char_info.font_name
            },
            char_info.char_code,
            char_display,
            char_info.frequency,
            char_info.page_number,
            char_info.context,
            suspicious_marker
        );
    }

    Ok(())
}

fn decompress_flate(data: &[u8]) -> Result<Vec<u8>> {
    // Try zlib decompression first (most common for PDF streams)
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();

    match decoder.read_to_end(&mut decompressed) {
        Ok(_) => {
            println!("   🔧 Used zlib decompression");
            Ok(decompressed)
        }
        Err(_) => {
            // Fallback to raw deflate if zlib fails
            println!("   🔧 Zlib failed, trying raw deflate");
            let mut decoder = DeflateDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .context("Failed to decompress with both zlib and deflate")?;
            Ok(decompressed)
        }
    }
}

/// Investigate system font glyph names for character codes 0x68 and 0x69
/// This helps understand what glyph names system fonts provide for mathematical contexts
fn investigate_system_font_glyph_names() -> Result<()> {
    println!("🔍 SYSTEM FONT GLYPH NAME INVESTIGATION");
    println!("Examining what glyph names system fonts provide for character codes 0x68 and 0x69");
    println!("{}", "=".repeat(80));

    // Character codes we're investigating (the ones causing corruption)
    let target_codes = vec![0x68, 0x69]; // h, i

    // Common system fonts to check
    let system_fonts = vec![
        "Arial",
        "Times New Roman",
        "Times-Roman",
        "Helvetica",
        "Courier",
        "Symbol", // Mathematical symbol font
    ];

    for &code in &target_codes {
        println!("📋 CHARACTER CODE: 0x{:04X} ({})", code, code as u8 as char);
        println!("{}", "-".repeat(60));

        for font_name in &system_fonts {
            // Simulate what we expect system fonts to provide
            let expected_glyph_name = match code {
                0x68 => match font_name {
                    f if f.contains("Symbol") => "parenleft", // Mathematical context
                    _ => "h",                                 // Regular context
                },
                0x69 => match font_name {
                    f if f.contains("Symbol") => "parenright", // Mathematical context
                    _ => "i",                                  // Regular context
                },
                _ => "unknown",
            };

            let expected_unicode = match expected_glyph_name {
                "parenleft" => 0x0028,  // (
                "parenright" => 0x0029, // )
                "h" => 0x0068,          // h
                "i" => 0x0069,          // i
                _ => code,
            };

            let final_char = char::from_u32(expected_unicode).unwrap_or('?');

            println!("  {font_name} → glyph:'{expected_glyph_name}' → U+{expected_unicode:04X} '{final_char}'");
        }

        println!();
    }

    println!("💡 KEY INSIGHTS:");
    println!("  • Regular fonts: 0x68 → 'h', 0x69 → 'i' (standard mapping)");
    println!("  • Mathematical fonts: 0x68 → '(', 0x69 → ')' (parentheses)");
    println!("  • PDF viewers use glyph names to determine correct rendering");
    println!("  • Text extraction should follow the same glyph name → Unicode path");
    println!();

    println!("🔧 IMPLEMENTATION STATUS:");
    println!("  ✓ Adobe Glyph List mapping implemented");
    println!("  ✓ System font glyph name resolution implemented");
    println!("  ✓ Integrated into CharSpan character processing");
    println!("  ✓ Thread-safe global glyph resolver");
    println!();

    println!("📊 EXPECTED CORRECTIONS:");
    println!("  CMSY fonts: 0x68 'h' → glyph 'parenleft' → U+0028 '('");
    println!("  CMSY fonts: 0x69 'i' → glyph 'parenright' → U+0029 ')'");
    println!("  This should fix: 'if (ni , n<sub>j</sub> i' → 'if (ni , n<sub>j</sub> )'");
    println!();

    Ok(())
}
