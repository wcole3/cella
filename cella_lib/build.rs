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
//! or the command fails for any other reason, we fall back to `"unknown"`
//! rather than erroring out.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The short commit hash of `HEAD`, or `"unknown"` if `git` can't answer
/// (not installed, not a git checkout, detached weirdness, etc).
fn git_short_sha() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
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
    println!("cargo:rustc-env=CELLA_GIT_SHA={}", git_short_sha());
    println!("cargo:rustc-env=CELLA_BUILT_UTC={}", built_utc());
}
