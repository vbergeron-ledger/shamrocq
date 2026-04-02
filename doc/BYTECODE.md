# Bytecode format

This document describes the binary format produced by `shamrocq-compiler` and
consumed by the shamrocq VM at runtime.

All multi-byte integers are **little-endian**.

## Blob layout

A compiled blob is a single contiguous byte array:

```
┌───────────────────────┐
│  Header               │  magic, version, global table, tag table
├───────────────────────┤
│  Code                 │  instruction stream
└───────────────────────┘
```

### Header

```
magic    : [u8; 4]           "SMRQ"
version  : u16le             bytecode version (currently 5)
n_globals: u16le             number of top-level defines

For each global (repeated n_globals times):
  name_len : u8              length of the name in bytes
  name     : [u8; name_len]  UTF-8 name (e.g. "negb", "append")
  offset   : u16le           byte offset into the Code section

n_tags   : u16le             number of embedded constructor tags
For each tag (repeated n_tags times):
  name_len : u8
  name     : [u8; name_len]  UTF-8 tag name (e.g. "True", "Cons")
```

The index of a global in this table is its **slot index**, used by `GLOBAL`
instructions and by the Rust-side `funcs::` constants.

### Code

A flat byte stream of instructions starting immediately after the header.
All code offsets (jump targets, closure addresses) are absolute byte positions
within this section.

## Instruction set

Each instruction is a 1-byte opcode followed by zero or more inline operands.

### Stack / locals

#### `LOAD` (0x01)

```
01 idx:u8
```

Copy the value at frame slot `idx` and push it. For closures, the first
slots hold bound values (captures, then any previously applied arguments),
followed by the fresh argument(s), then let-bindings.

#### `LOAD2` (0x02)

```
02 idx_a:u8 idx_b:u8
```

Push two frame slots in one instruction. Equivalent to `LOAD a; LOAD b`.

#### `LOAD3` (0x03)

```
03 idx_a:u8 idx_b:u8 idx_c:u8
```

Push three frame slots in one instruction.

#### `GLOBAL` (0x05)

```
05 idx:u16le
```

Push the already-evaluated value of global slot `idx`.

#### `DROP` (0x06)

```
06 n:u8
```

Remove the top `n` values from the stack.

#### `SLIDE` (0x07)

```
07 n:u8
```

Pop the result, drop `n` values below it, re-push the result. Used to clean
up `let` and `match` bindings in non-tail position.

#### `SLIDE1` (0x26)

```
26
```

Pop the result, drop the value below it, re-push the result. Specialized
form of `SLIDE 1` — saves one byte in the common single-binding case.

#### `DUP` (0x29)

```
29
```

Duplicate the top-of-stack value. Stack `[… a]` becomes `[… a a]`.

#### `OVER` (0x2A)

```
2A
```

Copy the second value from the top of stack and push it. Stack `[… a b]`
becomes `[… a b a]`. Used by `CaseNat` codegen to preserve the scrutinee
while building the succ-case call.

### Data

#### `PACK` (0x08)

```
08 tag:u8 arity:u8
```

If `arity` is 0, push a nullary constructor value (no heap allocation).
Otherwise pop `arity` values from the stack (top = last field), heap-allocate
a constructor with the given tag, push the result.

#### `PACK0` (0x23)

```
23 tag:u8
```

Push a nullary constructor with the given tag. No heap allocation.
Specialized form of `PACK tag 0` — saves one operand byte.

#### `UNPACK` (0x09)

```
09 n:u8
```

Pop a constructor, push its first `n` fields (field 0 first). Used for
single-case match destructuring.

#### `BIND` (0x0A)

```
0A n:u8
```

Pop the scrutinee constructor, push its first `n` fields. Like `UNPACK` but
used after a `MATCH` instruction to make constructor fields available as
local bindings.

#### `FOREIGN` (0x0B)

```
0B idx:u16le arity:u8
```

Push a `Value::foreign_fn(idx, arity)` — a host function callable from Scheme.
The `idx` is the registration index used with `vm.register_foreign()`.

#### `CLOSURE` (0x0C)

```
0C code_addr:u16le arity:u8 n_captures:u8
```

If `n_captures` is 0, push a `Value::function(code_addr, arity)` — no heap
allocation.

Otherwise, pop `n_captures` values from the stack (top = last capture),
heap-allocate a closure object, push the result.  The `arity` field is the
**total** number of values the function body expects on the stack
(captures + parameters).

Heap layout of a closure:

```
word 0:  code_addr:u16 | arity:u8 | n_bound:u8
word 1…: bound values (raw u32 each)
```

Partial application extends an existing closure by appending additional
bound values (via `extend_closure` in the arena).  When `n_bound + 1 ==
arity`, the call is saturated and all bound values are pushed onto the stack
as the first frame slots.

#### `FIXPOINT` (0x0D)

```
0D cap_idx:u8
```

Peek the closure at TOS. If `cap_idx != 0xFF`, mutate
`closure.bound[cap_idx]` to point to the closure itself (self-reference).
Then overwrite slot 1 (the `letrec` dummy) with the closure and pop TOS.

This is the only mutation in the entire VM — it makes `letrec` work without
a GC or indirection cell.

#### `FUNCTION` (0x28)

```
28 code_addr:u16le arity:u8
```

Push a `Value::function(code_addr, arity)` — a bare (zero-capture) function
pointer. Used when a lambda has no free variables and does not need heap
allocation.

### Control flow

#### `CALL_DYNAMIC` (0x0E)

```
0E
```

Pop `arg`, pop `func`. If the call is saturated (`n_bound + 1 == arity`),
push a 3-word frame header `[saved_fb, saved_pc, saved_heap_top]`, push
the closure's bound values followed by `arg`, and jump to the code address.
For undersaturated calls, extend the closure with the new argument (no
frame push).  For foreign functions, the host Rust function is called
directly — no bytecode frame pushed.

#### `TAIL_CALL_DYNAMIC` (0x0F)

```
0F
```

Pop `arg`, pop `func`. **Truncate** the current frame (reuse the frame header).
Set up `arg` in the recycled frame, jump. No stack growth — this is how
tail recursion stays bounded.

#### `CALL` (0x10)

```
10 code_addr:u16le n_args:u8
```

The `n_args` arguments are already on the stack. Push a 3-word frame header
`[saved_fb, saved_pc, saved_heap_top]`. Set up a new frame with all `n_args`
arguments, jump to `code_addr`. No function Value on the stack — the target
is statically known at compile time.

This is used for exact-arity calls to known globals. The compiler
detects `App^N(Global(g), args)` where N equals the known arity of g and
emits all N arguments followed by `CALL flat_entry N`, bypassing the
curried closure chain entirely.

#### `TAIL_CALL` (0x11)

```
11 code_addr:u16le n_args:u8
```

Like `CALL` but in tail position. Saves the `n_args` arguments, truncates
the current frame, re-pushes the arguments, and jumps. No call-stack growth.

#### `RET` (0x12)

```
12
```

Pop the result, discard the current frame. If call depth is zero, return to
the Rust caller. Otherwise restore `(frame_base, pc, saved_heap_top)` from
the frame header, attempt frame-local heap reclamation (see
[VM.md](VM.md#frame-local-heap-reclamation)), and push the result in the
caller's frame.

#### `MATCH` (0x13)

```
13 base_tag:u8 n_entries:u8
   [arity:u8 offset:u16le] × n_entries
```

Pop the scrutinee. Compute `idx = scrutinee_tag - base_tag`. If `idx >=
n_entries` → `MatchFailure`. Otherwise index into the jump table at slot
`idx`. If `arity > 0`, re-push the scrutinee (for a subsequent `BIND`).
Jump to `offset`.

Gap entries (tags in range but not matched) point to an `ERROR` instruction.

#### `MATCH2` (0x27)

```
27 base_tag:u8
   [arity:u8 offset:u16le] × 2
```

Two-entry match. Identical to `MATCH` but with an implicit `n_entries = 2`,
saving one header byte. Used for boolean dispatches (True/False) and
`CaseNat` zero/non-zero tests.

#### `JMP` (0x14)

```
14 offset:u16le
```

Set `pc = offset`. Used after non-tail match branches to skip remaining cases.

#### `ERROR` (0x15)

```
15
```

Halt with `MatchFailure(tag=0xFF)`. Emitted for exhaustiveness-checked match
arms that should be unreachable.

### Integer

#### `INT` (0x16)

```
16 value:i32le
```

Push a 29-bit signed integer Value onto the stack. No heap allocation.

#### `INT0` (0x24)

```
24
```

Push integer 0. Specialized form of `INT 0` — saves 4 bytes.

#### `INT1` (0x25)

```
25
```

Push integer 1. Specialized form of `INT 1` — saves 4 bytes.

#### `ADD` (0x17)

Pop two integers, push their sum (wrapping).

#### `SUB` (0x18)

Pop `b`, pop `a`, push `a - b` (wrapping).

#### `MUL` (0x19)

Pop two integers, push their product (wrapping).

#### `DIV` (0x1A)

Pop `b`, pop `a`, push `a / b` (wrapping).

#### `NEG` (0x1B)

Pop one integer, push its negation.

#### `EQ` (0x1C)

Pop two integers. Push `True` if equal, `False` otherwise.

#### `LT` (0x1D)

Pop `b`, pop `a`. Push `True` if `a < b`, `False` otherwise.

### Bytes

#### `BYTES` (0x1E)

```
1E len:u8 data:[u8; len]
```

Heap-allocate a byte string from inline data, push the result.

#### `BYTES_LEN` (0x1F)

Pop a byte string, push its length as an integer.

#### `BYTES_GET` (0x20)

Pop index (integer), pop byte string. Push the byte at that index as an
integer. Errors with `IndexOutOfBounds` if out of range.

#### `BYTES_EQ` (0x21)

Pop two byte strings. Push `True` if equal, `False` otherwise.

#### `BYTES_CONCAT` (0x22)

Pop two byte strings. Heap-allocate and push their concatenation. Errors
with `BytesOverflow` if the combined length exceeds 255.

## Opcode summary

| Mnemonic | Hex | Operands | Size |
|---|---|---|---|
| `LOAD` | `0x01` | `idx:u8` | 2 |
| `LOAD2` | `0x02` | `idx_a:u8 idx_b:u8` | 3 |
| `LOAD3` | `0x03` | `idx_a:u8 idx_b:u8 idx_c:u8` | 4 |
| `GLOBAL` | `0x05` | `idx:u16le` | 3 |
| `DROP` | `0x06` | `n:u8` | 2 |
| `SLIDE` | `0x07` | `n:u8` | 2 |
| `PACK` | `0x08` | `tag:u8 arity:u8` | 3 |
| `UNPACK` | `0x09` | `n:u8` | 2 |
| `BIND` | `0x0A` | `n:u8` | 2 |
| `FOREIGN` | `0x0B` | `idx:u16le arity:u8` | 4 |
| `CLOSURE` | `0x0C` | `code_addr:u16le arity:u8 n_captures:u8` | 5 |
| `FIXPOINT` | `0x0D` | `cap_idx:u8` | 2 |
| `CALL_DYNAMIC` | `0x0E` | — | 1 |
| `TAIL_CALL_DYNAMIC` | `0x0F` | — | 1 |
| `CALL` | `0x10` | `code_addr:u16le n_args:u8` | 4 |
| `TAIL_CALL` | `0x11` | `code_addr:u16le n_args:u8` | 4 |
| `RET` | `0x12` | — | 1 |
| `MATCH` | `0x13` | `base_tag:u8 n:u8 [arity:u8 off:u16le]*n` | 3+3n |
| `JMP` | `0x14` | `offset:u16le` | 3 |
| `ERROR` | `0x15` | — | 1 |
| `INT` | `0x16` | `value:i32le` | 5 |
| `ADD` | `0x17` | — | 1 |
| `SUB` | `0x18` | — | 1 |
| `MUL` | `0x19` | — | 1 |
| `DIV` | `0x1A` | — | 1 |
| `NEG` | `0x1B` | — | 1 |
| `EQ` | `0x1C` | — | 1 |
| `LT` | `0x1D` | — | 1 |
| `BYTES` | `0x1E` | `len:u8 data:[u8;len]` | 2+len |
| `BYTES_LEN` | `0x1F` | — | 1 |
| `BYTES_GET` | `0x20` | — | 1 |
| `BYTES_EQ` | `0x21` | — | 1 |
| `BYTES_CONCAT` | `0x22` | — | 1 |
| `PACK0` | `0x23` | `tag:u8` | 2 |
| `INT0` | `0x24` | — | 1 |
| `INT1` | `0x25` | — | 1 |
| `SLIDE1` | `0x26` | — | 1 |
| `MATCH2` | `0x27` | `base_tag:u8 [arity:u8 off:u16le]*2` | 7 |
| `FUNCTION` | `0x28` | `code_addr:u16le arity:u8` | 4 |
| `DUP` | `0x29` | — | 1 |
| `OVER` | `0x2A` | — | 1 |

## Value kinds

Values are 32-bit words with a 3-bit kind tag in bits 31:29.

| Kind | Tag | Value word payload | Heap layout | Heap size |
|------|-----|--------------------|-------------|-----------|
| Ctor | `000` | `tag:8 \| offset:21` | `arity:u32, field[0], field[1], ...` | `(1 + arity) × 4` (0 if nullary) |
| Integer | `001` | `value:29` (sign-extended) | *(none)* | 0 |
| Bytes | `010` | `len:8 \| offset:21` | raw bytes, 4-aligned | `ceil(len/4) × 4` |
| *(unused)* | `011` | — | — | — |
| *(unused)* | `100` | — | — | — |
| *(unused)* | `101` | — | — | — |
| Closure | `110` | `offset:21` | `[code_addr:16\|arity:8\|n_bound:8], bound[0..n_bound-1]` | `(1+n_bound) × 4` |
| Function | `111` | `foreign:1 \| arity:4 \| addr:16` | *(none)* | 0 |

Callable detection: bit 31 == 1 (kinds `100`–`111`).

## Generated companion files

The compiler (both the CLI and `build.rs`) also emits a Rust source file
alongside `bytecode.bin`:

- **`bindings.rs`** — contains three modules:
  - `pub mod funcs` — one `pub const NAME: u16 = idx;` per global, mapping
    function names to their slot index.
  - `pub mod ctors` — one `pub const NAME: u8 = id;` per Scheme-defined
    constructor tag.
  - `pub mod foreign` — one `pub const NAME: u16 = idx;` per foreign function.

This is meant to be `include!`'d in the consuming crate so that call sites
use symbolic names (`funcs::APPEND`) rather than raw integers.
