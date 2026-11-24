//! Locating the browser parity fixtures.
//!
//! The fixtures (`proc-*.png`, `proc-source.png`) are generated from the
//! upstream web app and are not distributed with this repository. Point
//! `FORM_FIXTURES` at a directory that holds them, or keep them in
//! `reference/processed` next to a checkout that has the reference corpus.

use std::path::PathBuf;

/// Directory holding `proc-metal.png`, `proc-heat.png` and `proc-source.png`.
pub fn fixture_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FORM_FIXTURES") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reference/processed")
}

/// Path to one fixture map, with a hint when it is missing.
pub fn map_path(name: &str) -> PathBuf {
    let path = fixture_dir().join(name);
    if !path.exists() {
        eprintln!(
            "missing fixture {} — set FORM_FIXTURES to the directory with the\n\
             upstream maps (see docs/PARITY.md)",
            path.display()
        );
    }
    path
}
