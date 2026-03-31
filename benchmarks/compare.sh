#!/usr/bin/env bash
# Compare benchmark results across commits.
#
# Usage:
#   ./benchmarks/compare.sh *.jsonl                         # aggregate all commits across files
#   ./benchmarks/compare.sh *.jsonl -- 78f705d 4fe01ce      # compare two commits
#   ./benchmarks/compare.sh hash_forest.jsonl synth_list.jsonl -- 78f 4fe

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

FILES=()
COMMITS=()
PARSING_FILES=true

for arg in "$@"; do
    if [[ "$arg" == "--" ]]; then
        PARSING_FILES=false
        continue
    fi
    if $PARSING_FILES; then
        if [[ -f "$arg" ]]; then
            FILES+=("$arg")
        elif [[ -f "$SCRIPT_DIR/$arg" ]]; then
            FILES+=("$SCRIPT_DIR/$arg")
        elif [[ -f "$SCRIPT_DIR/$arg.jsonl" ]]; then
            FILES+=("$SCRIPT_DIR/$arg.jsonl")
        else
            echo "Warning: cannot find benchmark file for '$arg'" >&2
        fi
    else
        COMMITS+=("$arg")
    fi
done

if [[ ${#FILES[@]} -eq 0 ]]; then
    FILES=("$SCRIPT_DIR"/*.jsonl)
fi

GLOB_EXPR=$(printf "'%s'," "${FILES[@]}")
GLOB_EXPR="[${GLOB_EXPR%,}]"

if [[ ${#COMMITS[@]} -ge 2 ]]; then
    duckdb -c "
    WITH data AS (
      SELECT * FROM read_ndjson_auto($GLOB_EXPR)
      WHERE commit LIKE '${COMMITS[0]}%' OR commit LIKE '${COMMITS[1]}%'
    ),
    old AS (SELECT * FROM data WHERE commit LIKE '${COMMITS[0]}%'),
    new AS (SELECT * FROM data WHERE commit LIKE '${COMMITS[1]}%')
    SELECT
      COALESCE(o.test, n.test) AS test,
      o.exec_instruction_count AS old_insns,
      n.exec_instruction_count AS new_insns,
      n.exec_instruction_count - o.exec_instruction_count AS Δinsns,
      printf('%.1f%%', 100.0*(n.exec_instruction_count - o.exec_instruction_count)/o.exec_instruction_count) AS \"%insns\",
      o.peak_heap_bytes AS old_heap,
      n.peak_heap_bytes AS new_heap,
      n.peak_heap_bytes - o.peak_heap_bytes AS Δheap,
      o.alloc_bytes_total AS old_alloc,
      n.alloc_bytes_total AS new_alloc,
      n.alloc_bytes_total - o.alloc_bytes_total AS Δalloc,
    FROM old o FULL OUTER JOIN new n USING (test)
    ORDER BY test;
    "
else
    duckdb -c "
    WITH data AS (
      SELECT * FROM read_ndjson_auto($GLOB_EXPR)
    )
    SELECT
      commit[:7] AS commit,
      count(*) AS tests,
      sum(exec_instruction_count) AS Σinsns,
      sum(peak_heap_bytes) AS Σheap,
      sum(peak_stack_bytes) AS Σstack,
      sum(alloc_bytes_total) AS Σalloc,
      sum(exec_call_count) AS Σcall,
      sum(exec_tail_call_count) AS Σtail,
      sum(exec_match_count) AS Σmatch
    FROM data
    GROUP BY commit
    ORDER BY min(timestamp);
    "
fi
