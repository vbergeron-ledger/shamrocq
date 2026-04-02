# Shamrocq

<p align="center">
  <img src="assets/logo.png" alt="Shamrocq logo" width="300">
</p>

**Scheme + Rocq = shamrock**

A minimal, `no_std` Scheme interpreter designed to run
[Rocq](https://rocq-prover.org/) (Coq) extracted code on bare-metal
microcontrollers.

## Target

- STM32 family, Cortex-M4
- No libc, no dynamic allocation
- Memory budget: 30–64 KB (BYOB — Bring Your Own Buffer)

The caller provides a mutable byte slice; the VM does all allocation inside it
via a bump allocator.  Computations are finite data transformations — no
long-running tasks.

## Architecture

```
crates/
  shamrocq-compiler/     Build-time compiler: parse → optimize → resolve → codegen
  shamrocq/              no_std runtime: arena, value representation, bytecode VM
  shamrocq-bytecode/     Shared opcode definitions
doc/                     Technical documentation (see doc/README.md)
rocq-projects/           Rocq/Coq sources + extraction configs (sort, rbtree, eval, parser)
examples/
  sort/                  Merge sort benchmark (Cortex-M4 baremetal)
  rbtree/                Red-black tree benchmark
  eval/                  Lambda calculus normaliser benchmark
  parser/                Binary format parser benchmark
  demo/                  Minimal demo firmware
tools/
  bench/                 Baremetal benchmark runner (QEMU semihosting)
```

### Compiler pipeline

1. **Parser** — S-expression reader
2. **Desugarer** — expands `lambdas`, `@`, `quasiquote`/`unquote`, `match`
3. **Optimization passes** — inline, beta-reduce, CaseNat, constant fold,
   dead binding elimination, case-of-known-ctor, eta-reduce, arity
   specialization
4. **Resolver** — de Bruijn indexing, constructor tag interning
5. **Arity analysis + ANF** — tags multi-arg globals for direct calls,
   normalizes to A-normal form
6. **Codegen** — emits a compact bytecode blob

See [doc/CODEGEN.md](doc/CODEGEN.md) for details.

### Runtime

- **Values** are tagged 32-bit words: constructors, integers, byte strings,
  closures, and bare function pointers
- **Heap** uses bump allocation; constructors carry an arity header word for
  self-describing heap layout
- **Stack** grows downward from the other end of the same buffer
- **Frame-local reclamation** reclaims heap memory on function return when
  the result does not reference the frame's allocations
- **Mark-and-compact GC** handles heap exhaustion by tracing live roots and
  compacting survivors
- **CaseNat** rewrites Church-encoded nat eliminators (from Rocq extraction)
  into inline dispatch, eliminating closure allocations
- **Direct calls** (`CALL`) bypass the curried closure chain for known
  globals at exact arity
- **Match** dispatches via O(1) jump table indexed by constructor tag

See [doc/VM.md](doc/VM.md) for internals and [doc/BYTECODE.md](doc/BYTECODE.md)
for the instruction set.

## Usage

### 1. Compile Scheme to bytecode

```sh
cargo install --path crates/shamrocq-compiler

shamrocq-compiler -o out/ mylib.scm helpers.scm
```

```
compiled 12 globals, 8 ctors, 1024 bytes of bytecode from 2 files
  -> out/bytecode.bin
  -> out/bindings.rs
```

| Output file | Contents |
|-------------|----------|
| `bytecode.bin` | Compiled bytecode image |
| `bindings.rs` | `pub mod funcs`, `pub mod ctors`, `pub mod foreign` with const indices |

Run `shamrocq-compiler --help` for all options.

### 2. Embed in your `no_std` project

Include the generated files in your firmware crate:

```rust
static BYTECODE: &[u8] = include_bytes!("path/to/bytecode.bin");

mod bindings {
    include!("path/to/bindings.rs");
}
use bindings::{funcs, ctors};
```

Then load and run:

```rust
use shamrocq::{Program, Vm, Value, tags};

let mut buf = [0u8; 65536];
let prog = Program::from_blob(BYTECODE).unwrap();
let mut vm = Vm::new(&mut buf);
vm.load(&prog).unwrap();

let result = vm.call(funcs::NEGB, &[Value::ctor(tags::TRUE, 0)]).unwrap();
assert_eq!(result.tag(), tags::FALSE);
```

See [`examples/demo/`](examples/demo/) for a complete STM32 firmware example
with FFI, list manipulation, and semihosting output.

## Footprint

Compiled sizes (release, `thumbv7em-none-eabihf`):

| Component | Size |
|-----------|------|
| VM + app code (`.text`) | ~13 KB |
| Bytecode (baremetal demo, 7 globals) | < 1 KB |

## Optional features

- **`stats`** — enables `vm.stats` and `vm.mem_snapshot()` for tracking
  peak heap/stack usage, allocation counts, instruction counts, call depth,
  and heap reclamation statistics.

## Tests

```sh
cargo test                          # without stats
cargo test --features stats         # with memory/execution statistics printed
```

### Benchmarking

#### Baremetal benchmarks (QEMU)

The `shamrocq-bench` tool builds each example as a Cortex-M firmware, runs
it in QEMU with semihosting, and captures VM statistics:

```sh
cargo run -p shamrocq-bench                  # all benchmarks
cargo run -p shamrocq-bench -- sort rbtree   # specific benchmarks
cargo run -p shamrocq-bench -- --profile     # + QEMU function profiling
```

Results are saved to `bench-results/` and printed as a summary table with
peak heap, peak stack, instructions, calls, allocations, reclaim, and
wall-clock time.

#### Test-based stats

Run tests with `stats` to see per-test VM counters:

```sh
cargo test --features stats -p shamrocq -- --nocapture
```

## Documentation

See [`doc/README.md`](doc/README.md) for the full list:
[VM internals](doc/VM.md) ·
[Bytecode format](doc/BYTECODE.md) ·
[Compiler pipeline](doc/CODEGEN.md) ·
[FFI](doc/FFI.md) ·
[Rocq extraction](doc/ROCQ.md) ·
[Performance roadmap](doc/OPTIMIZE.md)

## License

MIT
