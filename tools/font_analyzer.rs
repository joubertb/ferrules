use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use ferrules_core::{
    correction_engine::{CorrectionConfigFile, CorrectionEngine, FontCorrectionConfig},
    diagnostic_logger::FontDiagnosticLogger,
    font_analysis::FontCorruptionDetector,
};
use pdfium_render::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "font-analyzer")]
#[command(about = "Analyze PDF fonts and generate corruption correction suggestions")]
#[command(version = "1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze a PDF file for font corruption patterns
    Analyze {
        /// Path to the PDF file to analyze
        #[arg(short, long)]
        pdf: PathBuf,

        /// Output file for the diagnostic report (JSON format)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Also analyze first N characters with pdfium-render for validation
        #[arg(short, long, default_value = "200")]
        chars: usize,

        /// Enable verbose logging
        #[arg(short, long)]
        verbose: bool,
    },
    /// Generate correction suggestions from analysis results
    Generate {
        /// Path to the diagnostic report JSON file
        #[arg(short, long)]
        report: PathBuf,

        /// Output file for generated corrections (JSON format)
        #[arg(short, long)]
        output: PathBuf,

        /// Minimum confidence threshold for suggestions (0.0-1.0)
        #[arg(short, long, default_value = "0.7")]
        confidence: f64,
    },
    /// Validate existing correction configuration against a PDF
    Validate {
        /// Path to the PDF file to validate against
        #[arg(short, long)]
        pdf: PathBuf,

        /// Path to the correction configuration file
        #[arg(short, long)]
        config: PathBuf,

        /// Number of characters to test correction accuracy
        #[arg(short, long, default_value = "500")]
        test_chars: usize,
    },
    /// Add font corrections to configuration file
    Add {
        /// Path to the correction configuration file
        #[arg(short, long)]
        config: PathBuf,

        /// Font name to add corrections for
        #[arg(short, long)]
        font: String,

        /// Character corrections in format "char:replacement,char:replacement"
        #[arg(short, long)]
        corrections: String,

        /// Confidence score for these corrections (0.0-1.0)
        #[arg(long, default_value = "0.8")]
        confidence: f64,

        /// Mark font as subset
        #[arg(long)]
        subset: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    if let Commands::Analyze { verbose, .. } = &cli.command {
        if *verbose {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .init();
        } else {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .init();
        }
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    match cli.command {
        Commands::Analyze {
            pdf,
            output,
            chars,
            verbose: _,
        } => {
            analyze_pdf_fonts(pdf, output, chars).await?;
        }
        Commands::Generate {
            report,
            output,
            confidence,
        } => {
            generate_corrections(report, output, confidence).await?;
        }
        Commands::Validate {
            pdf,
            config,
            test_chars,
        } => {
            validate_corrections(pdf, config, test_chars).await?;
        }
        Commands::Add {
            config,
            font,
            corrections,
            confidence,
            subset,
        } => {
            add_font_corrections(config, font, corrections, confidence, subset).await?;
        }
    }

    Ok(())
}

/// Analyze a PDF file for font corruption patterns
async fn analyze_pdf_fonts(
    pdf_path: PathBuf,
    output_path: Option<PathBuf>,
    char_limit: usize,
) -> Result<()> {
    info!("🔍 Analyzing PDF: {}", pdf_path.display());

    if !pdf_path.exists() {
        return Err(anyhow!("PDF file not found: {}", pdf_path.display()));
    }

    // Step 1: Analyze fonts using lopdf
    info!("📋 Analyzing font dictionaries with lopdf...");
    let mut lopdf_detector = FontCorruptionDetector::new(pdf_path.to_str().unwrap())?;
    let font_analyses = lopdf_detector.analyze_all_fonts()?;

    info!("✅ Found {} fonts in PDF", font_analyses.len());

    // Initialize diagnostic logger
    let document_name = pdf_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut diagnostic_logger = FontDiagnosticLogger::new(document_name);

    // Register all problematic fonts
    for analysis in &font_analyses {
        diagnostic_logger.register_problematic_font(
            &analysis.font_name,
            analysis.is_subset,
            analysis.has_tounicode,
        );
    }

    // Step 2: Extract and analyze text with pdfium-render
    info!("🔤 Analyzing text extraction with pdfium-render...");

    let pdfium = Pdfium::default();
    let document = pdfium.load_pdf_from_file(&pdf_path, None)?;

    let page = document.pages().get(0)?;
    let page_text = page.text()?;
    let chars = page_text.chars();

    info!(
        "📊 Processing {} characters from first page...",
        chars.len().min(char_limit)
    );

    let mut chars_processed = 0;
    for char_obj in chars.iter().take(char_limit) {
        if let Some(unicode_char) = char_obj.unicode_char() {
            let font_name = char_obj.font_name();
            let char_index = char_obj.index();

            // Get surrounding context
            let all_chars: Vec<_> = chars.iter().collect();
            let current_pos = all_chars
                .iter()
                .position(|c| c.index() == char_obj.index())
                .unwrap_or(0);

            let context_before = if current_pos >= 3 {
                all_chars[current_pos.saturating_sub(3)..current_pos]
                    .iter()
                    .filter_map(|c| c.unicode_char())
                    .collect::<String>()
            } else {
                String::new()
            };

            let context_after = if current_pos + 3 < all_chars.len() {
                all_chars[current_pos + 1..=(current_pos + 3).min(all_chars.len() - 1)]
                    .iter()
                    .filter_map(|c| c.unicode_char())
                    .collect::<String>()
            } else {
                String::new()
            };

            // Log suspicious characters
            match unicode_char {
                '(' | ')' | '[' | ']' | '{' | '}' | '\u{0}' | '\u{2}' => {
                    let (pos_x, pos_y) = if let Ok(bbox) = char_obj.tight_bounds() {
                        (bbox.left.value, bbox.bottom.value)
                    } else {
                        (0.0, 0.0)
                    };

                    diagnostic_logger.log_suspicious_character(
                        unicode_char,
                        char_index as u16,
                        &font_name,
                        &context_before,
                        &context_after,
                        0, // page number
                        pos_x as f64,
                        pos_y as f64,
                    );
                }
                _ => {}
            }

            chars_processed += 1;
        }
    }

    info!("✅ Processed {} characters", chars_processed);

    // Generate and save diagnostic report
    let report_path = output_path.unwrap_or_else(|| {
        let mut path = pdf_path.clone();
        path.set_extension("font_analysis.json");
        path
    });

    diagnostic_logger.save_report(&report_path)?;
    diagnostic_logger.print_summary();

    info!("💾 Diagnostic report saved to: {}", report_path.display());

    Ok(())
}

/// Generate correction suggestions from diagnostic report
async fn generate_corrections(
    report_path: PathBuf,
    output_path: PathBuf,
    confidence_threshold: f64,
) -> Result<()> {
    info!(
        "🛠️  Generating corrections from report: {}",
        report_path.display()
    );

    if !report_path.exists() {
        return Err(anyhow!("Report file not found: {}", report_path.display()));
    }

    // Load diagnostic report
    let report_content = std::fs::read_to_string(&report_path)?;
    let report: ferrules_core::diagnostic_logger::FontDiagnosticReport =
        serde_json::from_str(&report_content)?;

    let mut font_corrections = HashMap::new();
    let mut pattern_corrections = HashMap::new();

    // Process each problematic font
    for font_analysis in &report.problematic_fonts {
        if font_analysis.total_suspicious_chars > 0 {
            let mut corrections = HashMap::new();

            // Add high-confidence corrections
            for (char, suggestion) in &font_analysis.suggested_corrections {
                if let Some(&confidence) = font_analysis.confidence_scores.get(char) {
                    if confidence >= confidence_threshold {
                        corrections.insert(char.to_string(), suggestion.clone());
                        info!(
                            "✅ Added correction: '{}' -> '{}' (confidence: {:.2}) for font {}",
                            char, suggestion, confidence, font_analysis.font_name
                        );
                    }
                }
            }

            if !corrections.is_empty() {
                let font_config = FontCorrectionConfig {
                    description: format!(
                        "Auto-generated corrections for {} (subset: {}, tounicode: {})",
                        font_analysis.font_name,
                        font_analysis.is_subset,
                        font_analysis.has_tounicode
                    ),
                    corrections,
                    confidence: font_analysis
                        .confidence_scores
                        .values()
                        .copied()
                        .fold(0.0, f64::max), // Use highest confidence
                    enabled: true,
                    is_subset: font_analysis.is_subset,
                    has_tounicode: font_analysis.has_tounicode,
                };

                font_corrections.insert(font_analysis.font_name.clone(), font_config);
            }
        }
    }

    // Generate pattern corrections from common corruption evidence
    let mut pattern_frequency: HashMap<String, u32> = HashMap::new();

    for evidence in &report.corruption_evidence {
        if evidence.confidence_suspicious >= confidence_threshold {
            // Look for patterns like "t(e", "whic(", etc.
            let pattern = format!(
                "{}{}{}",
                evidence.context_before, evidence.character, evidence.context_after
            );

            // Generate likely corrections for common patterns
            if let Some(suggested_pattern) =
                generate_pattern_correction(&pattern, evidence.character)
            {
                *pattern_frequency
                    .entry(format!("{}:{}", pattern, suggested_pattern))
                    .or_insert(0) += 1;
            }
        }
    }

    // Add frequent patterns to corrections
    for (pattern_pair, frequency) in pattern_frequency {
        if frequency >= 2 {
            // Must appear at least twice to be included
            let parts: Vec<&str> = pattern_pair.split(':').collect();
            if parts.len() == 2 {
                pattern_corrections.insert(parts[0].to_string(), parts[1].to_string());
                info!(
                    "✅ Added pattern correction: '{}' -> '{}' (frequency: {})",
                    parts[0], parts[1], frequency
                );
            }
        }
    }

    // Create correction config file
    let correction_config = CorrectionConfigFile {
        version: "1.0.0".to_string(),
        description: format!(
            "Auto-generated corrections from analysis of {}",
            report.document_name
        ),
        last_updated: chrono::Utc::now().to_rfc3339(),
        font_corrections,
        pattern_corrections,
        settings: ferrules_core::correction_engine::CorrectionSettings {
            enable_font_corrections: true,
            enable_pattern_corrections: true,
            confidence_threshold: confidence_threshold,
            max_corrections_per_word: 3,
            enable_diagnostic_logging: true,
        },
    };

    // Save corrections
    let json = serde_json::to_string_pretty(&correction_config)?;
    std::fs::write(&output_path, json)?;

    info!(
        "💾 Generated corrections saved to: {}",
        output_path.display()
    );
    info!(
        "📊 Summary: {} font corrections, {} pattern corrections",
        correction_config.font_corrections.len(),
        correction_config.pattern_corrections.len()
    );

    Ok(())
}

/// Generate pattern correction suggestions
fn generate_pattern_correction(pattern: &str, suspicious_char: char) -> Option<String> {
    match suspicious_char {
        '(' => {
            if pattern.contains("t(e") {
                Some(pattern.replace('(', "h"))
            } else if pattern.contains("whic(") {
                Some(pattern.replace('(', "h"))
            } else if pattern.contains("(as") {
                Some(pattern.replace('(', "h"))
            } else {
                Some(pattern.replace('(', "h")) // Default
            }
        }
        ')' => Some(pattern.replace(')', "i")),
        '[' => Some(pattern.replace('[', "fi")),
        ']' => Some(pattern.replace(']', "fl")),
        '{' => Some(pattern.replace('{', "ff")),
        '}' => Some(pattern.replace('}', "ffi")),
        _ => None,
    }
}

/// Validate corrections against a PDF file
async fn validate_corrections(
    pdf_path: PathBuf,
    config_path: PathBuf,
    test_chars: usize,
) -> Result<()> {
    info!(
        "🧪 Validating corrections against PDF: {}",
        pdf_path.display()
    );

    if !pdf_path.exists() {
        return Err(anyhow!("PDF file not found: {}", pdf_path.display()));
    }

    if !config_path.exists() {
        return Err(anyhow!("Config file not found: {}", config_path.display()));
    }

    // Load correction engine
    let correction_engine = CorrectionEngine::new(&config_path)?;

    // Extract text with pdfium-render
    let pdfium = Pdfium::default();
    let document = pdfium.load_pdf_from_file(&pdf_path, None)?;
    let page = document.pages().get(0)?;
    let page_text = page.text()?;
    let chars = page_text.chars();

    let mut corrections_applied = 0;
    let mut total_chars = 0;

    info!(
        "🔍 Testing corrections on {} characters...",
        chars.len().min(test_chars)
    );

    for char_obj in chars.iter().take(test_chars) {
        if let Some(unicode_char) = char_obj.unicode_char() {
            let font_name = char_obj.font_name();
            let original_text = unicode_char.to_string();

            // Test font correction
            let corrected = correction_engine
                .apply_font_corrections(&original_text, &font_name)
                .await?;

            if corrected != original_text {
                corrections_applied += 1;
                info!(
                    "🔧 Correction applied: '{}' -> '{}' (font: {})",
                    original_text, corrected, font_name
                );
            }

            total_chars += 1;
        }
    }

    let correction_rate = if total_chars > 0 {
        (corrections_applied as f64 / total_chars as f64) * 100.0
    } else {
        0.0
    };

    // Get engine stats
    let stats = correction_engine.get_stats().await;

    info!("📊 Validation Results:");
    info!("  Total characters tested: {}", total_chars);
    info!("  Corrections applied: {}", corrections_applied);
    info!("  Correction rate: {:.2}%", correction_rate);
    info!("  Font corrections: {}", stats.font_corrections_applied);
    info!(
        "  Pattern corrections: {}",
        stats.pattern_corrections_applied
    );

    Ok(())
}

/// Add font corrections to configuration file
async fn add_font_corrections(
    config_path: PathBuf,
    font_name: String,
    corrections_str: String,
    confidence: f64,
    is_subset: bool,
) -> Result<()> {
    info!("➕ Adding font corrections for: {}", font_name);

    // Parse corrections string (format: "char:replacement,char:replacement")
    let mut corrections = HashMap::new();
    for pair in corrections_str.split(',') {
        let parts: Vec<&str> = pair.trim().split(':').collect();
        if parts.len() == 2 {
            corrections.insert(parts[0].to_string(), parts[1].to_string());
            info!("  '{}' -> '{}'", parts[0], parts[1]);
        } else {
            warn!("⚠️  Invalid correction format: {}", pair);
        }
    }

    if corrections.is_empty() {
        return Err(anyhow!("No valid corrections provided"));
    }

    // Load or create config file
    let mut config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str::<CorrectionConfigFile>(&content)?
    } else {
        CorrectionConfigFile {
            version: "1.0.0".to_string(),
            description: "Font correction configuration".to_string(),
            last_updated: chrono::Utc::now().to_rfc3339(),
            font_corrections: HashMap::new(),
            pattern_corrections: HashMap::new(),
            settings: ferrules_core::correction_engine::CorrectionSettings::default(),
        }
    };

    // Add font corrections
    let font_config = FontCorrectionConfig {
        description: format!(
            "Font corrections for {} (confidence: {:.2})",
            font_name, confidence
        ),
        corrections,
        confidence,
        enabled: true,
        is_subset,
        has_tounicode: false, // Assume false for manually added problematic fonts
    };

    config
        .font_corrections
        .insert(font_name.clone(), font_config);
    config.last_updated = chrono::Utc::now().to_rfc3339();

    // Save updated config
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(&config_path, json)?;

    info!("✅ Font corrections added to: {}", config_path.display());
    info!(
        "📊 Total fonts in config: {}",
        config.font_corrections.len()
    );

    Ok(())
}
