# Running Rocq-extracted Scheme on shamrocq

This guide explains how to configure Rocq's Scheme extraction so that the
generated code runs efficiently on shamrocq — in particular, how to replace
Peano natural numbers with native integers and how Coq lists map to shamrocq
lists.

## How the extraction pipeline works

Rocq's `Extraction Language Scheme.` produces curried Scheme code that relies
on three conventions:

| Convention | Standard R5RS (`macros_extr.scm`) | shamrocq |
|---|---|---|
| Curried lambdas | `(lambdas (a b) body)` macro | Built-in syntax |
| Multi-arg application | `(@ f x y)` macro | Built-in syntax |
| Pattern matching | `match` macro using `car`/`cdr` | Native `match` dispatching on constructor tags |
| Constructors | List-encoded: `'(Tag field ...)` | Tagged 32-bit values: `\`(Tag ,field ...)` → `CTOR` bytecode |

Because shamrocq implements `lambdas`, `@`, `match`, and quasiquote
constructors as first-class language features, **most Rocq-extracted Scheme
runs unmodified** — no `macros_extr.scm` needed.

## Lists — zero-effort mapping

Rocq's `list` type extracts to `Nil` and `Cons` constructors.  These are
pre-interned in shamrocq's tag table:

| Constructor | Tag ID | Arity |
|---|---|---|
| `Nil` | 2 | 0 |
| `Cons` | 3 | 2 |

The extracted code:

```scheme
(define map (lambdas (f l)
  (match l
    ((Nil) `(Nil))
    ((Cons a t) `(Cons ,(f a) ,(@ map f t))))))
```

compiles to `PACK 2 0` for nil, `PACK 3 2` for cons cells, and `MATCH`/`BIND`
for destructuring — no special extraction directives needed.

### Building and inspecting lists from Rust

```rust
use shamrocq::{Vm, Value};

// Build a list [1, 2, 3]
let mut list = Value::ctor(ctors::NIL, 0);
for &x in [3, 2, 1].iter() {
    list = vm.alloc_ctor(ctors::CONS, &[Value::integer(x), list]).unwrap();
}

// Walk a list
let mut cur = list;
while cur.tag() == ctors::CONS {
    let head = vm.ctor_field(cur, 0);
    let tail = vm.ctor_field(cur, 1);
    // head.integer_value(), head.tag(), etc.
    cur = tail;
}
// cur.tag() == ctors::NIL
```

Each cons cell costs **2 words (8 bytes)** on the heap.  A list of length N
uses 8N bytes plus the values it contains.

## Peano nats — rewriting as native integers

### The problem

By default, Rocq extracts `nat` as Peano constructors `O` and `S`:

```scheme
(define length (lambda (l)
  (match l
    ((Nil) `(O))
    ((Cons _ t) `(S ,(length t))))))

(define leb (lambdas (n m)
  (match n
    ((O) `(True))
    ((S n~) (match m
               ((O) `(False))
               ((S m~) (@ leb n~ m~)))))))
```

Every natural number N allocates N heap words (one `S` constructor per
successor), and every comparison or arithmetic operation is O(N).  On a
memory-constrained target this is catastrophic.

### The solution: `Extract Inductive nat`

Shamrocq has native 29-bit signed integers with O(1) arithmetic (`ADD`, `SUB`,
`MUL`, `DIV`, `EQ`, `LT`).  We tell Rocq to map `nat` directly to these:

```coq
Extract Inductive nat => "int" [ "0" "(lambda (n) (+ n 1))" ]
  "(lambdas (fO fS n) (if (= n 0) fO (fS (- n 1))))".
```

The three parts are:

1. **Target type name** — `"int"` (informational, not used by shamrocq).
2. **Constructor replacements** — `O` becomes the integer literal `0`;
   `S` becomes `(lambda (n) (+ n 1))`, i.e. increment.
3. **Case function** — replaces pattern matching on `nat`.  For Scheme
   extraction, **branches come first** (one per constructor, in declaration
   order), **scrutinee comes last**.

The case function receives:
- `fO`: the value for the `O` branch (not thunked — passed directly),
- `fS`: a one-argument function for the `S` branch,
- `n`: the integer to inspect.

### What changes in the extracted code

| Before (Peano) | After (native int) |
|---|---|
| `` `(O) `` | `0` |
| `` `(S ,n) `` | `(+ n 1)` |
| `(match n ((O) e₁) ((S p) e₂))` | `(@ (lambdas (fO fS n) ...) e₁ (lambda (p) e₂) n)` |

The `length` function above would extract as:

```scheme
(define length (lambda (l)
  (match l
    ((Nil) 0)
    ((Cons _ t) (+ (length t) 1)))))
```

No heap allocation for naturals, and `+` compiles to a single `ADD`
instruction.

### Avoiding S chains in numeric literals

`Extract Inductive nat` replaces `S` with `(lambda (n) (+ n 1))` and `O`
with `0`.  A literal like `100` in Coq is `S (S (... (S O) ...))` — 100
nested applications.  The extracted code becomes:

```scheme
((lambda (n) (+ n 1)) ((lambda (n) (+ n 1)) ... ((lambda (n) (+ n 1)) 0) ...))
```

This compiles and evaluates correctly, but produces 100 closure calls at
runtime and bloats the bytecode.

**Fix: extract named constants as integer literals.**

If your Coq development defines numeric constants — fuel values, buffer
sizes, thresholds — extract them directly:

```coq
Definition max_depth := 64.
Definition timeout := 1000.

Extract Inlined Constant max_depth => "64".
Extract Inlined Constant timeout  => "1000".
```

`Extract Inlined Constant` replaces every occurrence of the constant with
the literal value, so no global slot is consumed and the integer compiles to
a single `INT` instruction.

For constants that are computed from others, extract the final value:

```coq
Definition buffer_words := Nat.div buffer_bytes 4.

Extract Inlined Constant buffer_words => "256".  (* 1024 / 4 *)
```

If you cannot name every literal (e.g. numeric arguments scattered across
function bodies), consider wrapping them:

```coq
Definition nat_literal (n : nat) : nat := n.
Extract Inlined Constant nat_literal => "(lambda (n) n)".
```

Then use `nat_literal 42` where you need a literal.  The identity lambda is
eliminated at extraction time thanks to `Inlined`, leaving just `42`.

### Optimizing standard library functions

With `Extract Inductive nat`, functions like `Nat.add` will still use the
case function to recurse one step at a time.  Override them with `Extract
Constant` to get O(1) implementations:

```coq
(* Arithmetic *)
Extract Constant Nat.add  => "(lambdas (n m) (+ n m))".
Extract Constant Nat.mul  => "(lambdas (n m) (* n m))".
Extract Constant Nat.sub  => "(lambdas (n m) (if (< n m) 0 (- n m)))".
Extract Constant Nat.div  => "(lambdas (n m) (if (= m 0) 0 (/ n m)))".
Extract Constant Nat.modulo =>
  "(lambdas (n m) (if (= m 0) n (- n (* (/ n m) m))))".

(* Comparisons — return True/False constructors *)
Extract Constant Nat.eqb => "(lambdas (n m) (if (= n m) `(True) `(False)))".
Extract Constant Nat.leb => "(lambdas (n m) (if (< m n) `(False) `(True)))".
Extract Constant Nat.ltb => "(lambdas (n m) (if (< n m) `(True) `(False)))".
```

Notes:
- `Nat.sub` is truncated subtraction (never negative), hence the guard.
- `Nat.div` and `Nat.modulo` define division by zero as 0 and n respectively,
  matching Coq's `Nat.div` and `Nat.modulo` specifications.
- shamrocq's `=` and `<` return `True`/`False` constructors (tags 0/1),
  which `if` dispatches on natively.

### Complete extraction preamble

Drop this block at the top of your Rocq extraction file:

```coq
Require Import Coq.extraction.Extraction.
Require Import Coq.extraction.ExtrScheme.

Extract Language Scheme.

(* --- nat → native int --- *)
Extract Inductive nat => "int" [ "0" "(lambda (n) (+ n 1))" ]
  "(lambdas (fO fS n) (if (= n 0) fO (fS (- n 1))))".

Extract Constant Nat.add    => "(lambdas (n m) (+ n m))".
Extract Constant Nat.mul    => "(lambdas (n m) (* n m))".
Extract Constant Nat.sub    => "(lambdas (n m) (if (< n m) 0 (- n m)))".
Extract Constant Nat.div    => "(lambdas (n m) (if (= m 0) 0 (/ n m)))".
Extract Constant Nat.modulo => "(lambdas (n m) (if (= m 0) n (- n (* (/ n m) m))))".
Extract Constant Nat.eqb    => "(lambdas (n m) (if (= n m) `(True) `(False)))".
Extract Constant Nat.leb    => "(lambdas (n m) (if (< m n) `(False) `(True)))".
Extract Constant Nat.ltb    => "(lambdas (n m) (if (< n m) `(True) `(False)))".

(* --- nat literal constants: add one line per named constant --- *)
(* Extract Inlined Constant my_const => "42". *)

(* --- list and bool map directly, no directives needed --- *)
```

### Compiler optimization: CaseNat

The case function `(lambdas (fO fS n) (if (= n 0) (fO 0) (fS (- n 1))))`
appears at every pattern match on a natural number. Without optimization,
each occurrence allocates three closures at runtime.

The compiler's `CaseNat` pass (see [CODEGEN.md](CODEGEN.md)) recognizes
this exact pattern — structurally, regardless of parameter names — and
rewrites it to a first-class `CaseNat` IR node. At codegen time this emits
an inline integer test and branch (`DUP`/`EQ`/`MATCH2`) instead of closure
allocations. On the sort benchmark this reduces closure allocations by 48%
and total instructions by 21%.

No extraction-side changes are needed — the standard `Extract Inductive nat`
directive produces the pattern that `CaseNat` recognizes automatically.

### Caveats

- **Strict branches**: the case function evaluates the `O` branch eagerly
  even when the scrutinee is non-zero.  Since Coq code is pure and
  terminating, this is semantically correct but may evaluate unused base
  cases.  In practice the base case is almost always a cheap constant.
- **Integer range**: shamrocq integers are 29-bit signed (−268 435 456 to
  +268 435 455).  Coq `nat` is unbounded, so extracted code that produces
  very large naturals will silently wrap.  For most embedded use cases the
  range is more than sufficient.
- **`lambda` arity**: shamrocq `lambda` takes exactly one parameter.  The
  case function and constructor replacements must use curried form.  The
  `lambdas` syntax sugar handles multi-parameter definitions.

## Binary naturals and integers (N, Z)

If your Coq development uses `N` or `Z` instead of `nat`, the same principle
applies.  The extraction is more involved because `positive` has three
constructors (`xI`, `xO`, `xH`):

```coq
Extract Inductive positive => "int"
  [ "(lambda (p) (+ (* 2 p) 1))" "(lambda (p) (* 2 p))" "1" ]
  "(lambdas (fI fO f1 p) (if (= p 1) f1 (if (= (- p (* (/ p 2) 2)) 0) (fO (/ p 2)) (fI (/ p 2)))))".

Extract Inductive N => "int" [ "0" "(lambda (p) p)" ]
  "(lambdas (f0 fp n) (if (= n 0) f0 (fp n)))".

Extract Inductive Z => "int" [ "0" "(lambda (p) p)" "(lambda (p) (neg p))" ]
  "(lambdas (f0 fp fn z) (if (= z 0) f0 (if (< 0 z) (fp z) (fn (neg z)))))".
```

The same `Extract Constant` technique should be used for `N.add`, `Z.add`,
etc. to avoid the per-bit recursion that the generic case function would
produce.
