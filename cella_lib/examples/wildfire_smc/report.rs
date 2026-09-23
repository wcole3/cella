//! The bit every mode's report shares: a `(binary_git, binary_built_utc)`
//! provenance stamp so two report files can be told apart, and the
//! create-parent-dirs-then-write-pretty-JSON boilerplate every mode ended
//! with verbatim.

use std::path::Path;

use serde::Serialize;

/// Which build produced a report, so two runs can be told apart: the short
/// git commit hash `wildfire_smc` was compiled from (`"unknown"` if `git`
/// wasn't available at build time) and the UTC timestamp it was compiled
/// at. Every report struct carries these two fields under the names
/// `binary_git`/`binary_built_utc`.
pub(crate) fn provenance() -> (String, String) {
    (
        env!("CELLA_GIT_SHA").to_string(),
        env!("CELLA_BUILT_UTC").to_string(),
    )
}

/// Create `out`'s parent directory if needed and write `report` to it as
/// pretty JSON. Every mode ends this way.
pub(crate) fn write_json<T: Serialize>(out: &Path, report: &T) {
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, serde_json::to_string_pretty(report).unwrap()).unwrap();
}
