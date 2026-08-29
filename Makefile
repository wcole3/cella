.PHONY: all build build-release run run-gui check clippy fmt fmt-check doc clean \
		test test-all test-all-single test-all-single-run \
		coverage coverage-all test-create-snapshots test-update-benchmarks

# ── Default ───────────────────────────────────────────────────────────────────
all: build

# ── Build ─────────────────────────────────────────────────────────────────────

## Debug build
build:
	cargo build --package cella --bin cella

## Release build
build-release:
	cargo build --package cella --bin cella --release

# ── Run ───────────────────────────────────────────────────────────────────────

## Run the application (CLI / default mode)
run:
	cargo run --package cella --bin cella

## Run the application with the GUI at 1024x768
run-gui:
	cargo run --package cella --bin cella -- --gui --size 1024x768

# ── Code quality ──────────────────────────────────────────────────────────────

## Check compilation without producing binaries
check:
	cargo check --workspace

## Run Clippy lints across the workspace
clippy:
	cargo clippy --workspace -- -D warnings

## Format all source files
fmt:
	cargo fmt --all

## Check formatting without modifying files
fmt-check:
	cargo fmt --all -- --check

# ── Documentation ─────────────────────────────────────────────────────────────

## Build and open the documentation
doc:
	cargo doc --workspace --no-deps --open

# ── Tests ─────────────────────────────────────────────────────────────────────

## Quick test run — only non-ignored tests (fast feedback)
test:
	cd cella_lib && CELLA_ASCII=0 cargo test --package cella_lib

## Run all tests including ignored ones (multi-threaded)
test-all:
	cd cella_lib && CELLA_ASCII=0 cargo test --package cella_lib -- --include-ignored

## Run all tests including ignored ones, single-threaded
test-all-single:
	cd cella_lib && cargo test --package cella_lib -- --include-ignored --test-threads=1

## Run all tests single-threaded with bench runs=1 and config export enabled
test-all-single-run:
	cd cella_lib && CELLA_BENCH_RUNS=1 CELLA_EXPORT_CONFIGS=1 cargo test --package cella_lib -- --include-ignored --test-threads=1

## Generate line coverage for cella_lib (non-ignored tests)
coverage:
	cd cella_lib && CELLA_ASCII=0 cargo llvm-cov --package cella_lib --html

## Generate line coverage for cella_lib including ignored tests
coverage-all:
	cd cella_lib && CELLA_ASCII=0 cargo llvm-cov --package cella_lib --html -- --include-ignored

## Regenerate / update snapshot files
test-create-snapshots:
	cd cella_lib && CELLA_UPDATE_SNAPSHOTS=1 cargo test --package cella_lib -- --include-ignored

## Update benchmark baseline files
test-update-benchmarks:
	cd cella_lib && CELLA_UPDATE_BENCH=1 cargo test --package cella_lib -- --include-ignored --test-threads=1

# ── Clean ─────────────────────────────────────────────────────────────────────

## Remove build artifacts
clean:
	cargo clean
