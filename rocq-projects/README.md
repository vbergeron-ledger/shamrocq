# Rocq benchmark projects

Four self-contained Rocq projects that extract to Scheme and run on the
shamrocq VM.  Each project lives in its own directory with a `dune-project`
so it can be built independently with `dune build`.

## Projects

### sort

Merge sort on `list nat`.  Heavy list allocation (split + merge), O(n log n)
recursive calls, nat-to-int comparisons.  Exercises GC throughput and tail-call
optimisation on the accumulator-based merge.

### rbtree

Red-black tree insertion and lookup.  Multi-constructor ADTs (`color`, `tree`),
deep 4-way pattern matching in `balance`, tree-shaped allocation.  The best
stress test for `MATCH`/`BIND` sequences.

### eval

Lambda calculus normaliser.  `term` is a 3-constructor recursive type
(Var / Abs / App).  Capture-avoiding substitution and beta normalisation
produce heavy closure allocation, deep non-tail recursion, and recursive ADT
traversal.

### parser

Monadic binary-format parser over shamrocq byte values.  Combinators
`bind`/`pure`/`fail` drive closure chains; primitives `read_u8`/`read_u16_be`
map to `bytes-get` via `Extract Constant`.  The only benchmark that exercises
the `BYTES_*` opcodes.  Parses a simple TLV message format.

## Extraction convention

Every project contains a `Preamble.v` with the standard shamrocq extraction
directives (nat → native int, Nat.add → `+`, etc.) and an `Extract.v` that
pulls in the library and calls `Recursive Extraction`.

The extracted `.scm` files are committed so the Rust test suite can run without
requiring a Rocq installation.
