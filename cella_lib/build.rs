//! Stamps every build of this crate with two facts that let a validation
//! report be traced back to the exact binary that produced it: which git
//! commit it was built from, and when. A previous round of experiments ran
//! two different binaries back to back and had no way to tell their reports
//! apart afterwards — this closes that gap.
//!
//! Cargo auto-detects `build.rs` at the package root and runs it before
//! compiling the crate, so nothing needs registering in `Cargo.toml`. The two
//! `cargo:rustc-env=...` lines below set environment variables that
//! `env!("CELLA_GIT_SHA")` / `env!("CELLA_BUILT_UTC")` read at compile time
//! elsewhere in the crate (see `examples/wildfire_smc.rs`).
//!
//! Provenance is a nice-to-have, not something that should ever block a
//! build: if `git` is missing, this checkout has no `.git` (e.g. a tarball),
//! or a command fails for any other reason, we fall back to `"unknown"`
//! rather than erroring out.
//!
//! **Why the `cargo:rerun-if-changed` lines matter**: by default Cargo only
//! re-runs a build script when a file *inside this package* changes, and a
//! `git commit` touches no file in `cella_lib/` — so without an explicit
//! trigger, `binary_git` would silently go stale the moment you commit
//! (caught in review: it happened to the very commit that introduced this
//! file). We instead watch the specific git-internal files that change when
//! `HEAD` moves, plus `src/` and `examples/` so a source edit refreshes the
//! dirty-tree marker below on the next build. The one gap this doesn't
//! close: editing a tracked file *outside* `src/` or `examples/` (e.g.
//! `Cargo.toml`) without also touching one of those dirs won't by itself
//! trigger a rebuild, so a report from that build could under-report
//! dirtiness until something else forces a rebuild.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The absolute-or-relative path to this checkout's `.git` directory
/// (resolved via `git rev-parse --git-dir`, run from the crate root — git
/// walks up to find it even though `.git` actually lives at the repo root,
/// one level above `cella_lib/`), or `None` if `git` is missing or this
/// isn't a checkout at all.
fn git_dir() -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

/// Every file whose contents change whenever `HEAD` moves to a new commit:
/// the `HEAD` file itself, the branch ref it points at (when `HEAD` is a
/// symbolic ref — i.e. we're on a branch rather than detached), and
/// `packed-refs`, since a branch ref can live there instead of as a loose
/// file once `git gc` has packed it.
fn git_watch_paths(dir: &Path) -> Vec<PathBuf> {
    let head_file = dir.join("HEAD");
    let mut paths = vec![head_file.clone()];
    if let Ok(head) = std::fs::read_to_string(&head_file)
        && let Some(ref_path) = head.trim().strip_prefix("ref: ")
    {
        paths.push(dir.join(ref_path));
    }
    paths.push(dir.join("packed-refs"));
    paths
}

/// True when the working tree has staged or unstaged changes to tracked
/// files. Untracked files are excluded on purpose — a stray scratch file
/// sitting in the tree shouldn't make every report claim the binary itself
/// is dirty. `git status` looks at the whole repository regardless of the
/// directory it's run from, so running it from the crate root (this build
/// script's working directory) is equivalent to running it at the repo
/// root. Any failure (no git, not a repo) reads as "not dirty" — that's
/// already reflected in `binary_git` being `"unknown"`.
fn is_dirty() -> bool {
    Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| !output.stdout.is_empty())
        .unwrap_or(false)
}

/// The short commit hash of `HEAD`, or `"unknown"` if `git` can't answer
/// (not installed, not a git checkout, detached weirdness, etc), with a
/// `-dirty` suffix when the working tree has uncommitted changes to tracked
/// files — otherwise a clean build and a build made from edited-but-not-yet-
/// committed source would report the exact same sha.
fn git_short_sha() -> String {
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    if sha != "unknown" && is_dirty() {
        format!("{sha}-dirty")
    } else {
        sha
    }
}

/// The current time as an ISO-8601 UTC timestamp, e.g. `2026-09-11T12:34:56Z`.
///
/// Computed by hand from `SystemTime` instead of pulling in a date/time
/// crate: a build script only needs to print one timestamp once per build,
/// so a tiny calendar conversion is simpler than a new dependency.
fn built_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (hour, minute, second) = (
        time_of_day / 3600,
        (time_of_day / 60) % 60,
        time_of_day % 60,
    );
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days-since-1970-01-01 to a civil (year, month, day) date, UTC, proleptic
/// Gregorian calendar. This is Howard Hinnant's well-known integer-only
/// `civil_from_days` algorithm — exact for any day since the calendar has no
/// gaps, and needs no floating point or leap-year special-casing.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // day of era: [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era: [0, 399]
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year: [0, 365]
    let mp = (5 * doy + 2) / 153; // month, shifted so the year starts in March: [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn main() {
    // Re-run this script (and so refresh CELLA_GIT_SHA) whenever HEAD moves
    // to a new commit. Silently skip this when git is unavailable: without
    // it, CELLA_GIT_SHA is already pinned at "unknown" and there is nothing
    // ref-related to watch.
    if let Some(dir) = git_dir() {
        for path in git_watch_paths(&dir) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    // Re-run on any source edit too, so the dirty-tree marker in
    // CELLA_GIT_SHA reflects the latest change on the next build.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=examples");

    println!("cargo:rustc-env=CELLA_GIT_SHA={}", git_short_sha());
    println!("cargo:rustc-env=CELLA_BUILT_UTC={}", built_utc());
}
