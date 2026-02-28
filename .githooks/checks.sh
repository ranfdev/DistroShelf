#!/usr/bin/env sh
set -eu

echo "Running pre-commit checks..."
echo "• cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "• cargo clippy --all-targets --all-features"
cargo clippy --all-targets --all-features -- -Dwarnings

echo "• cargo test --all-features"
cargo test --all-features
