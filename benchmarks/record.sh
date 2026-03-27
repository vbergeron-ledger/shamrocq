#!/usr/bin/env bash
# Run integration tests with the stats feature and append results to results.jsonl.
# Each test that calls print_stats/record_stats emits one JSON line per invocation.
#
# Usage:
#   ./benchmarks/record.sh
#   ./benchmarks/record.sh --results-file /path/to/custom.jsonl
#
# Environment variables respected by the test harness:
#   BENCHMARK_FILE      – path to the JSONL results file (set by this script)
#   BENCHMARK_COMMIT    – git commit hash (set by this script)
#   BENCHMARK_TIMESTAMP – ISO-8601 UTC timestamp (set by this script)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULTS_FILE="$SCRIPT_DIR/results.jsonl"

# Parse optional --results-file argument
while [[ $# -gt 0 ]]; do
    case "$1" in
        --results-file)
            RESULTS_FILE="$2"; shift 2 ;;
        *)
            echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

COMMIT=$(git -C "$REPO_ROOT" rev-parse HEAD)
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

echo "Recording benchmarks"
echo "  commit    : $COMMIT"
echo "  timestamp : $TIMESTAMP"
echo "  output    : $RESULTS_FILE"
echo ""

BENCHMARK_FILE="$RESULTS_FILE" \
BENCHMARK_COMMIT="$COMMIT" \
BENCHMARK_TIMESTAMP="$TIMESTAMP" \
    cargo test \
        --manifest-path "$REPO_ROOT/Cargo.toml" \
        --package shamrocq \
        --features integration,stats \
        -- --test-threads=1 2>&1

echo ""
echo "Done. Results appended to $RESULTS_FILE"
