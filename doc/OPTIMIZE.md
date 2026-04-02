# Performance analysis and roadmap

## Done

### Jump-table MATCH (bytecode v3)

MATCH uses a dense jump table indexed by `scrutinee_tag - base_tag`.
O(1) dispatch instead of O(n) linear scan.  Entry format is 3 bytes
(arity + offset) instead of 4 (tag + arity + offset).  Gap entries
point to an ERROR instruction.

### CALL / TAIL_CALL (bytecode v4)

When the compiler sees `App^N(Global(g), args)` where N equals the known
arity of g, it emits all N arguments followed by `CALL flat_entry N`.
The VM jumps directly to a flat entry point compiled with `frame_depth =
arity`, bypassing the curried closure chain entirely.  No PAP allocations,
no intermediate CALL_DYNAMIC dispatches.  Curried CALL_DYNAMIC remains for
partial application and unknown callees.

### Compiler optimization passes

Fixed-point iteration over expr-level and resolved-level passes:
inline small globals, beta-reduce, CaseNat, constant fold, if→match,
dead binding elimination, case-of-known-ctor, eta-reduce, arity
specialization.
See [CODEGEN.md](CODEGEN.md#3½-optimization-passes-pass) for details.

### Unified call stack

Frame headers (`saved_fb`, `saved_pc`, `saved_env`, `saved_heap_top`) are
stored on the arena operand stack rather than in a separate fixed-size
`[CallFrame; 256]` array.  This removes the hard 256-depth limit and
eliminates one data structure from the VM.

### Ctor arity headers

Heap-allocated constructors now carry a 1-word arity header before their
fields.  This makes the heap self-describing (enables `ctor_arity()` without
bytecode context) and is a prerequisite for heap traversal / GC.  Nullary
constructors remain immediate values — no overhead.

### Frame-local heap reclamation

Each frame header saves `heap_top` at call entry.  On return, if the result
value does not reference heap memory allocated during the call, the region
`[saved_heap, current_heap_top)` is reclaimed by resetting `heap_top`.

This is sound because the language is pure: older heap objects never contain
pointers to newer allocations.  The optimization is especially effective for
functions that build intermediate data structures (e.g. merge/dedup in
hforest) but return a value allocated before the call.

### CaseNat — Church-encoded nat eliminator (bytecode v5)

Rocq's `Extract Inductive nat` produces a Church-encoded eliminator that
creates three closures per pattern match on a natural number.  The `CaseNat`
compiler pass recognizes this pattern at the Expr level and rewrites it to a
first-class `CaseNat(zero_case, succ_case, scrutinee)` IR node.

At codegen time, `CaseNat` emits an inline `DUP`/`INT0`/`EQ`/`MATCH2`
dispatch sequence — no closures are allocated for the eliminator itself.
Two new stack opcodes (`DUP` and `OVER`) preserve the scrutinee across
branches without introducing `Let` bindings.

Benchmark results (`shamrocq-bench`, merge_sort(rev_range(256))):

| Metric | Before | After | Change |
|---|---:|---:|---:|
| closures allocated | 5,375 | 2,815 | −47.6% |
| total alloc bytes | 150,512 B | 111,600 B | −25.8% |
| instructions | 119,835 | 94,748 | −20.9% |
| GC collections | 7 | 3 | −57.1% |
| GC bytes freed | 200,988 B | 84,852 B | −57.8% |

Impact scales with the number of nat eliminators in the program:

| Benchmark | Instructions | Closures | Heap |
|---|---|---|---|
| sort | −20.9% | −47.6% | −0.1% |
| rbtree | −6.5% | −33.0% | −15.9% |
| eval | −12.8% | −4.9% | −36.6% |
| parser | −0.6% | — | −0.2% |

### Garbage collector

Mark-and-compact GC over the contiguous heap.  Triggered when a bump
allocation would overflow, the collector traces live roots from the stack
and globals, compacts survivors toward the bottom of the heap, and updates
all pointers.  Reclaims memory from functions that accumulate intermediate
data beyond what frame-local reclamation can handle.

---

## Known issues

### 1. Residual Curried Overhead

CALL handles exact-arity calls to known globals, but several cases still
go through the curried CALL_DYNAMIC pipeline:

- **Partial application** — `(map f)` where `map` has arity 2.
- **Unknown callees** — higher-order calls like `(f x)` where `f` is a
  closure argument.
- **Over-application** — `(compose f g x)` where compose has arity 2 but
  receives 3 arguments.

Each of these creates PAP (Application) objects on the heap.  A GRAB/multi-arg
calling convention (see Roadmap) would handle all three.

### 2. GC pressure

The mark-and-compact GC handles heap exhaustion, but collections are
whole-heap: every object is traced and relocated.  Programs with high
allocation rates (e.g. recursive list construction) may trigger frequent
collections, each of which scans the entire live set.

### 3. Dispatch Overhead

The main loop is a `match opcode { ... }` with 30+ arms. The CPU branch
predictor sees a single indirect branch site for all opcodes, so it cannot
predict the next handler. Each iteration also checks `pc >= code.len()`.

### 4. Bounds Checking on Every Memory Access

`read_word` goes through `try_into().unwrap()` with slice bounds checks.
`stack_push` checks `heap_top + 4` every time. These are safe but costly
in the hot loop.

---

## Roadmap

### Tier 0: Low Effort

| Change | Impact | Effort |
|--------|--------|--------|
| **Remove `pc` bounds check** from the hot loop (trust bytecode, validate at load time) | Medium | Low |
| **`unsafe` aligned reads** for `read_word`/`write_word` on LE targets behind a `cfg` | Medium | Low |

### Tier 1: Full Multi-Arg Calling Convention (GRAB)

Phase 1 (CALL for exact-arity known calls) is done.  Remaining phases:

**Phase 2 — GRAB for unknown callees.**
Add `GRAB K` at the entry of all multi-arity functions.  CALL then works
with closures and higher-order calls, not just known globals.  GRAB handles
under-application (builds PAP) so the curried CALL1 path is no longer
needed for that case.

**Phase 3 — Over-application in RET.**
Extend the frame header with extra_args.  RET checks extra_args and
re-dispatches.  This handles cases like `(compose f g x)` where compose
has arity 2 but is called with 3 args.

### Tier 2: Interpreter Dispatch

| Change | Impact | Effort |
|--------|--------|--------|
| **Tail-call threaded dispatch** | High | Medium |
| **Superinstructions** (LOAD+CALL, PACK(0)+RET, MATCH+BIND, etc.) | Medium | Medium |

### Tier 3: Memory Management

| Change | Impact | Effort |
|--------|--------|--------|
| ~~**Copying/compacting GC**~~ | ~~Critical~~ | ~~High~~ — **done** (mark-and-compact) |
| **Generational / incremental GC** — reduce per-collection pause by partitioning the heap | Medium | High |

### Tier 4: Aspirational

| Change | Impact | Effort |
|--------|--------|--------|
| **Register-based bytecode** | High | Very High |
| **Template JIT** | Very High | Very High |
| **NaN-boxing** on 64-bit host targets | Medium | Medium |

---

## Priority order

1. **GRAB / full multi-arg calls** (Tier 1 phases 2–3)
2. **Unsafe word access** (Tier 0)
3. **Threaded dispatch** (Tier 2)
4. **Generational / incremental GC** (Tier 3)
