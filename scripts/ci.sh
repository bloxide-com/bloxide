# Copyright 2025 Bloxide, all rights reserved
#!/usr/bin/env bash
# Run CI checks locally, directly on the host machine.
# This executes the same commands as .github/workflows/lint-and-test.yml,
# but without Docker overhead. Much faster than act for routine pre-PR checks.
#
# Usage:
#   ./scripts/ci.sh              # run all checks
#   ./scripts/ci.sh lint         # run only lint (build, check, fmt, clippy)
#   ./scripts/ci.sh test         # run only tests
#   ./scripts/ci.sh copyright    # run only copyright check
#   ./scripts/ci.sh docs         # run only docs build
#
# Requirements:
#   - stable Rust toolchain (rustup, cargo, rustc)
#   - clippy component installed
#   - riscv32imc-unknown-none-elf target installed (for embassy checks)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

run_step() {
    local name="$1"
    shift
    echo ""
    echo "========================================"
    echo "  ${name}"
    echo "========================================"
    if "$@"; then
        echo -e "${GREEN}✓ ${name} passed${NC}"
    else
        echo -e "${RED}✗ ${name} failed${NC}"
        exit 1
    fi
}

check_copyright() {
    INCORRECT_COPYRIGHT=$(find . \( -name '*.rs' -o -name 'Cargo.toml' \) -not -path '*/target/*' \
        | xargs grep -LE '(#|//) Copyright 202[0-9] Bloxide, all rights reserved' || true)
    if [ -n "$INCORRECT_COPYRIGHT" ]; then
        echo "Incorrect copyright notice found:"
        echo "$INCORRECT_COPYRIGHT" | tr ' ' '\n'
        return 1
    fi
    echo "All source files have correct copyright notices."
}

run_lint() {
    run_step "Cargo Build (default features)" cargo build

    run_step "Cargo Check (bloxide-core no-default-features)" \
        cargo check -p bloxide-core --no-default-features

    run_step "Cargo Check (bloxide-core alloc)" \
        cargo check -p bloxide-core --no-default-features --features alloc

    run_step "Cargo Check (bloxide-core std)" \
        cargo check -p bloxide-core --features std

    run_step "Cargo Check (bloxide-timer riscv32imc)" \
        cargo check -p bloxide-timer --target riscv32imc-unknown-none-elf

    run_step "Cargo Check (ping-pong-messages no-default-features)" \
        cargo check -p ping-pong-messages --no-default-features

    run_step "Cargo Format Check" cargo fmt -- --check

    run_step "Cargo Clippy" cargo clippy --all-targets -- -W warnings -D warnings
}

run_tests() {
    run_step "Cargo Test (bloxide-core std)" \
        cargo test -p bloxide-core --features std

    run_step "Cargo Test (workspace default)" \
        cargo test
}

run_docs() {
    run_step "Cargo Doc Build" \
        sh -c 'RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps'
}

# Parse arguments
MODE="${1:-all}"

case "$MODE" in
    all)
        run_step "Copyright Compliance" check_copyright
        run_lint
        run_tests
        run_docs
        echo ""
        echo "========================================"
        echo -e "${GREEN}All CI checks passed!${NC}"
        echo "========================================"
        ;;
    lint)
        run_lint
        ;;
    test)
        run_tests
        ;;
    copyright)
        run_step "Copyright Compliance" check_copyright
        ;;
    docs)
        run_docs
        ;;
    *)
        echo "Usage: $0 [all|lint|test|copyright|docs]"
        exit 1
        ;;
esac
