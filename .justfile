# List all just recipes.
_help:
	just -l

# Run all tests with cargo-nextest
@test:
	cargo nextest run --workspace --all-features --future-incompat-report

# Build the cli for release
@release:
	cargo build --release --all

# Clippy and other lints
@lint:
	cargo clippy --all --all-features
