# Compiler pipeline

This document describes the passes that turn Scheme source into a bytecode
blob.

Source files: `crates/shamrocq-compiler/src/{parser,desugar,pass/,resolve,codegen,bytecode}.rs`.

```
  .scm source
      │
      ▼
 ┌──────────┐
 │  Parser   │   Sexp
 └────┬─────┘
      │ Vec<Sexp>
      ▼
 ┌──────────┐
 │ Desugarer │   Expr, Define
 └────┬─────┘
      │ Vec<Define>
      ▼
 ┌──────────────────┐
 │ Expr-level passes │   (inline, beta-reduce, constant fold, if→match)
 └────┬─────────────┘
      │ Vec<Define>
      ▼
 ┌──────────┐
 │ Resolver  │   RExpr, RDefine
 └────┬─────┘
      │ Vec<RDefine>
      ▼
 ┌──────────────────────┐
 │ Resolved-level passes │  (dead binding, case-of-known-ctor, eta-reduce,
 └────┬─────────────────┘   arity analysis, ANF)
      │ Vec<RDefine>
      ▼
 ┌──────────┐
 │ Codegen   │   bytecode
 └────┬─────┘
      │
      ▼
  bytecode.bin + bindings.rs
```

---

## 1. Parser (`parser.rs`)

**Input:** Scheme source text (UTF-8).

**Output:** `Vec<Sexp>` — a flat list of top-level S-expressions.

The parser is a hand-written recursive-descent reader over a byte slice.
It recognizes:

- **Atoms** — any run of non-delimiter characters.
- **Lists** — `(` items... `)`.
- **Reader macros** — expanded inline during parsing:
  - `'x`  → `(quote x)`
  - `` `x ``  → `(quasiquote x)`
  - `,x`  → `(unquote x)`
- **Strings** — `"..."`, kept as atoms with the quotes embedded.
- **Comments** — `;` to end of line, discarded.

Numeric literals are recognized as atoms and converted during desugaring.
String literals (`"..."`) are kept as atoms with their quotes for processing
in the desugarer.

---

## 2. Desugarer (`desugar.rs`)

**Input:** `Vec<Sexp>`.

**Output:** `Vec<Define>` — top-level definitions over a high-level IR with
named variables.

### IR types

```
Expr  = Var(name)
      | Int(i32)                   integer literal
      | Bytes(Vec<u8>)             byte string literal
      | Ctor(tag_name, fields)     constructor application
      | PrimOp(op, args)           built-in operation (+, -, *, /, etc.)
      | Lambda(param, body)        single-argument
      | App(func, arg)             single-argument
      | If(cond, then, else)
      | Let(name, val, body)
      | Letrec(name, val, body)    single-binding recursive let
      | Match(scrutinee, cases)
      | CaseNat(zero_case,         nat eliminator (see §3½)
               succ_case,
               scrutinee)
      | Error                      unreachable branch
```

### Transformations

| Source form | Desugared form |
|-------------|----------------|
| `(define name expr)` | `Define { name, body }` |
| `(lambda (x) body)` | `Lambda("x", body)` |
| `(lambdas (a b c) body)` | `Lambda("a", Lambda("b", Lambda("c", body)))` |
| `(f x y z)` | `App(App(App(f, x), y), z)` — auto-curried |
| `(@ f x y z)` | same as `(f x y z)` — explicit curried apply |
| `` `(Tag ,e1 ,e2) `` | `Ctor("Tag", [e1, e2])` |
| `` `(Tag) `` | `Ctor("Tag", [])` |
| `'Foo` | `Ctor("Foo", [])` |
| `42` | `Int(42)` — integer literal |
| `"hello"` | `Bytes(b"hello")` — byte string literal (quotes stripped) |
| `(+ a b)` | `PrimOp(Add, [a, b])` — and similarly for `- * / neg = <` |
| `(bytes-len s)` | `PrimOp(BytesLen, [s])` — and similarly for `bytes-get`, `bytes-eq`, `bytes-cat` |
| `(match s ((Ctor a b) body) ...)` | `Match(s, [case...])` |
| `(let ((x v)) body)` | `Let("x", v, body)` — multiple bindings nest |
| `(letrec ((f v)) body)` | `Letrec("f", v, body)` — single binding only |
| `(if c t e)` | `If(c, t, e)` (lowered to `Match` during resolve) |
| `(error)` | `Error` |
| `(load ...)` | skipped |

All functions are **single-argument** at this stage.  Multi-argument
`lambdas` and application are desugared into nested curried forms.

---

## 3. Resolver (`resolve.rs`)

**Input:** `Vec<Define>` + mutable `TagTable` + mutable `GlobalTable`.

**Output:** `Vec<RDefine>` — resolved IR with numeric indices only.

This pass does two things in sequence:

### 3a. Global registration (first pass)

All top-level `define` names are registered in the `GlobalTable` before any
body is resolved.  This allows mutual recursion between globals.

### 3b. Name resolution (second pass)

Each definition's body is walked and every name is resolved:

- **Local variables** → `Local(de_bruijn_index)` — index 0 is the innermost
  binding.  The resolver maintains a stack of local names and searches from
  the end.
- **Global variables** → `Global(slot_index)` — looked up in the
  `GlobalTable`.
- **Constructor names** → interned to `u8` tag IDs via the `TagTable`.
  Hardcoded tags (`True=0`, `False=1`, ..., `Build_hforest=11`) are
  pre-registered; new constructors get the next available ID.
- **`if`** is lowered to `Match` on `True`/`False` tags during this pass.

## 3½. Optimization passes (`pass/`)

Both the high-level IR (`Define`) and the resolved IR (`RDefine`) go through
fixed-point optimization loops: each set of passes runs repeatedly until no
more changes are made (up to `DEFAULT_MAX_PASS_ITERATIONS`).

### Expr-level passes (pre-resolution)

| Pass | File | Effect |
|------|------|--------|
| Inline small globals | `p00_inline.rs` | Inline definitions whose body is small enough |
| Beta-reduce | `p01_beta_reduce.rs` | `(lambda (x) body)(arg)` → `let x = arg in body` |
| CaseNat | `p01b_case_nat.rs` | Rewrite Church-encoded nat eliminators to `CaseNat` nodes |
| Constant fold | `p02_constant_fold.rs` | Evaluate known-constant arithmetic at compile time |
| If → Match | `p03_if_to_match.rs` | Lower `if` to `match` on True/False |

### Resolved-level passes (post-resolution)

| Pass | File | Effect |
|------|------|--------|
| Dead binding elimination | `p04_dead_binding.rs` | Remove unused `let` bindings |
| Case of known ctor | `p05_case_known_ctor.rs` | `match (Ctor ...) { (Ctor ...) → e }` → `e` |
| Eta-reduce | `p06_eta_reduce.rs` | `(lambda (x) (f x))` → `f` |
| Arity analysis | `p07_arity_analysis.rs` | Tag multi-arg lambdas with their arity for `CALL` |
| Arity specialization | `p07b_arity_specialize.rs` | Rewrite saturated curried calls to `AppN` for `CALL` emission |
| ANF normalization | `p08_anf.rs` | Lift non-atomic sub-expressions into `let` bindings |

The CaseNat pass recognizes the Church-encoded nat eliminator pattern
generated by Rocq's `Extract Inductive nat` directive:

```
((lambdas (fO fS n) (if (= n 0) (fO 0) (fS (- n 1)))) zc sc scrut)
```

and rewrites it to a first-class `CaseNat(zc, sc, scrut)` IR node. The
pattern is matched structurally (parameter names are ignored). At codegen
time, `CaseNat` emits an inline `DUP`/`EQ`/`MATCH2` dispatch sequence
instead of the three closure allocations that the original Church encoding
would produce.

ANF normalization guarantees that every argument in `App` and every field in
`Ctor` is atomic (a `Local` or `Global` reference).  This ensures the stack
contains no intermediate temporaries when `BIND` pushes match-destructured
fields — a critical invariant for the stack-based VM.

### Resolved IR

```
RExpr = Local(u8)           de Bruijn index
      | Global(u16)         global slot
      | Int(i32)            integer literal
      | Bytes(Vec<u8>)      byte string literal
      | Ctor(u8, Vec)       tag ID + fields
      | PrimOp(op, Vec)     built-in operation
      | Lambda(body)        param is implicit (index 0)
      | App(func, arg)      arg is guaranteed atomic after ANF
      | Let(val, body)
      | Letrec(val, body)
      | Match(scrutinee, cases)
      | CaseNat(zc, sc, scrut)  nat eliminator
      | Error
```

### Tag table

The `TagTable` assigns stable `u8` IDs to constructor names.  The first 12
are hardcoded to match the VM's `value::tags` module:

| ID | Name |
|----|------|
| 0 | True |
| 1 | False |
| 2 | Nil |
| 3 | Cons |
| 4 | O |
| 5 | S |
| 6 | Left |
| 7 | Right |
| 8 | Pair |
| 9 | Build_root |
| 10 | Build_edge |
| 11 | Build_hforest |
| 12+ | (user-defined, auto-assigned) |

---

## 4. Codegen (`codegen.rs`)

**Input:** `Vec<RDefine>`.

**Output:** `CompiledProgram` (header + bytecode).

### Overview

The code generator walks each resolved definition and emits bytecode into a
linear `Emitter` buffer.  Key design choices:

- **Tail-call detection** — every `compile_expr` call carries a `tail: bool`
  flag.  In tail position, `App` emits `TAIL_CALL` instead of `CALL`, and
  terminal expressions emit `RET` directly.
- **Primitive operations** — `PrimOp` nodes compile their arguments, then
  emit the corresponding arithmetic or byte-string opcode (`ADD`, `SUB`,
  `BYTES_LEN`, etc.).
- **Literals** — `Int(n)` emits `INT n`.  `Bytes(data)` emits
  `BYTES len data`.
- **Deferred lambda bodies** — lambda bodies are not emitted inline.  The
  `CLOSURE` instruction is emitted with a placeholder code address, and the
  body is queued.  After all globals are compiled, deferred bodies are
  emitted and their addresses are back-patched.
- **Capture analysis** — before emitting a `CLOSURE`, `collect_free` walks
  the lambda body to find free variables (de Bruijn indices >= the binding
  depth).  These are sorted and deduplicated, then `LOAD`ed onto the stack
  before the `CLOSURE` instruction packs them.

### Compilation context (`Ctx`)

The `Ctx` struct tracks how de Bruijn indices map to `LOAD` slot indices
during compilation:

```
Frame layout:  [param  bind_0  bind_1 ...]
LOAD index:     0      1       2      ...

Captures are NOT on the stack; they live in the closure on the heap and
are accessed via LOAD_CAPTURE through the VM's env register.
```

At frame depth `D` (number of locals including param):

- De Bruijn `idx < D` → `LOAD(D - 1 - idx)` — local variable.
- De Bruijn `idx >= D` → `LOAD_CAPTURE(cap_slot)` — captured parent variable.

### Global compilation

Each global's body is compiled at a top-level `Ctx` (no captures, no param).
The code offset is recorded in the program header.  At runtime,
`load_program` evaluates each global's code and stores the resulting `Value`.

### Match compilation

For `Match(scrutinee, cases)`:

1. Compile the scrutinee (pushes value onto stack).
2. Emit `MATCH` header with `n_cases` slots (placeholder offsets).
3. For each case:
   - Record the current code position, back-patch the case table entry.
   - If `arity > 0`: emit `BIND(arity)` to push destructured fields.
   - Compile the case body.
   - If non-tail and `arity > 0`: emit `SLIDE(arity)` to clean up.
   - If non-tail and not last case: emit `JMP` placeholder.
4. Back-patch all `JMP` targets to the end position.

### CaseNat compilation

For `CaseNat(zero_case, succ_case, scrutinee)`:

1. Compile `scrutinee` (pushes the integer onto the stack).
2. `DUP` — keep a copy for the branch body.
3. `INT0; EQ` — test whether the scrutinee is zero.
4. `MATCH2` (True=0, False=1) — branch on the boolean.

**Zero branch** (True):
- `DROP 1` — discard the duplicated scrutinee.
- Compile `zero_case`, push the result.
- `INT0` — push 0 as the argument.
- `CALL_DYNAMIC` (or `TAIL_CALL_DYNAMIC` in tail position).

**Succ branch** (False):
- Compile `succ_case`, push the closure.
- `OVER` — copy the scrutinee from below.
- `INT1; SUB` — compute `scrutinee − 1`.
- `CALL_DYNAMIC` (or `TAIL_CALL_DYNAMIC`).
- `SLIDE 1` — discard the leftover scrutinee (non-tail only).

This avoids allocating closures for the branches: `zero_case` and
`succ_case` are compiled directly as expressions rather than wrapped in
intermediate lambdas.

### Letrec compilation

1. Emit `PACK 0 0` — push a dummy placeholder value.
2. Compile the val expression (expected to be a `Lambda`).
3. Find which capture slot (if any) corresponds to the self-reference
   (de Bruijn 0 from the lambda's perspective).
4. Emit `FIXPOINT(cap_idx)` — patches the closure's capture to point to
   itself and replaces the dummy slot.
5. Compile the body.

### Lambda body emission

Lambda bodies are not emitted inline.  Instead:

1. The free variables are computed, `LOAD`ed onto the stack, and a `CLOSURE`
   instruction is emitted with a placeholder address.
2. The body + capture list are pushed onto a `deferred` queue.
3. After all top-level globals are compiled, `emit_deferred` drains the
   queue, emits each body (which may itself defer nested lambdas), and
   back-patches the `CLOSURE` address.

This means all lambda code appears after the global initializers in the
bytecode stream.

### Exact-arity calls (CALL / TAIL_CALL)

When the compiler encounters `App^N(Global(g), args)` and the arity analysis
pass has determined that `g` has arity N, it emits an exact-arity call
instead of N chained `CALL_DYNAMIC` instructions:

1. Compile all N arguments onto the stack.
2. Emit `CALL flat_entry N` (or `TAIL_CALL` in tail position).

The callee is compiled with a **flat entry point** — `frame_depth = arity`,
no captures, and de Bruijn indices map directly to `LOAD` slots.  This
bypasses the curried closure chain entirely: no PAP allocations, no
intermediate `CALL_DYNAMIC` dispatches.

Non-matching call sites (partial application, unknown callees) continue to
use `CALL_DYNAMIC`.

### Output

`CompiledProgram.serialize()` produces the final blob: the header
(global table) followed by the code section.  See [BYTECODE.md](./BYTECODE.md)
for the binary format.
