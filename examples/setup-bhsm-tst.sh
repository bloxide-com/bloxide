#!/bin/bash
set -e

echo "=== BhsmTst Interactive Example Setup ==="

# 1. Scaffold base crates
cargo blox new bhsm-tst

# 2. Add message variants one by one
for ev in A B C D E F G H I K X; do
    cargo blox add-message bhsm-tst-messages "$ev"
done

# 3. Remove template flat states
cargo blox remove-state bhsm-tst Ready
cargo blox remove-state bhsm-tst Done

# 4. Build deep hierarchy
cargo blox add-state bhsm-tst S  --composite
cargo blox add-state bhsm-tst S1 --parent S --composite
cargo blox add-state bhsm-tst S11 --parent S1
cargo blox add-state bhsm-tst S2 --parent S --composite
cargo blox add-state bhsm-tst S21 --parent S2 --composite
cargo blox add-state bhsm-tst S211 --parent S21
cargo blox add-state bhsm-tst Error
cargo blox add-state bhsm-tst Done

# 5. Generate enums and topology
cargo blox generate

echo "=== Setup complete. Now manually edit: ==="
echo "  - crates/bloxes/bhsm-tst/src/spec.rs (transitions)"
echo "  - examples/bhsm-tst-demo.rs (binary)"
echo "  - spec/bloxes/bhsm.md (documentation)"
echo "Run: cargo run --example bhsm-tst-demo"
