#!/usr/bin/env bash
# Compare benchmark results across commits.
#
# Usage:
#   ./benchmarks/compare.sh                    # aggregate all commits
#   ./benchmarks/compare.sh 78f705d 4fe01ce    # compare two specific commits

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_FILE="${RESULTS_FILE:-$SCRIPT_DIR/results.jsonl}"

if [[ $# -ge 2 ]]; then
    FILTER="WHERE commit LIKE '${1}%' OR commit LIKE '${2}%'"
else
    FILTER=""
fi

if [[ $# -ge 2 ]]; then
    duckdb -c "
    WITH data AS (
      SELECT * FROM read_ndjson_auto('$RESULTS_FILE') $FILTER
    ),
    old AS (SELECT * FROM data WHERE commit LIKE '${1}%'),
    new AS (SELECT * FROM data WHERE commit LIKE '${2}%')
    SELECT
      COALESCE(o.test, n.test) AS test,
      o.exec_instruction_count AS old_insns,
      n.exec_instruction_count AS new_insns,
      n.exec_instruction_count - o.exec_instruction_count AS Δinsns,
      o.peak_heap_bytes AS old_heap,
      n.peak_heap_bytes AS new_heap,
      n.peak_heap_bytes - o.peak_heap_bytes AS Δheap,
      o.peak_stack_bytes AS old_stack,
      n.peak_stack_bytes AS new_stack,
      n.peak_stack_bytes - o.peak_stack_bytes AS Δstack,
      o.alloc_bytes_total AS old_alloc,
      n.alloc_bytes_total AS new_alloc,
      n.alloc_bytes_total - o.alloc_bytes_total AS Δalloc,
    FROM old o FULL OUTER JOIN new n USING (test)
    ORDER BY test;
    "
else
    duckdb -c "
    WITH data AS (
      SELECT * FROM read_ndjson_auto('$RESULTS_FILE')
    )
    SELECT
      commit[:7] AS commit,
      count(*) AS tests,
      sum(exec_instruction_count) AS Σinsns,
      sum(peak_heap_bytes) AS Σheap,
      sum(peak_stack_bytes) AS Σstack,
      sum(alloc_bytes_total) AS Σalloc,
      sum(exec_apply_count) AS Σapply,
      sum(exec_tail_apply_count) AS Σtail,
      sum(exec_match_count) AS Σmatch
    FROM data
    GROUP BY commit
    ORDER BY min(timestamp);
    "
fi
