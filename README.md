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
scheme/              Scheme sources (Rocq extractions + synthetic tests)
crates/
  shamrocq-compiler/ Build-time compiler: parse → desugar → resolve → ANF → codegen
  shamrocq/          no_std runtime: arena, value representation, bytecode VM
```

### Compiler pipeline

1. **Parser** — S-expression reader
2. **Desugarer** — expands `lambdas`, `@`, `quasiquote`/`unquote`, `match`
3. **Resolver** — de Bruijn indexing, constructor tag interning
4. **ANF normalizer** — lifts complex arguments into `let` bindings
5. **Codegen** — emits a compact bytecode blob

The compiler is available both as a library (used by `build.rs` to embed
bytecode at compile time) and as a standalone CLI.

### Runtime

- **Values** are tagged 32-bit words: constructors, integers, byte strings,
  closures, and bare function pointers
- **Heap** uses bump allocation inside the caller-provided buffer
- **Stack** grows downward from the other end of the same buffer
- **Closures** capture by value; recursive closures use `FIXPOINT`
- **Direct calls** bypass the curried closure chain for known multi-arity globals
- **Match** dispatches on constructor tags with `BIND`/`SLIDE` for field access

### Generated modules

`build.rs` produces two Rust modules from the compiled Scheme:

- `funcs` — global function indices (`funcs::APPEND`, `funcs::NAT_ORD`, …)
- `ctors` — constructor tag constants (`ctors::SOME`, `ctors::LEAF`, …)

Core tags (`True`, `False`, `Nil`, `Cons`, `O`, `S`, …) live in
`value::tags`.  Scheme-defined constructors get their constants generated
automatically — no manual registration needed.

## Usage

### 1. Compile Scheme to bytecode

```sh
cargo install --path crates/shamrocq-compiler

shamrocq-compiler -o out/ mylib.scm helpers.scm
```

```
compiled 12 globals, 8 ctors, 1024 bytes of bytecode from 2 files
  -> out/bytecode.bin
  -> out/funcs.rs
  -> out/ctors.rs
```

| Output file | Contents |
|-------------|----------|
| `bytecode.bin` | Compiled bytecode image |
| `funcs.rs` | `pub const` for each global function index |
| `ctors.rs` | `pub const` for each constructor tag ID |

Run `shamrocq-compiler --help` for all options.

### 2. Embed in your `no_std` project

Include the generated files in your firmware crate:

```rust
static BYTECODE: &[u8] = include_bytes!("path/to/bytecode.bin");

mod funcs {
    include!("path/to/funcs.rs");
}

mod ctors {
    include!("path/to/ctors.rs");
}
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

### In-tree development

Within this repo, enabling the `integration` feature compiles all `.scm` files
under `scheme/` via `build.rs` and embeds the result, so the generated `funcs`
and `ctors` modules are available directly from the `shamrocq` crate.
This feature is **not** enabled by default — downstream users bring their own
compiled bytecode via the CLI.

## Footprint

Compiled sizes (release, `thumbv7em-none-eabihf`):

| Component | Size |
|-----------|------|
| VM runtime (`.text`) | ~12 KB |
| Bytecode (`hash_forest.scm`, 32 globals) | ~3 KB |
| **Total flash** | **~15 KB** |

## Optional features

- **`integration`** — compiles the in-tree `scheme/` files via `build.rs` and
  embeds the bytecode, `funcs`, and `ctors` modules. Only needed for running
  the repo's own integration tests; downstream users compile their own Scheme
  via the CLI.
- **`stats`** — enables `vm.stats` and `vm.mem_snapshot()` for tracking
  peak heap/stack usage, allocation counts, instruction counts, and call depth.

## Tests

```sh
cargo test --features integration                  # without stats
cargo test --features integration,stats            # with memory/execution statistics printed
```

## License

MIT
