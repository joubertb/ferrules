use bitflags::bitflags;
use std::cell::RefCell;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

bitflags! {
    /// Debug output routing flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DebugOutput: u8 {
        const NONE = 0b00;
        const STDERR = 0b01;
        const FILE = 0b10;
    }
}

impl Default for DebugOutput {
    fn default() -> Self {
        DebugOutput::NONE
    }
}

impl std::str::FromStr for DebugOutput {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(DebugOutput::NONE),
            "stderr" => Ok(DebugOutput::STDERR),
            "file" => Ok(DebugOutput::FILE),
            "both" => Ok(DebugOutput::STDERR | DebugOutput::FILE),
            _ => Err(format!(
                "Invalid debug output option: '{s}'. Valid options are: none, stderr, file, both"
            )),
        }
    }
}

/// Debug context for a specific document parsing session
#[derive(Debug, Clone)]
pub struct DebugContext {
    pub doc_name: String,
    pub output_flags: DebugOutput,
    pub debug_dir: PathBuf,
}

/// Global configuration for debug output
static DEBUG_CONFIG: OnceLock<DebugConfig> = OnceLock::new();

#[derive(Debug, Clone)]
struct DebugConfig {
    pub default_output: DebugOutput,
    pub default_dir: PathBuf,
}

impl DebugConfig {
    fn new() -> Self {
        let debug_dir = std::env::var("FERRULES_DEBUG_DIR")
            .unwrap_or_else(|_| "/tmp/ferrules-debug".to_string());

        Self {
            default_output: DebugOutput::NONE,
            default_dir: PathBuf::from(debug_dir),
        }
    }
}

thread_local! {
    static DEBUG_CONTEXT: RefCell<Option<DebugContext>> = const { RefCell::new(None) };
}

/// Initialize global debug configuration
pub fn init_debug_config(default_output: DebugOutput) {
    let mut config = DebugConfig::new();
    config.default_output = default_output;
    DEBUG_CONFIG.set(config).ok();
}

/// Set debug context for current thread
pub fn set_debug_context(doc_name: String, output_flags: Option<DebugOutput>) {
    let config = DEBUG_CONFIG.get_or_init(DebugConfig::new);
    let flags = output_flags.unwrap_or(config.default_output);

    let context = DebugContext {
        doc_name,
        output_flags: flags,
        debug_dir: config.default_dir.clone(),
    };

    // Create debug directory if needed and file output is enabled
    if flags.contains(DebugOutput::FILE) {
        if let Err(e) = create_dir_all(&context.debug_dir) {
            eprintln!(
                "Failed to create debug directory {:?}: {}",
                context.debug_dir, e
            );
        }
    }

    DEBUG_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(context);
    });
}

/// Clear debug context for current thread
pub fn clear_debug_context() {
    DEBUG_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });
}

/// Get current debug context
pub fn get_debug_context() -> Option<DebugContext> {
    DEBUG_CONTEXT.with(|ctx| ctx.borrow().clone())
}

/// Get current debug file path for a doc_name
pub fn get_debug_file_path(doc_name: &str) -> Result<PathBuf, String> {
    let config = DEBUG_CONFIG.get_or_init(DebugConfig::new);
    Ok(config.default_dir.join(format!("{doc_name}-debug.txt")))
}

/// Write debug output according to current context
pub fn write_debug(message: &str) {
    DEBUG_CONTEXT.with(|ctx| {
        if let Some(context) = ctx.borrow().as_ref() {
            // Write to stderr if enabled
            if context.output_flags.contains(DebugOutput::STDERR) {
                eprintln!("{message}");
            }

            // Write to file if enabled
            if context.output_flags.contains(DebugOutput::FILE) {
                let debug_file_path = context
                    .debug_dir
                    .join(format!("{}-debug.txt", context.doc_name));

                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&debug_file_path)
                {
                    if let Err(e) = writeln!(file, "{message}") {
                        eprintln!("Failed to write to debug file {debug_file_path:?}: {e}");
                    }
                } else {
                    eprintln!("Failed to open debug file {debug_file_path:?}");
                }
            }
        }
        // If no context is set, do nothing (silent mode)
    });
}

/// Clean up debug files older than the specified duration
pub fn cleanup_old_debug_files(max_age_hours: u64) -> Result<usize, String> {
    use std::fs;
    use std::time::{Duration, SystemTime};

    let config = DEBUG_CONFIG.get_or_init(DebugConfig::new);
    let debug_dir = &config.default_dir;

    if !debug_dir.exists() {
        return Ok(0);
    }

    let max_age = Duration::from_secs(max_age_hours * 3600);
    let cutoff_time = SystemTime::now()
        .checked_sub(max_age)
        .ok_or("Failed to calculate cutoff time")?;

    let entries =
        fs::read_dir(debug_dir).map_err(|e| format!("Failed to read debug directory: {e}"))?;

    let mut deleted_count = 0;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let path = entry.path();

        // Only process debug files (ending in -debug.txt)
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.ends_with("-debug.txt") {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified < cutoff_time {
                            if let Err(e) = fs::remove_file(&path) {
                                eprintln!("Failed to delete old debug file {path:?}: {e}");
                            } else {
                                deleted_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(deleted_count)
}

/// Delete debug file for specific doc_name
pub fn delete_debug_file(doc_name: &str) -> Result<(), String> {
    let debug_file_path = get_debug_file_path(doc_name)?;

    if debug_file_path.exists() {
        std::fs::remove_file(&debug_file_path)
            .map_err(|e| format!("Failed to delete debug file: {e}"))?;
    }

    Ok(())
}

/// Read debug file for specific doc_name
pub fn read_debug_file(doc_name: &str) -> Result<String, String> {
    let debug_file_path = get_debug_file_path(doc_name)?;

    if !debug_file_path.exists() {
        return Err("Debug file not found".to_string());
    }

    std::fs::read_to_string(&debug_file_path).map_err(|e| format!("Failed to read debug file: {e}"))
}

/// Macro for debug output - replaces eprintln!
#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        $crate::debug::write_debug(&format!($($arg)*))
    };
}

/// Macro for debug output with newline
#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        $crate::debug::write_debug(&format!($($arg)*))
    };
}
