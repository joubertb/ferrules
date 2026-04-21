//! Shared CoreML execution-provider construction for ONNX sessions.
//!
//! All CoreML call sites (layout, table-standard, table-ANE) route through
//! [`build_coreml_provider`] to keep configuration consistent. The model
//! cache directory is the primary knob: without it CoreML recompiles every
//! ONNX graph to an `.mlmodelc` in `/tmp` on each process start.

use std::path::{Path, PathBuf};

use ort::execution_providers::{
    coreml::{CoreMLComputeUnits, CoreMLExecutionProvider},
    ExecutionProviderDispatch,
};

/// FNV-1a 64-bit hash of the embedded model bytes. Used to namespace the
/// cache directory so replacing the model invalidates the cache without
/// requiring a manual wipe. Stable across Rust versions (unlike
/// `DefaultHasher`), so cache keys remain valid across rebuilds.
pub fn fnv1a_hex8(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut h = OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    format!("{:016x}", h)[..8].to_string()
}

/// Build a per-model CoreML cache subdirectory under `root`. Creates it on
/// demand; returns `None` (and logs a warning) if creation fails, so callers
/// fall back to the "no cache" path rather than failing session creation.
pub fn per_model_cache_dir(root: &Path, key: &str) -> Option<PathBuf> {
    let dir = root.join(key);
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(e) => {
            tracing::warn!(
                "CoreML cache dir {:?} could not be created ({}); continuing without cache",
                dir,
                e
            );
            None
        }
    }
}

/// Construct a CoreML execution provider with consistent options across all
/// ONNX sessions in the crate.
///
/// - `ane_only`: when true, restrict compute units to CPU + Neural Engine
///   (replaces the removed `with_ane_only()` API in ort rc.10).
/// - `cache_dir`: per-model cache directory. Caller is responsible for
///   passing distinct paths for distinct ONNX graphs — sharing a directory
///   across different models will corrupt CoreML's cache.
pub fn build_coreml_provider(
    ane_only: bool,
    cache_dir: Option<&Path>,
) -> ExecutionProviderDispatch {
    let mut provider = CoreMLExecutionProvider::default();
    if ane_only {
        provider = provider.with_compute_units(CoreMLComputeUnits::CPUAndNeuralEngine);
    }
    if let Some(dir) = cache_dir {
        provider = provider.with_model_cache_dir(dir.display().to_string());
    }
    provider.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_is_deterministic_and_differs_per_input() {
        assert_eq!(fnv1a_hex8(b"abc"), fnv1a_hex8(b"abc"));
        assert_ne!(fnv1a_hex8(b"abc"), fnv1a_hex8(b"abd"));
        assert_eq!(fnv1a_hex8(b"").len(), 8);
    }

    #[test]
    fn build_coreml_provider_does_not_panic() {
        // Black-box: we can't easily inspect the built provider, but we can
        // confirm every code path builds cleanly.
        let _ = build_coreml_provider(false, None);
        let _ = build_coreml_provider(true, None);
        let tmp = std::env::temp_dir().join("ferrules-coreml-test-cache");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = build_coreml_provider(true, Some(&tmp));
    }
}
