//! Shared by the manifest suites.

use std::path::PathBuf;

/// This crate's directory as cargo reports it when it runs the test, falling
/// back to the compile-time value. Cargo can reuse a test binary built from
/// another checkout, or from this clone before it moved, with that path baked
/// into `env!`: the suites then loaded 0 manifests (2026-10-13 clone move;
/// 2026-10-17, a review checkout that shared this `target/`).
pub fn crate_dir() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from)
}
