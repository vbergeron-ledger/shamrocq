# Memory optimization

This document evaluates techniques to reduce runtime memory consumption in
shamrocq, drawing from three reference small-Lisp implementations:
**TinyScheme**, **PicoLisp**, and **FemtoLisp**.

## Current memory layout

The VM operates in a single caller-provided buffer (target: 30–64 KB):

```
┌──────────────────────────────────────────────┐
│  Heap ──▶                    ◀── Stack       │
│  (bump alloc, grows up)      (grows down)    │
└──────────────────────────────────────────────┘
```

- **Heap**: constructors, closures, and heap-backed byte-string payloads,
  bump-allocated, never freed.
- **Stack**: operand stack for the bytecode interpreter.
- **Globals**: `[Value; 64]` — 256 B, fixed, on the Rust stack.
- **Call stack**: `[CallFrame; 256]` — 2 KB on Cortex-M4, on the Rust stack.

### Value representation

Every value is a 32-bit tagged word.  The high **3-bit kind** (bits 31–29)
selects the layout; the remaining bits are payload (tag extensions, offsets, or
inline data):

| Kind (bits 31–29) | Layout | Heap cost |
|--------------------|--------|-----------|
| `000` — Ctor | 8-bit tag, 21-bit word offset | `arity × 4` B (0 for nullary) |
| `001` — Integer | 29-bit signed value (tag extends payload) | 0 |
| `010` — Bytes | 8-bit length, 21-bit word offset | `ceil(len/4) × 4` B |
| `110` — Closure | 21-bit word offset | `(1 + n_captures) × 4` B |
| `111` — Bare fn | code address in payload | 0 |

Ctors store no header on the heap — the tag and arity come from the
bytecode / the `Value` word itself.  Closures store a 4-byte header
(`code_addr:u16 | n_captures:u16`) followed by capture values.

### Where memory goes

Stats from the integration tests reveal that **closures dominate the heap**.
Small integers and short byte strings are represented **without** heap
allocation (immediate `Integer` and inline or short `Bytes` encodings); the
table below counts **heap-backed** constructors and closures only.

| Test | Ctors | Closures | Total heap |
|------|-------|----------|------------|
| `hforest_lifecycle` | 92 | 215 | 2 952 B |
| `hforest_merge_basic` | 26 | 54 | 800 B |
| `tree_to_list_sorted` | 32 | 25 | 552 B |

The closure-to-ctor ratio is still driven by the fully-curried calling
convention where it applies: `(lambdas (a b c) body)` desugars into
`Lambda(Lambda(Lambda(body)))`, and each partial application allocates a new
closure with its own copy of captures.  **Known-arity direct calls** (see
Implemented) bypass that path for many global multi-argument calls.

The heap is bump-only — every intermediate closure and constructor cell
produced during computation stays on the heap forever, even after becoming
unreachable.

---

## Reference implementations

### TinyScheme

- **Value representation**: tagged-union `cell` — a `_flag` word (5-bit type +
  GC/immutability bits) and a union holding `{car, cdr}`, `{string_ptr, len}`,
  a `num`, or a `foreign_func`.  ~24 B per cell on 64-bit.
- **GC**: mark-and-sweep with **Schorr–Deutsch–Waite link-inversion** for the
  mark phase — traverses the object graph in **O(1) auxiliary stack space** by
  temporarily reversing pointers.
- **Closures**: a cons cell tagged `T_CLOSURE` — `car` = code, `cdr` =
  environment chain.  Maximally compact: 2 pointers per closure, environments
  are shared chains.
- **Symbols**: interned in a hash table, no duplication.
- **Notable trick**: `get_consecutive_cells` finds N adjacent free cells in the
  free list for vector allocation.

### PicoLisp

- **Value representation**: everything is a **2-word cell** (CAR + CDR).  Type
  information is encoded in the **low 4 bits of pointers** (all cells are
  even-aligned).  No union, no type field, no separate header.
- **GC**: mark-and-sweep using **bit 0 of CDR** as the mark bit (stolen from
  the pointer field).  No separate mark bitmap needed.
- **Closures/environments**: PicoLisp uses **dynamic binding** with an explicit
  bind-frame stack.  No closures in the traditional sense — avoids all closure
  allocation overhead entirely.
- **Heap**: linked blocks of fixed-size cells.  Free cells form an intrusive
  free list via CAR.  Uniform cell size → zero fragmentation, trivial
  allocator.

### FemtoLisp

- **Value representation**: **3-bit tagged pointers** (not NaN-boxing).
  `value_t` is a machine word.  Fixnums exploit `x & 3 == 0` (both `0b000`
  and `0b100`), giving **30-bit signed integers** with zero heap cost.
- **Closures**: a function object is exactly **4 words**: `{bcode, vals, env,
  name}`.  The `env` is a captured environment shared by all closures from the
  same scope.
- **GC**: **Cheney-style copying/semispace collector** — copies all live data
  from `fromspace` to `tospace` via `relocate()`, installs forwarding
  pointers.  Automatically compacts; zero fragmentation after GC.
- **Stack**: separate `value_t` array with explicit `SP` and `curr_frame`.
  Frame metadata is pushed directly onto the value stack.
- **Notable tricks**:
  - `leafp(a) = (a & 3) != 3` — O(1) "no heap references" test, skips GC
    recursion.
  - `cons_reserve(n)` — batch-allocates N cons cells in one bump.
  - `alloc_words(n)` always aligns to 2-word boundaries.

---

## Optimization opportunities

### Implemented

#### ~~1. Zero-capture closures as bare function pointers~~ ✓

Kind `0b11` encodes a bare function pointer: the code address is stored
directly in the `Value` word with no heap allocation.  The existing `CLOSURE`
bytecode with `n_captures=0` produces a bare-fn value at runtime — no
compiler changes were needed.

Measured impact (before → after):

| Test | Heap before | Heap after | Savings |
|------|-------------|------------|---------|
| `load_program` | 260 B | 8 B | 97% |
| `negb(true)` | 260 B | 8 B | 97% |
| `compose(negb,negb)` | 276 B | 28 B | 90% |
| `tree_insert_and_member` | 1 072 B | 820 B | 23% |
| `tree_to_list_sorted` | 1 124 B | 872 B | 22% |
| `hforest_merge_basic` | 1 748 B | 1 500 B | 14% |
| `hforest_lifecycle` | 6 576 B | 6 328 B | 4% |

Biggest win is at program load (63 zero-capture globals) and for simple
function calls.  Heavier workloads see 14–23% reduction from zero-capture
closures created during partial application.

#### ~~2. Uncurried / multi-argument closures (known-arity direct calls)~~ ✓

Implemented as **known-arity direct calls**: the compiler detects fully applied
calls to known multi-arity globals and emits `CALL_DIRECT` / `TAIL_CALL_DIRECT`
into a **flat** code body, bypassing intermediate closure creation and the
curried apply chain for those sites.

Measured impact:

| Signal | Result |
|--------|--------|
| `hforest_lifecycle` | 24% fewer instructions, 63% fewer closures |
| `int_gcd`, `int_pow` | 70–83% fewer closures |

#### ~~3. Word-granularity heap offsets~~ ✓

The value refactor stores **word indices** (`byte offset / 4`) in the 21-bit
heap payload.  With word addressing, 21 bits cover **8 MB** of heap — ample
for the embedded target — and frees encoding space compared to byte offsets
within the same word layout.

#### `GLOBAL_CALL` — subsumed

The Tier 2 idea “`GLOBAL_CALL` — avoid materializing closures for known
targets” is **subsumed** by `CALL_DIRECT` / `TAIL_CALL_DIRECT`: direct calls
already jump to the global’s flat body without pushing and re-reading a
closure for those patterns.  A separate `GLOBAL_CALL` opcode is unnecessary.

### Rejected

#### ~~Arena watermarking / checkpoint-reset~~

Watermarking (save `heap_top`, reset after a call) was investigated and
rejected.  The simple case — result is immediate, just reset — only helps
trivial boolean predicates.  The heavy workloads (map, merge, forest
operations) all return heap-allocated structures, requiring full relocation.

Relocation is blocked by a structural issue: **constructors have no heap header**.
A constructor cell on the heap is just its fields — no length word, no tag.  The arity
was known at compile time from the `TUPLE tag arity` bytecode operand but is
not stored anywhere at runtime.  This means:

- **Can't determine ctor size** from a heap offset alone — needed to copy it.
- **Can't place forwarding pointers** — no header word to overwrite, so shared
  references would be duplicated (increasing memory) or require an external
  side table.
- **Self-referential closures** (`FIXPOINT`) create cycles that require
  forwarding pointers to handle correctly.

Fixing this would require adding a header word to every ctor (+4 B each),
building a compile-time tag→arity table, or encoding arity in the Value word.
Each option partially negates the savings and constitutes a significant design
change.  Furthermore, watermarking only works at `vm.call()` boundaries —
internal recursion garbage within a single call is not reclaimable.

### Tier 1 — high impact

#### 1. Shared closure environments (FemtoLisp-style)

When sibling closures from the same scope capture the same variables, each
independently copies all captures:

```
Current:   Closure_A [hdr|c0|c1|c2] + Closure_B [hdr|c0|c1|c2]  = 8 words
Shared:    Env [c0|c1|c2] + Closure_A [hdr|env] + Closure_B [hdr|env]  = 7 words
```

In `hash_forest.scm`, many functions create 2–4 lambdas that all capture `h`
(the ordering function).  Savings scale with capture count × number of sibling
closures.

Requires: compiler grouping of sibling lambdas, new env-block allocation, VM
env-pointer dereference.

### Tier 2 — medium impact

#### 2. Inline unary constructors

`S(n)` costs 4 bytes on the heap for one field.  When the field is itself an
immediate (like `S(O)`), the entire value fits in 32 bits with no heap
allocation.  Reserve a bit to distinguish "payload is heap offset" from
"payload is inline immediate":

```
0b01_tttttt_0_ppppppppppppppppppppppp   heap offset (23 bits → 8 MB)
0b01_tttttt_1_iiiiiiiiiiiiiiiiiiiiiii   inline immediate field (23 bits)
```

Makes Peano numbers up to ~depth 23 free (no heap), and `Some(x)`, `Left(x)`,
`Right(x)` with immediate payloads also free.

Requires: value encoding change, VM BIND/tuple_field adjustment.

#### 3. In-arena call frames (FemtoLisp-style)

The `[CallFrame; 256]` array is 2 KB on Cortex-M4.  Pushing `return_pc` and
`frame_base` as raw `u32` values onto the arena stack instead would:

- Eliminate the fixed 2 KB overhead.
- Make the call depth limit dynamic (bounded by available arena space).
- Unify all runtime state into the single buffer.

Requires: VM refactor of call/return paths.

### Tier 3 — lower impact / exploratory

#### 4. Lambda lifting (compiler pass)

Lift lambdas with few free variables to top-level functions with extra
parameters.  The caller passes captures as arguments.  Trades heap allocation
(permanent in bump model) for stack space (temporary) — almost always
favorable given bump-only allocation.

#### 5. Stack-allocated temporary tuples

Tuples that are immediately destructured by `MATCH`/`BIND` and never escape
could stay on the stack instead of the heap.  Requires escape analysis in the
compiler.

---

## Recommended implementation order

| Priority | Technique | Complexity | Estimated savings |
|----------|-----------|------------|-------------------|
| ~~done~~ | ~~Zero-capture closures as bare fns~~ | ~~Low~~ | ~~97% load, 14–23% heavy~~ |
| ~~done~~ | ~~Known-arity direct calls (`CALL_DIRECT` / `TAIL_CALL_DIRECT`)~~ | ~~Medium~~ | ~~e.g. 24% fewer instr. / 63% fewer closures (`hforest_lifecycle`); 70–83% closure cut (`int_gcd`, `int_pow`)~~ |
| ~~done~~ | ~~Word-granularity heap offsets (21-bit word index)~~ | ~~Low~~ | ~~8 MB addressable; supports denser encodings~~ |
| 1 | Inline unary constructors | Medium | Significant for Peano-heavy code |
| 2 | Shared environments | Medium | Good for HOF-heavy code |

Known-arity direct calls address a large slice of closure proliferation for
common global call sites.  Inline unary constructors and shared environments are
independent medium-complexity changes that can be done in any order.
