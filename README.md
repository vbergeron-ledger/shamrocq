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
scheme/                  Scheme sources (Rocq extractions + synthetic tests)
crates/
  shamrocq-compiler/     Build-time compiler: parse → optimize → resolve → codegen
  shamrocq/              no_std runtime: arena, value representation, bytecode VM
  shamrocq-bytecode/     Shared opcode definitions
doc/                     Technical documentation (see doc/README.md)
examples/
  baremetal/             Complete STM32 Cortex-M4 firmware example
```

### Compiler pipeline

1. **Parser** — S-expression reader
2. **Desugarer** — expands `lambdas`, `@`, `quasiquote`/`unquote`, `match`
3. **Optimization passes** — inline, beta-reduce, constant fold, dead
   binding elimination, case-of-known-ctor, eta-reduce
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
- **Direct calls** (`CALL_N`) bypass the curried closure chain for known
  multi-arity globals at exact arity
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
vm.load_program(&prog).unwrap();

let result = vm.call(funcs::NEGB, &[Value::ctor(tags::TRUE, 0)]).unwrap();
assert_eq!(result.tag(), tags::FALSE);
```

See [`examples/baremetal/`](examples/baremetal/) for a complete STM32
firmware example with FFI, list manipulation, and semihosting output.

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

Run tests with `stats` and capture results to a JSONL file:

```sh
BENCHMARK_FILE=benchmarks/run.jsonl \
BENCHMARK_COMMIT=$(git rev-parse --short HEAD) \
BENCHMARK_TIMESTAMP=$(date -Iseconds) \
  cargo test --features stats -p shamrocq -- --nocapture
```

Each test that calls `print_stats` appends a JSON line with allocation counts,
instruction counts, peak memory, and reclaim statistics.

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
