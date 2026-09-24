#!/usr/bin/env bash
# Copyright 2025 Bloxide, all rights reserved
# Run the repository CI matrix using cargo-blox from this checkout.
# Requirements: stable Rust with rustfmt, clippy and riscv32imc-unknown-none-elf.
# Install cargo-deny to include the optional local dependency audit.
# The feature, coverage and round-trip workflow jobs remain separate checks.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}/.."

if (( $# != 0 )); then
    echo "Usage: $0" >&2
    echo "For individual checks, use cargo run -p cargo-blox -- blox <command>." >&2
    exit 1
fi

# Build materializes the generated workspace before the check matrix runs.
cargo run -p cargo-blox -- blox build
cargo run -p cargo-blox -- blox ci
