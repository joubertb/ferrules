use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// Information about a suspicious character or corruption pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorruptionEvidence {
    pub character: char,
    pub unicode_value: u32,
    pub char_code: u16,
    pub font_name: String,
    pub context_before: String,
    pub context_after: String,
    pub page_number: usize,
    pub position_x: f64,
    pub position_y: f64,
    pub confidence_suspicious: f64, // 0.0 to 1.0
}

/// Analysis of a problematic font discovered during processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblematicFontAnalysis {
    pub font_name: String,
    pub is_subset: bool,
    pub has_tounicode: bool,
    pub corruption_patterns: HashMap<char, Vec<String>>, // character -> contexts where it appears suspiciously
    pub frequency_analysis: HashMap<char, u32>,          // character -> occurrence count
    pub suggested_corrections: HashMap<char, String>,    // character -> suggested replacement
    pub confidence_scores: HashMap<char, f64>,           // character -> confidence of suggestion
    pub total_suspicious_chars: u32,
    pub pages_affected: Vec<usize>,
}

/// Complete diagnostic report for a PDF document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontDiagnosticReport {
    pub document_name: String,
    pub total_fonts: usize,
    pub problematic_fonts: Vec<ProblematicFontAnalysis>,
    pub corruption_evidence: Vec<CorruptionEvidence>,
    pub processing_stats: ProcessingStats,
    pub recommendations: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingStats {
    pub total_characters_processed: u64,
    pub total_suspicious_characters: u64,
    pub fonts_with_missing_tounicode: u32,
    pub subset_fonts_detected: u32,
    pub corruption_rate: f64, // percentage
}

/// Diagnostic logger for tracking font corruption patterns
pub struct FontDiagnosticLogger {
    evidence: Vec<CorruptionEvidence>,
    font_analyses: HashMap<String, ProblematicFontAnalysis>,
    processing_stats: ProcessingStats,
    document_name: String,
}

impl FontDiagnosticLogger {
    /// Create a new diagnostic logger for a document
    pub fn new(document_name: String) -> Self {
        info!(
            "🔍 Starting diagnostic logging for document: {}",
            document_name
        );

        Self {
            evidence: Vec::new(),
            font_analyses: HashMap::new(),
            processing_stats: ProcessingStats {
                total_characters_processed: 0,
                total_suspicious_characters: 0,
                fonts_with_missing_tounicode: 0,
                subset_fonts_detected: 0,
                corruption_rate: 0.0,
            },
            document_name,
        }
    }

    /// Log a suspicious character that might indicate corruption
    pub fn log_suspicious_character(
        &mut self,
        character: char,
        char_code: u16,
        font_name: &str,
        context_before: &str,
        context_after: &str,
        page_number: usize,
        position_x: f64,
        position_y: f64,
    ) {
        let confidence = self.calculate_suspicion_confidence(
            character,
            font_name,
            context_before,
            context_after,
        );

        if confidence > 0.5 {
            // Only log if reasonably suspicious
            let evidence = CorruptionEvidence {
                character,
                unicode_value: character as u32,
                char_code,
                font_name: font_name.to_string(),
                context_before: context_before.to_string(),
                context_after: context_after.to_string(),
                page_number,
                position_x,
                position_y,
                confidence_suspicious: confidence,
            };

            debug!("🚨 Suspicious character logged: '{}' (U+{:04X}) in font '{}' with confidence {:.2}", 
                   character, character as u32, font_name, confidence);

            self.evidence.push(evidence);
            self.processing_stats.total_suspicious_characters += 1;

            // Update font analysis
            self.update_font_analysis(
                font_name,
                character,
                context_before,
                context_after,
                page_number,
            );
        }

        self.processing_stats.total_characters_processed += 1;
    }

    /// Register a font as problematic based on lopdf analysis
    pub fn register_problematic_font(
        &mut self,
        font_name: &str,
        is_subset: bool,
        has_tounicode: bool,
    ) {
        if !has_tounicode {
            self.processing_stats.fonts_with_missing_tounicode += 1;
        }

        if is_subset {
            self.processing_stats.subset_fonts_detected += 1;
        }

        // Create or update font analysis entry
        let analysis = self
            .font_analyses
            .entry(font_name.to_string())
            .or_insert_with(|| {
                info!(
                    "📝 Registered problematic font: {} (subset: {}, tounicode: {})",
                    font_name, is_subset, has_tounicode
                );

                ProblematicFontAnalysis {
                    font_name: font_name.to_string(),
                    is_subset,
                    has_tounicode,
                    corruption_patterns: HashMap::new(),
                    frequency_analysis: HashMap::new(),
                    suggested_corrections: HashMap::new(),
                    confidence_scores: HashMap::new(),
                    total_suspicious_chars: 0,
                    pages_affected: Vec::new(),
                }
            });

        // Update existing analysis if already exists
        analysis.is_subset = is_subset;
        analysis.has_tounicode = has_tounicode;
    }

    /// Update font analysis with new corruption evidence
    fn update_font_analysis(
        &mut self,
        font_name: &str,
        character: char,
        context_before: &str,
        context_after: &str,
        page_number: usize,
    ) {
        let analysis = self
            .font_analyses
            .entry(font_name.to_string())
            .or_insert_with(|| {
                ProblematicFontAnalysis {
                    font_name: font_name.to_string(),
                    is_subset: font_name.contains('+'), // Heuristic detection
                    has_tounicode: false,               // Assume false for suspicious fonts
                    corruption_patterns: HashMap::new(),
                    frequency_analysis: HashMap::new(),
                    suggested_corrections: HashMap::new(),
                    confidence_scores: HashMap::new(),
                    total_suspicious_chars: 0,
                    pages_affected: Vec::new(),
                }
            });

        // Update frequency analysis
        *analysis.frequency_analysis.entry(character).or_insert(0) += 1;
        analysis.total_suspicious_chars += 1;

        // Add to corruption patterns
        let full_context = format!("{}{}{}", context_before, character, context_after);
        analysis
            .corruption_patterns
            .entry(character)
            .or_insert_with(Vec::new)
            .push(full_context);

        // Add page to affected pages if not already there
        if !analysis.pages_affected.contains(&page_number) {
            analysis.pages_affected.push(page_number);
        }

        // Generate suggestion based on known patterns
        if let Some(suggestion) =
            Self::suggest_correction_static(character, context_before, context_after)
        {
            let confidence = Self::calculate_suggestion_confidence_static(
                character,
                &suggestion,
                context_before,
                context_after,
            );
            analysis.suggested_corrections.insert(character, suggestion);
            analysis.confidence_scores.insert(character, confidence);
        }
    }

    /// Calculate how suspicious a character is based on context
    fn calculate_suspicion_confidence(
        &self,
        character: char,
        font_name: &str,
        context_before: &str,
        context_after: &str,
    ) -> f64 {
        let mut confidence: f64 = 0.0;

        // High suspicion for parentheses/brackets in regular text contexts
        match character {
            '(' | ')' => {
                // Very suspicious in word contexts
                if context_before
                    .chars()
                    .last()
                    .map_or(false, |c| c.is_alphabetic())
                    || context_after
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_alphabetic())
                {
                    confidence += 0.8;
                }
            }
            '[' | ']' | '{' | '}' => {
                // Suspicious in non-mathematical contexts
                if !context_before.contains("=") && !context_after.contains("=") {
                    confidence += 0.7;
                }
            }
            '\u{0}' | '\u{2}' => {
                // Null/control characters are almost always suspicious
                confidence += 0.9;
            }
            _ => {}
        }

        // Additional suspicion for subset fonts
        if font_name.contains('+') {
            confidence += 0.3;
        }

        // Boost confidence for known problematic font families
        if font_name.contains("NimbusRomNo9L")
            || font_name.contains("CMSY")
            || font_name.contains("CMMI")
        {
            confidence += 0.4;
        }

        confidence.min(1.0)
    }

    /// Suggest a correction for a suspicious character based on context
    fn suggest_correction_static(
        character: char,
        context_before: &str,
        context_after: &str,
    ) -> Option<String> {
        // Context-based suggestions
        let full_context = format!("{}{}{}", context_before, character, context_after);

        // Known pattern-based corrections
        match character {
            '(' => {
                if full_context.contains("t(e")
                    || context_before.ends_with("t") && context_after.starts_with("e")
                {
                    return Some("h".to_string());
                }
                if full_context.contains("whic(") {
                    return Some("h".to_string());
                }
                Some("h".to_string()) // Default for parentheses
            }
            ')' => Some("i".to_string()),   // Most common mapping
            '[' => Some("fi".to_string()),  // Ligature corruption
            ']' => Some("fl".to_string()),  // Ligature corruption
            '{' => Some("ff".to_string()),  // Mathematical font ligature
            '}' => Some("ffi".to_string()), // Mathematical font ligature
            _ => None,
        }
    }

    /// Calculate confidence in a suggested correction
    fn calculate_suggestion_confidence_static(
        character: char,
        suggestion: &str,
        context_before: &str,
        context_after: &str,
    ) -> f64 {
        let mut confidence: f64 = 0.5; // Base confidence

        // Increase confidence for well-known patterns
        let test_replacement = format!("{}{}{}", context_before, suggestion, context_after);

        // Check if replacement forms common English words
        if test_replacement.contains("the")
            || test_replacement.contains("which")
            || test_replacement.contains("given")
            || test_replacement.contains("with")
        {
            confidence += 0.4;
        }

        // High confidence for ligature corrections
        if matches!(character, '[' | ']' | '{' | '}') {
            confidence += 0.3;
        }

        confidence.min(1.0)
    }

    /// Generate a comprehensive diagnostic report
    pub fn generate_report(&mut self) -> FontDiagnosticReport {
        // Calculate corruption rate
        self.processing_stats.corruption_rate =
            if self.processing_stats.total_characters_processed > 0 {
                (self.processing_stats.total_suspicious_characters as f64
                    / self.processing_stats.total_characters_processed as f64)
                    * 100.0
            } else {
                0.0
            };

        // Generate recommendations
        let recommendations = self.generate_recommendations();

        info!("📊 Generated diagnostic report: {} suspicious characters out of {} total ({:.2}% corruption rate)",
              self.processing_stats.total_suspicious_characters,
              self.processing_stats.total_characters_processed,
              self.processing_stats.corruption_rate);

        FontDiagnosticReport {
            document_name: self.document_name.clone(),
            total_fonts: self.font_analyses.len(),
            problematic_fonts: self.font_analyses.values().cloned().collect(),
            corruption_evidence: self.evidence.clone(),
            processing_stats: self.processing_stats.clone(),
            recommendations,
            generated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Generate actionable recommendations based on analysis
    fn generate_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        // High corruption rate recommendations
        if self.processing_stats.corruption_rate > 10.0 {
            recommendations.push("High corruption rate detected. Consider using OCR as an alternative extraction method.".to_string());
        }

        // Font-specific recommendations
        for analysis in self.font_analyses.values() {
            if analysis.total_suspicious_chars > 10 {
                recommendations.push(format!("Font '{}' shows significant corruption patterns. Consider adding specific corrections for this font.", analysis.font_name));
            }

            if !analysis.has_tounicode && analysis.is_subset {
                recommendations.push(format!("Font '{}' is a subset font without ToUnicode mapping - primary cause of corruption.", analysis.font_name));
            }
        }

        // Pattern-based recommendations
        if self.processing_stats.subset_fonts_detected > 5 {
            recommendations.push("Multiple subset fonts detected. This document likely has systematic encoding issues.".to_string());
        }

        recommendations
    }

    /// Save diagnostic report to file
    pub fn save_report<P: AsRef<Path>>(&mut self, output_path: P) -> Result<()> {
        let report = self.generate_report();
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(output_path.as_ref(), json)?;

        info!(
            "💾 Diagnostic report saved to: {}",
            output_path.as_ref().display()
        );
        Ok(())
    }

    /// Print a summary of findings to console
    pub fn print_summary(&mut self) {
        let report = self.generate_report();

        println!("\n📊 FONT DIAGNOSTIC SUMMARY");
        println!("==========================");
        println!("Document: {}", report.document_name);
        println!("Total fonts analyzed: {}", report.total_fonts);
        println!(
            "Characters processed: {}",
            report.processing_stats.total_characters_processed
        );
        println!(
            "Suspicious characters: {}",
            report.processing_stats.total_suspicious_characters
        );
        println!(
            "Corruption rate: {:.2}%",
            report.processing_stats.corruption_rate
        );
        println!(
            "Subset fonts: {}",
            report.processing_stats.subset_fonts_detected
        );
        println!(
            "Fonts missing ToUnicode: {}",
            report.processing_stats.fonts_with_missing_tounicode
        );

        println!("\n🔍 PROBLEMATIC FONTS:");
        for font in &report.problematic_fonts {
            println!(
                "  {} (suspicious chars: {}, pages affected: {})",
                font.font_name,
                font.total_suspicious_chars,
                font.pages_affected.len()
            );

            for (char, suggestion) in &font.suggested_corrections {
                let confidence = font.confidence_scores.get(char).unwrap_or(&0.0);
                println!(
                    "    '{}' -> '{}' (confidence: {:.2})",
                    char, suggestion, confidence
                );
            }
        }

        println!("\n💡 RECOMMENDATIONS:");
        for (i, rec) in report.recommendations.iter().enumerate() {
            println!("  {}. {}", i + 1, rec);
        }
    }
}
