use anyhow::Result;
use fuzzy_matcher::skim::SkimMatcherV2;
use moka::future::Cache;
use once_cell::sync::OnceCell;
use spellbook::Dictionary;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Duration;
use tracing::info;

// Thread-local spell checker - each thread gets its own Dictionary instance
thread_local! {
    static SPELL_CHECKER: RefCell<Option<Dictionary>> = const { RefCell::new(None) };
    static FUZZY_MATCHER: RefCell<SkimMatcherV2> = RefCell::new(SkimMatcherV2::default());
}

/// Global common words checker for basic words not in main dictionary
static COMMON_WORDS: OnceCell<HashSet<String>> = OnceCell::new();

/// Global correction cache for thread-safe concurrent access
static CORRECTION_CACHE: OnceCell<Cache<String, String>> = OnceCell::new();

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
    config: SmartCorrectionConfig,
}

impl SmartCorrector {
    /// Create a new SmartCorrector instance
    pub fn new(config: SmartCorrectionConfig) -> Result<Self> {
        // Initialize global resources if not already done
        Self::init_common_words()?;
        Self::init_cache(&config)?;

        info!("✅ SmartCorrector initialized");

        Ok(Self { config })
    }

    /// Initialize the thread-local spell checker for the current thread
    fn init_thread_dictionary() -> Result<()> {
        SPELL_CHECKER.with(|checker| -> Result<()> {
            let mut checker = checker.borrow_mut();
            if checker.is_none() {
                // Use comprehensive Hunspell English dictionary from LibreOffice
                let aff = include_str!("dictionaries/en_US.aff");
                let dic = include_str!("dictionaries/en_US.dic");
                let dict = Dictionary::new(aff, dic)
                    .map_err(|e| anyhow::anyhow!("Failed to create dictionary: {:?}", e))?;
                *checker = Some(dict);
            }
            Ok(())
        })
    }

    /// Initialize the global common words list (called once)
    fn init_common_words() -> Result<()> {
        COMMON_WORDS.get_or_try_init(|| -> Result<HashSet<String>, anyhow::Error> {
            let common_dic_content = include_str!("dictionaries/common.dic");
            let mut words = HashSet::new();

            // Parse common.dic format (first line is count, then words)
            let lines: Vec<&str> = common_dic_content.trim().lines().collect();
            if lines.is_empty() {
                return Ok(words);
            }

            // Skip first line (word count) and parse word entries
            for line in lines.iter().skip(1) {
                let word = line.split_whitespace().next().unwrap_or("").trim();
                if !word.is_empty() {
                    words.insert(word.to_lowercase());
                }
            }

            info!("📝 Loaded {} common words from common.dic", words.len());
            Ok(words)
        })?;
        Ok(())
    }

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

    /// Check if a word is in the dictionary (checks both main and common dictionaries)
    fn is_valid_word(word: &str) -> bool {
        // First check thread-local Hunspell dictionary
        let is_valid_in_main = SPELL_CHECKER.with(|checker| -> bool {
            let checker = checker.borrow();
            if let Some(ref dict) = *checker {
                dict.check(word)
            } else {
                false
            }
        });

        if is_valid_in_main {
            return true;
        }

        // Fallback to common words dictionary
        if let Some(common_words) = COMMON_WORDS.get() {
            if common_words.contains(&word.to_lowercase()) {
                return true;
            }
        }

        false
    }

    /// Check if word contains likely corruption characters
    fn contains_corruption_chars(word: &str) -> bool {
        word.chars().any(|c| matches!(c, ')' | '(' | '\u{0002}'))
            || word.contains("ff")
            || word.contains("ffi")
            || word.chars().any(|c| c.is_control())
    }

    /// Check if parenthetical usage is legitimate (not corruption)
    pub fn is_legitimate_parenthetical(_word: &str) -> bool {
        false // No longer needed since we don't do blind character replacements
    }

    /// Determine if a word should be processed for correction
    fn should_correct(word: &str) -> bool {
        // Skip if already valid
        if Self::is_valid_word(word) {
            return false;
        }

        // Skip legitimate parenthetical usage
        if Self::is_legitimate_parenthetical(word) {
            return false;
        }

        // Only process if contains corruption characters
        Self::contains_corruption_chars(word)
    }

    /// Apply character-level substitutions based on known corruption patterns
    fn apply_character_substitutions(word: &str) -> Vec<String> {
        let mut candidates = vec![word.to_string()];

        // Known character corruptions from font analysis
        let substitutions = [
            ('\u{0002}', 'i'), // Control character conversion
        ];

        for (corrupt_char, correct_char) in &substitutions {
            if word.contains(*corrupt_char) {
                let corrected = word.replace(*corrupt_char, &correct_char.to_string());
                candidates.push(corrected);
            }
        }

        // Handle ligature corruptions
        if word.contains("ff") {
            candidates.push(word.replace("ff", "{"));
        }
        if word.contains("ffi") {
            candidates.push(word.replace("ffi", "}"));
        }

        candidates
    }

    /// Perform fuzzy matching using spell checker suggestions
    fn fuzzy_correct(_word: &str, _threshold: i64) -> Option<String> {
        // TODO: Implement proper fuzzy matching with dictionary suggestions
        // For now, we'll rely on character-level corrections
        // Future enhancement: implement edit distance and fuzzy matching
        None
    }

    /// Correct a single word using the smart correction pipeline
    pub async fn correct_word(&self, word: &str) -> Option<Cow<'_, str>> {
        // Ensure thread-local dictionary is initialized
        if let Err(_e) = Self::init_thread_dictionary() {
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

        // Try character-level corrections first
        let candidates = Self::apply_character_substitutions(word);
        for candidate in &candidates {
            let is_valid = Self::is_valid_word(candidate);
            if is_valid {
                // Cache the result
                if let Some(cache) = CORRECTION_CACHE.get() {
                    cache.insert(word.to_string(), candidate.clone()).await;
                }

                return Some(Cow::Owned(candidate.clone()));
            }
        }

        // Try fuzzy matching as fallback
        if let Some(fuzzy_match) = Self::fuzzy_correct(word, self.config.fuzzy_match_threshold) {
            // Cache the result
            if let Some(cache) = CORRECTION_CACHE.get() {
                cache.insert(word.to_string(), fuzzy_match.clone()).await;
            }

            return Some(Cow::Owned(fuzzy_match));
        }

        None
    }

    /// Correct all words in a text string
    pub async fn correct_text(&self, text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut corrected_words = Vec::with_capacity(words.len());

        for word in words {
            match self.correct_word(word).await {
                Some(corrected) => corrected_words.push(corrected.into_owned()),
                None => corrected_words.push(word.to_string()),
            }
        }

        corrected_words.join(" ")
    }

    /// Get cache statistics for monitoring
    pub async fn get_cache_stats(&self) -> Option<u64> {
        if let Some(cache) = CORRECTION_CACHE.get() {
            let entry_count = cache.entry_count();
            Some(entry_count)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_corruption_detection() {
        // Initialize thread-local dictionary for testing
        let _ = SmartCorrector::init_thread_dictionary();

        assert!(SmartCorrector::should_correct("be)tween"));
        assert!(SmartCorrector::should_correct("w)th"));
        assert!(SmartCorrector::should_correct("whic("));

        // Should not correct legitimate usage
        assert!(!SmartCorrector::should_correct("(NLP)"));
        assert!(!SmartCorrector::should_correct("(2018)"));
        assert!(!SmartCorrector::should_correct("between")); // Valid word
    }

    #[tokio::test]
    async fn test_character_substitutions() {
        let candidates = SmartCorrector::apply_character_substitutions("be)tween");
        assert!(candidates.contains(&"beitween".to_string())); // ) -> i

        let candidates = SmartCorrector::apply_character_substitutions("whic(");
        assert!(candidates.contains(&"which".to_string())); // ( -> h
    }

    #[tokio::test]
    async fn test_word_correction() {
        let config = SmartCorrectionConfig::default();
        let corrector = SmartCorrector::new(config).unwrap();

        // Test character-level correction with a word that becomes valid after substitution
        // "w)th" -> "with" (valid word)
        let result = corrector.correct_word("w)th").await;
        assert!(result.is_some()); // Should find "with"

        // Test no correction needed
        let result = corrector.correct_word("between").await;
        assert_eq!(result, None);

        // Test legitimate parenthetical
        let result = corrector.correct_word("(NLP)").await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_thread_local_dictionary_concurrent_access() {
        use std::sync::Arc;
        use std::thread;
        use tokio::runtime::Runtime;

        let config = Arc::new(SmartCorrectionConfig::default());
        let mut handles = vec![];

        // Create multiple threads that each use the smart corrector
        for i in 0..3 {
            let config_clone = config.clone();
            let handle = thread::spawn(move || {
                let rt = Runtime::new().unwrap();
                rt.block_on(async {
                    let corrector = SmartCorrector::new((*config_clone).clone()).unwrap();

                    // Each thread should have its own dictionary instance
                    let result = corrector.correct_word("w)th").await;
                    println!("Thread {} result: {:?}", i, result);

                    assert!(result.is_some(), "Thread {} should find correction", i);
                });
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
