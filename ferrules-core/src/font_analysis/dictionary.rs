//! Dictionary-based word validation and correction system
//!
//! This module provides sophisticated word validation and correction using Hunspell
//! dictionaries with caching and fuzzy matching capabilities.

use crate::debug_print;

#[cfg(feature = "correction-engine")]
use {
    anyhow::Result,
    moka::future::Cache,
    once_cell::sync::OnceCell,
    spellbook::Dictionary,
    std::{borrow::Cow, cell::RefCell, time::Duration},
    tracing::info,
};

#[cfg(feature = "correction-engine")]
// Thread-local spell checker - each thread gets its own Dictionary instance
thread_local! {
    static SPELL_CHECKER: RefCell<Option<Dictionary>> = const { RefCell::new(None) };
    static CUSTOM_SPELL_CHECKER: RefCell<Option<Dictionary>> = const { RefCell::new(None) };
}

#[cfg(feature = "correction-engine")]
/// Global correction cache for thread-safe concurrent access
static CORRECTION_CACHE: OnceCell<Cache<String, String>> = OnceCell::new();

#[cfg(feature = "correction-engine")]
/// Global SmartCorrector instance for reuse across function calls
static GLOBAL_SMART_CORRECTOR: OnceCell<SmartCorrector> = OnceCell::new();

#[cfg(feature = "correction-engine")]
/// Flag to prevent repeated dictionary initialization logging
static DICTIONARY_INIT_LOGGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Configuration for smart correction behavior
#[derive(Debug, Clone)]
pub struct SmartCorrectionConfig {
    pub confidence_threshold: f64,
    pub cache_size: u64,
    pub cache_ttl_seconds: u64,
    pub fuzzy_match_threshold: i64,
}

impl Default for SmartCorrectionConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.7,
            cache_size: 10_000,
            cache_ttl_seconds: 3600,
            fuzzy_match_threshold: 50,
        }
    }
}

/// Lightweight smart corrector with no owned data
/// Safe to clone and use across multiple threads
#[derive(Debug, Clone)]
pub struct SmartCorrector {
    #[allow(dead_code)]
    config: SmartCorrectionConfig,
}

impl SmartCorrector {
    /// Get or create the global SmartCorrector instance (recommended for efficiency)
    #[cfg(feature = "correction-engine")]
    pub fn global() -> Result<&'static SmartCorrector> {
        GLOBAL_SMART_CORRECTOR.get_or_try_init(|| {
            let config = SmartCorrectionConfig::default();
            Self::init_cache(&config)?;
            Self::init_thread_dictionary()?;
            info!("✅ SmartCorrector initialized (global instance)");
            Ok(SmartCorrector { config })
        })
    }

    /// Create a new SmartCorrector instance (use global() instead for better performance)
    pub fn new(config: SmartCorrectionConfig) -> Result<Self> {
        #[cfg(feature = "correction-engine")]
        {
            Self::init_cache(&config)?;
            // Initialize thread-local dictionary to ensure it works
            Self::init_thread_dictionary()?;
            info!("✅ SmartCorrector initialized (new instance)");
        }

        Ok(Self { config })
    }

    #[cfg(feature = "correction-engine")]
    /// Initialize the thread-local spell checker for the current thread
    fn init_thread_dictionary() -> Result<()> {
        SPELL_CHECKER.with(|checker| -> Result<()> {
            let mut checker = checker.borrow_mut();
            if checker.is_none() {
                // Use comprehensive Hunspell English dictionary
                let aff = include_str!("../dictionaries/en_US.aff");
                let dic = include_str!("../dictionaries/en_US.dic");
                let dict = Dictionary::new(aff, dic)
                    .map_err(|e| anyhow::anyhow!("Failed to create dictionary: {:?}", e))?;
                *checker = Some(dict);
            }
            Ok(())
        })?;

        // Initialize custom dictionary
        CUSTOM_SPELL_CHECKER.with(|checker| -> Result<()> {
            let mut checker = checker.borrow_mut();
            if checker.is_none() {
                // Load custom technical dictionary
                let custom_aff = include_str!("../dictionaries/custom.aff");
                let custom_dic = include_str!("../dictionaries/custom.dic");
                let custom_dict = Dictionary::new(custom_aff, custom_dic)
                    .map_err(|e| anyhow::anyhow!("Failed to create custom dictionary: {:?}", e))?;
                *checker = Some(custom_dict);

                // Only log initialization once across all threads to reduce log spam
                if !DICTIONARY_INIT_LOGGED.load(std::sync::atomic::Ordering::Relaxed) {
                    DICTIONARY_INIT_LOGGED.store(true, std::sync::atomic::Ordering::Relaxed);
                    info!("📚 Custom Spellbook dictionary initialized");
                }
            }
            Ok(())
        })?;

        Ok(())
    }

    #[cfg(feature = "correction-engine")]
    /// Initialize the global correction cache (called once)
    fn init_cache(config: &SmartCorrectionConfig) -> Result<()> {
        CORRECTION_CACHE.get_or_try_init(|| -> Result<Cache<String, String>, anyhow::Error> {
            let cache = Cache::builder()
                .max_capacity(config.cache_size)
                .time_to_live(Duration::from_secs(config.cache_ttl_seconds))
                .build();
            info!(
                "🗄️ Initialized correction cache with capacity: {}",
                config.cache_size
            );
            Ok(cache)
        })?;
        Ok(())
    }

    /// Strip HTML tags from text for validation
    fn strip_html_tags(text: &str) -> String {
        // Simple regex-like removal of HTML tags
        let mut result = String::new();
        let mut in_tag = false;

        for ch in text.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => result.push(ch),
                _ => {} // Skip characters inside tags
            }
        }
        result
    }

    /// Check if a word is in the dictionary
    pub fn is_valid_word(word: &str) -> bool {
        #[cfg(feature = "correction-engine")]
        {
            // Strip HTML tags before validation
            let clean_word = Self::strip_html_tags(word);

            // Check custom Spellbook dictionary
            let is_valid_in_custom = CUSTOM_SPELL_CHECKER.with(|checker| -> bool {
                let checker = checker.borrow();
                if let Some(ref dict) = *checker {
                    dict.check(&clean_word.to_lowercase())
                } else {
                    false
                }
            });

            if is_valid_in_custom {
                return true;
            }

            // Check thread-local main Hunspell dictionary
            let is_valid_in_main = SPELL_CHECKER.with(|checker| -> bool {
                let checker = checker.borrow();
                if let Some(ref dict) = *checker {
                    dict.check(&clean_word.to_lowercase())
                } else {
                    false
                }
            });

            if is_valid_in_main {
                return true;
            }
        }

        false
    }

    #[cfg(not(feature = "correction-engine"))]
    /// Fallback word validation when correction engine is disabled
    fn is_likely_valid_word_fallback(word: &str) -> bool {
        // Basic heuristics for word validation without dictionary
        let clean_word = word.trim_matches(|c: char| !c.is_alphabetic());

        // Too short or empty
        if clean_word.len() < 2 {
            return false;
        }

        // Contains obvious corruption markers
        if clean_word
            .chars()
            .any(|c| matches!(c, ')' | '(' | '\u{0002}'))
        {
            return false;
        }

        // Basic technical terms (hardcoded fallback)
        let basic_technical_terms = [
            "unicode",
            "utf",
            "ascii",
            "json",
            "xml",
            "html",
            "css",
            "api",
            "url",
            "uri",
            "http",
            "https",
            "pdf",
            "csv",
            "sql",
            "regex",
            "llm",
            "ai",
            "ml",
            "nlp",
            "bert",
            "gpt",
            "sdk",
            "gui",
            "cli",
            "ide",
            "github",
            "gitlab",
            "microsoft",
            "google",
        ];

        let lower_word = clean_word.to_lowercase();
        if basic_technical_terms.contains(&lower_word.as_str()) {
            return true;
        }

        // Assume reasonable length words without obvious corruption are valid
        clean_word.len() >= 3 && clean_word.chars().all(|c| c.is_alphabetic())
    }

    /// Check if word contains likely corruption characters
    fn contains_corruption_chars(word: &str) -> bool {
        word.chars().any(|c| matches!(c, ')' | '(' | '\u{0002}'))
            || word.contains("ff")
            || word.contains("ffi")
            || word.chars().any(|c| c.is_control())
    }

    /// Check if a word is a reasonable compound word even if not in dictionary
    /// This handles technical terms, product names, and hyphenated compounds
    fn is_reasonable_compound_word(word: &str) -> bool {
        // Check for hyphenated compounds like "AI-driven"
        if word.contains('-') {
            let parts: Vec<&str> = word.split('-').collect();
            if parts.len() == 2 {
                let (left, right) = (parts[0], parts[1]);
                // Accept if both parts look like reasonable words/acronyms
                if Self::is_reasonable_word_part(left) && Self::is_reasonable_word_part(right) {
                    return true;
                }
            }
        }

        // Check for spaced compounds like "LLM Guard"
        if word.contains(' ') {
            let parts: Vec<&str> = word.split_whitespace().collect();
            if parts.len() == 2 {
                let (left, right) = (parts[0], parts[1]);
                // Accept if both parts look reasonable
                if Self::is_reasonable_word_part(left) && Self::is_reasonable_word_part(right) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a word part is reasonable (for compound word validation)
    fn is_reasonable_word_part(part: &str) -> bool {
        if part.len() < 2 {
            return false;
        }

        // Check if it's a valid dictionary word
        if Self::is_valid_word(part) {
            return true;
        }

        // Check if it's a reasonable acronym (all uppercase, 2-4 letters)
        if part.len() <= 4 && part.chars().all(|c| c.is_ascii_uppercase()) {
            return true;
        }

        // Check if it's a reasonable technical term starting with uppercase
        if part.len() >= 3 && part.chars().next().unwrap().is_ascii_uppercase() {
            let lowercase_part = part.to_lowercase();
            // Common technical word endings
            if lowercase_part.ends_with("guard")
                || lowercase_part.ends_with("driven")
                || lowercase_part.ends_with("based")
                || lowercase_part.ends_with("aware")
            {
                return true;
            }
        }

        false
    }

    /// Determine if a word should be processed for correction
    fn should_correct(word: &str) -> bool {
        // Skip if already valid
        let is_valid = Self::is_valid_word(word);
        if is_valid {
            return false;
        }

        // Process if contains obvious corruption characters
        if Self::contains_corruption_chars(word) {
            return true;
        }

        // For insertion patterns, be more conservative:
        // Only process if word is invalid AND contains clear corruption indicators
        let insertion_patterns = ["fi", "fl", "ff", "ti", "te", "st"];
        let has_insertion_pattern = insertion_patterns
            .iter()
            .any(|&pattern| word.contains(pattern));

        if has_insertion_pattern {
            // Additional check: only process if the word looks corrupted
            // (not just any word containing these common letter combinations)

            // Check if removing any insertion pattern creates a more valid-looking word
            for pattern in &insertion_patterns {
                if word.contains(pattern) {
                    // Try removing single occurrence of pattern (not all occurrences)
                    let without_first_pattern = word.replacen(pattern, "", 1);
                    if without_first_pattern.len() >= 3
                        && Self::is_valid_word(&without_first_pattern)
                    {
                        return true; // Likely corruption if removing pattern makes it valid
                    }

                    // Also try removing all occurrences (original behavior)
                    let without_all_patterns = word.replace(pattern, "");
                    if without_all_patterns.len() >= 3 && Self::is_valid_word(&without_all_patterns)
                    {
                        return true; // Likely corruption if removing all patterns makes it valid
                    }

                    // Also check if replacing with hyphen/space creates reasonable compound
                    if *pattern == "fi" && word.len() > 6 {
                        let with_hyphen = word.replace(pattern, "-");
                        let with_space = word.replace(pattern, " ");

                        if Self::is_reasonable_compound_word(&with_hyphen)
                            || Self::is_reasonable_compound_word(&with_space)
                        {
                            return true; // Likely corrupted compound word
                        }
                    }
                }
            }

            // Don't correct words that just happen to contain these patterns
            // unless we have evidence they're corrupted
            return false;
        }

        false
    }

    /// Get Hunspell suggestion for a word (first suggestion only)
    fn get_hunspell_suggestion(word: &str) -> Option<String> {
        #[cfg(feature = "correction-engine")]
        {
            // Try custom dictionary first
            let custom_suggestion = CUSTOM_SPELL_CHECKER.with(|checker| -> Option<String> {
                let checker = checker.borrow();
                if let Some(ref dict) = *checker {
                    let mut suggestions = Vec::new();
                    dict.suggest(word, &mut suggestions);
                    // Return first suggestion if any
                    suggestions.first().cloned()
                } else {
                    None
                }
            });

            if custom_suggestion.is_some() {
                return custom_suggestion;
            }

            // Fall back to main dictionary
            SPELL_CHECKER.with(|checker| -> Option<String> {
                let checker = checker.borrow();
                if let Some(ref dict) = *checker {
                    let mut suggestions = Vec::new();
                    dict.suggest(word, &mut suggestions);
                    // Return first suggestion if any
                    suggestions.first().cloned()
                } else {
                    None
                }
            })
        }

        #[cfg(not(feature = "correction-engine"))]
        {
            None
        }
    }

    /// Apply character-level substitutions based on known corruption patterns
    fn apply_character_substitutions(word: &str) -> Vec<String> {
        let mut candidates = vec![word.to_string()];

        // Common ligature-like insertion patterns
        let insertion_patterns = [
            "fi", "fl", "ff", // Common ligature-like insertions
            "ti", "te", "st", // Common character pairs that get inserted
            "ifi", "ifl", "iff", // Longer ligature patterns
        ];

        for pattern in &insertion_patterns {
            if word.contains(pattern) {
                // Try removing single occurrence of pattern (for words like "Classifification")
                let without_first = word.replacen(pattern, "", 1);
                if without_first.len() >= 3 {
                    candidates.push(without_first);
                }

                // Try removing all occurrences of pattern (original behavior)
                let without_pattern = word.replace(pattern, "");
                if without_pattern.len() >= 3 {
                    candidates.push(without_pattern);
                }

                // For compound words, try adding hyphens and spaces
                if *pattern == "fi" && word.len() > 6 {
                    let with_hyphen = word.replace(pattern, "-");
                    candidates.push(with_hyphen);
                    let with_space = word.replace(pattern, " ");
                    candidates.push(with_space);
                }

                // Try replacing pattern with single letters
                for replacement in &["s", "t", "e", "m", "l", "-", " "] {
                    let with_replacement = word.replace(pattern, replacement);
                    candidates.push(with_replacement);
                }
            }
        }

        // Special handling for repeated letter patterns (like "ification" in "Classification")
        if word.contains("ification") {
            candidates.push(word.replace("ification", "ication"));
        }

        candidates
    }

    /// Correct a single word using the smart correction pipeline (synchronous)
    pub fn correct_word_sync(&self, word: &str) -> Option<Cow<'_, str>> {
        #[cfg(feature = "correction-engine")]
        {
            // Ensure thread-local dictionary is initialized
            if let Err(e) = Self::init_thread_dictionary() {
                eprintln!("❌ Dictionary initialization failed: {:?}", e);
                return None;
            }

            // Early return if no correction needed
            if !Self::should_correct(word) {
                return None;
            }

            // Try character-level corrections
            let candidates = Self::apply_character_substitutions(word);
            for candidate in &candidates {
                if Self::is_valid_word(candidate) {
                    return Some(Cow::Owned(candidate.clone()));
                }

                // Check if it's a reasonable compound word (like "LLM Guard" or "AI-driven")
                if Self::is_reasonable_compound_word(candidate) {
                    return Some(Cow::Owned(candidate.clone()));
                }
            }

            // Fallback: Try Hunspell suggestions
            let hunspell_suggestion = Self::get_hunspell_suggestion(word);
            if let Some(suggestion) = hunspell_suggestion {
                return Some(Cow::Owned(suggestion));
            }
        }

        #[cfg(not(feature = "correction-engine"))]
        {
            // Fallback without dictionary
            if Self::is_likely_valid_word_fallback(word) {
                return None; // Already valid
            }

            // Simple pattern-based correction
            let candidates = Self::apply_character_substitutions(word);
            for candidate in &candidates {
                if Self::is_likely_valid_word_fallback(candidate) {
                    return Some(Cow::Owned(candidate.clone()));
                }
            }
        }

        None
    }

    /// Correct a single word using the smart correction pipeline (async)
    pub async fn correct_word(&self, word: &str) -> Option<Cow<'_, str>> {
        #[cfg(feature = "correction-engine")]
        {
            // Ensure thread-local dictionary is initialized
            if let Err(e) = Self::init_thread_dictionary() {
                eprintln!("❌ Dictionary initialization failed: {:?}", e);
                return None;
            }

            // Early return if no correction needed
            if !Self::should_correct(word) {
                return None;
            }

            // Check cache first
            if let Some(cache) = CORRECTION_CACHE.get() {
                if let Some(cached) = cache.get(word).await {
                    return Some(Cow::Owned(cached));
                }
            }

            // Try character-level corrections
            let candidates = Self::apply_character_substitutions(word);
            for candidate in &candidates {
                if Self::is_valid_word(candidate) {
                    // Cache the result
                    if let Some(cache) = CORRECTION_CACHE.get() {
                        cache.insert(word.to_string(), candidate.clone()).await;
                    }
                    return Some(Cow::Owned(candidate.clone()));
                }

                // Check if it's a reasonable compound word (like "LLM Guard" or "AI-driven")
                if Self::is_reasonable_compound_word(candidate) {
                    return Some(Cow::Owned(candidate.clone()));
                }
            }

            // Fallback: Try Hunspell suggestions
            let hunspell_suggestion = Self::get_hunspell_suggestion(word);
            if let Some(suggestion) = hunspell_suggestion {
                // Cache the result
                if let Some(cache) = CORRECTION_CACHE.get() {
                    cache.insert(word.to_string(), suggestion.clone()).await;
                }
                return Some(Cow::Owned(suggestion));
            }
        }

        #[cfg(not(feature = "correction-engine"))]
        {
            // Fallback without dictionary
            if Self::is_likely_valid_word_fallback(word) {
                return None; // Already valid
            }

            // Simple pattern-based correction
            let candidates = Self::apply_character_substitutions(word);
            for candidate in &candidates {
                if Self::is_likely_valid_word_fallback(candidate) {
                    return Some(Cow::Owned(candidate.clone()));
                }
            }
        }

        None
    }

    /// Correct all words in a text string (synchronous)
    pub fn correct_text_sync(&self, text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut corrected_words = Vec::with_capacity(words.len());

        for word in words {
            // Handle HTML tags specially - extract clean text but preserve tag structure
            let clean_word_for_checking = Self::strip_html_tags(word);

            // If the stripped word is different from the original, we have HTML tags
            if clean_word_for_checking != word && !clean_word_for_checking.is_empty() {
                // Process only the text inside HTML tags
                match self.correct_word_sync(&clean_word_for_checking) {
                    Some(corrected) => {
                        // Replace the text inside HTML tags with corrected version
                        let corrected_html =
                            word.replace(&clean_word_for_checking, corrected.as_ref());
                        corrected_words.push(corrected_html);
                    }
                    None => corrected_words.push(word.to_string()),
                }
            } else {
                // No HTML tags - use original logic
                // Skip correction for words containing em-dashes to prevent "differences—such" → "differences"
                if word.contains('—') || word.contains('–') {
                    corrected_words.push(word.to_string());
                    continue;
                }

                let clean_word = word.trim_matches(|c: char| !c.is_alphabetic());
                let prefix = &word
                    [..word.len() - word.trim_start_matches(|c: char| !c.is_alphabetic()).len()];
                let suffix = &word[prefix.len() + clean_word.len()..];

                match self.correct_word_sync(clean_word) {
                    Some(corrected) => {
                        let full_corrected = format!("{}{}{}", prefix, corrected.as_ref(), suffix);
                        corrected_words.push(full_corrected);
                    }
                    None => corrected_words.push(word.to_string()),
                }
            }
        }

        corrected_words.join(" ")
    }

    /// Correct all words in a text string (async)
    pub async fn correct_text(&self, text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut corrected_words = Vec::with_capacity(words.len());

        for word in words {
            // Extract the actual word without punctuation
            let clean_word = word.trim_matches(|c: char| !c.is_alphabetic());
            let prefix =
                &word[..word.len() - word.trim_start_matches(|c: char| !c.is_alphabetic()).len()];
            let suffix = &word[prefix.len() + clean_word.len()..];

            match self.correct_word(clean_word).await {
                Some(corrected) => {
                    let full_corrected = format!("{}{}{}", prefix, corrected.as_ref(), suffix);
                    corrected_words.push(full_corrected);
                }
                None => corrected_words.push(word.to_string()),
            }
        }

        corrected_words.join(" ")
    }

    #[cfg(feature = "correction-engine")]
    /// Get cache statistics for monitoring
    pub async fn get_cache_stats(&self) -> Option<u64> {
        CORRECTION_CACHE.get().map(|cache| cache.entry_count())
    }

    #[cfg(not(feature = "correction-engine"))]
    /// Get cache statistics for monitoring (fallback)
    pub async fn get_cache_stats(&self) -> Option<u64> {
        None
    }

    /// Apply dictionary corrections to spans directly (modifies span text in place)
    pub fn correct_spans_sync(&self, spans: &mut [crate::entities::CharSpan]) {
        for span in spans.iter_mut() {
            let corrected_text = self.correct_text_sync(&span.text);
            if corrected_text != span.text {
                debug_print!(
                    "🔧 SPAN CORRECTION: '{}' → '{}' (Font: {})",
                    span.text,
                    corrected_text,
                    span.font_name
                );
                span.text = corrected_text;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_correction_sync() {
        let config = SmartCorrectionConfig::default();
        let corrector = SmartCorrector::new(config).unwrap();

        // Test specific case
        let result = corrector.correct_word_sync("sysfitems");
        assert!(result.is_some(), "Expected 'sysfitems' to be corrected");

        if let Some(corrected) = result {
            assert_eq!(corrected.as_ref(), "systems");
        }

        // Test no correction needed
        let result = corrector.correct_word_sync("systems");
        assert_eq!(result, None);

        // Test the exact text from jailbreak.pdf
        let text = "Through testing against six prominent protection sysfitems, including Microsoft's Azure Prompt Shield";
        let result = corrector.correct_text_sync(text);
        println!("Input: {}", text);
        println!("Output: {}", result);

        // Debug individual word
        let word_result = corrector.correct_word_sync("sysfitems");
        println!("Word 'sysfitems' corrects to: {:?}", word_result);

        if !result.contains("systems") {
            panic!(
                "Expected 'sysfitems' to be corrected to 'systems' in full text. Got: {}",
                result
            );
        }

        // Test the new problematic words
        println!("\n--- Testing new problematic words ---");
        let test_words = [
            "AIfidriven",
            "Classifification",
            "LLMfiGuard",
            "unicode",
            "Unicode",
            "Vijil",
            "Vifijil",
        ];
        for word in &test_words {
            let result = corrector.correct_word_sync(word);
            println!("Word '{}' corrects to: {:?}", word, result);

            // Check if the word should be corrected
            println!(
                "  - Is '{}' valid? {}",
                word,
                SmartCorrector::is_valid_word(word)
            );
            println!(
                "  - Should correct '{}'? {}",
                word,
                SmartCorrector::should_correct(word)
            );

            // Debug candidates for LLMfiGuard
            if *word == "LLMfiGuard" {
                let candidates = SmartCorrector::apply_character_substitutions(word);
                println!("  - Candidates: {:?}", candidates);
                for candidate in &candidates {
                    println!(
                        "    '{}' is valid? {}",
                        candidate,
                        SmartCorrector::is_valid_word(candidate)
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn test_word_correction() {
        let config = SmartCorrectionConfig::default();
        let corrector = SmartCorrector::new(config).unwrap();

        // Test specific case
        let result = corrector.correct_word("sysfitems").await;
        assert!(result.is_some());

        if let Some(corrected) = result {
            assert_eq!(corrected.as_ref(), "systems");
        }

        // Test no correction needed
        let result = corrector.correct_word("systems").await;
        assert_eq!(result, None);
    }

    #[test]
    fn test_word_validation() {
        // Test common words
        assert!(SmartCorrector::is_valid_word("systems"));
        assert!(SmartCorrector::is_valid_word("information"));
        assert!(SmartCorrector::is_valid_word("the"));

        // Test invalid words
        assert!(!SmartCorrector::is_valid_word("sysfitems"));
    }
}
