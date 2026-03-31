#!/usr/bin/env bash
# Run integration tests with the stats feature and record results.
# Each benchmark (test binary) writes to its own JSONL file under benchmarks/.
#
# Usage:
#   ./benchmarks/record.sh                         # record all benchmarks
#   ./benchmarks/record.sh hash_forest             # record one benchmark
#   ./benchmarks/record.sh hash_forest synth_list  # record specific benchmarks
#
# Environment variables respected by the test harness:
#   BENCHMARK_FILE      – path to the JSONL results file (set per benchmark)
#   BENCHMARK_COMMIT    – git commit hash (set by this script)
#   BENCHMARK_TIMESTAMP – ISO-8601 UTC timestamp (set by this script)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMMIT=$(git -C "$REPO_ROOT" rev-parse HEAD)
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# If no arguments, discover all test binaries that call print_stats
if [[ $# -eq 0 ]]; then
    BENCHMARKS=()
    for f in "$REPO_ROOT"/crates/shamrocq/tests/*.rs; do
        name="$(basename "$f" .rs)"
        if grep -q 'print_stats' "$f"; then
            BENCHMARKS+=("$name")
        fi
    done
else
    BENCHMARKS=("$@")
fi

echo "Recording benchmarks"
echo "  commit    : $COMMIT"
echo "  timestamp : $TIMESTAMP"
echo "  benchmarks: ${BENCHMARKS[*]}"
echo ""

for bench in "${BENCHMARKS[@]}"; do
    RESULTS_FILE="$SCRIPT_DIR/$bench.jsonl"
    echo "── $bench → $RESULTS_FILE"

    BENCHMARK_FILE="$RESULTS_FILE" \
    BENCHMARK_COMMIT="$COMMIT" \
    BENCHMARK_TIMESTAMP="$TIMESTAMP" \
        cargo test \
            --manifest-path "$REPO_ROOT/Cargo.toml" \
            --package shamrocq \
            --features stats \
            --test "$bench" \
            -- --test-threads=1 2>&1
done

echo ""
echo "Done."
