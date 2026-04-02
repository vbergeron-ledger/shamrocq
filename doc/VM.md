# VM internals

This document describes the shamrocq runtime: how values are represented,
how memory is managed, and how the interpreter executes bytecode.

Source files: `crates/shamrocq/src/{arena,value,vm,gc,stats}.rs`,
`crates/shamrocq-bytecode/src/{op,tags,lib}.rs`.

## Value representation

Every runtime value is a single `u32` word. Bit 31 splits the encoding into
two families — **immediates** (no heap allocation) and **references** (heap
pointer):

```
 bit 31 = 0  →  Immediate
 ┌───┬────┬─────────────────────────────────────┐
 │ 0 │ 00 │ value:29                            │  Integer (sign-extended)
 │ 0 │ 01 │ tag:8 │ 0:21                        │  Nullary ctor
 │ 0 │ 10 │ foreign:1 │ arity:4 │ addr:16 │ 0:8 │  Function
 │ 0 │ 11 │ (reserved)                          │
 └───┴────┴─────────────────────────────────────┘

 bit 31 = 1  →  Reference
 ┌───┬────────┬──────────┐
 │ 1 │ tag:8  │ offset:23│  Ctor (tag 0x00..0xFD)
 │ 1 │ 0xFF   │ offset:23│  Closure
 │ 1 │ 0xFE   │ offset:23│  Bytes
 └───┴────────┴──────────┘
```

| Kind | Bit pattern | Meaning |
|------|-------------|---------|
| Integer | `0 00 …` | Signed 29-bit integer (+/- 268 M) |
| Nullary ctor | `0 01 tag:8 …` | Zero-field constructor (immediate, no heap) |
| Function | `0 10 …` | Zero-capture function — code address + arity packed inline |
| Foreign fn | `0 10 1 …` | Host-registered function — index + arity packed inline |
| Ctor | `1 tag offset` | Heap-allocated constructor with tag 0..253 |
| Closure | `1 0xFF offset` | Heap-allocated closure / partial application |
| Bytes | `1 0xFE offset` | Heap-allocated byte string |

Reference offsets are **word indices** into the `u32` arena buffer. With 23
bits this addresses up to 32 MB of heap.

Functions carry a 4-bit arity field and a 16-bit code address (or foreign
function index). The `foreign` flag bit distinguishes bytecode functions from
host callbacks.

### Hardcoded tags

The following constructor tags are shared between the compiler and the VM
(defined in `shamrocq_bytecode::tags`):

| Name | Value |
|------|-------|
| `TRUE`  | 0 |
| `FALSE` | 1 |

Additional tags are assigned at compile time.

## Arena

The VM does zero dynamic allocation. The caller provides a `&mut [u8]`
buffer; the arena reinterprets it as `&mut [u32]` and partitions it into
two regions that grow toward each other:

```
 0                              buf.len()
 ├──── heap ────►      ◄──── stack ────┤
 │ ctors, closures, bytes │   values (LIFO) │
 └──────────────────────────────────────┘
       heap_top ──┘    └── stack_bot
```

- **Heap** grows upward from offset 0. Bump-only: allocations are never
  individually freed. All allocations are word-aligned.
- **Stack** grows downward from the end. Each slot is one `u32` word.
- When `heap_top` meets `stack_bot` and the GC cannot free enough space →
  `Oom`.
- `arena.reset()` reclaims everything (sets `heap_top = 0`,
  `stack_bot = buf.len()`).

### Heap objects

Every heap object starts with a **GC header word**:

```
 31   30   29   28          13  12            0
┌────┬─────┬──────┬──────────┬────────────────┐
│ 0  │mark │opaque│ fwd:16   │   size:13      │
└────┴─────┴──────┴──────────┴────────────────┘
```

- `mark` — set during GC mark phase.
- `opaque` — if set, the object contains no reference fields (e.g. byte
  data); the GC skips scanning its interior.
- `fwd` — forwarding address used during compaction.
- `size` — total object size in words (including the header itself).

**Constructor** — GC header + N field words:

```
offset+0:  gc_header          (opaque=0, size = 1 + arity)
offset+1:  field_0            (raw Value u32)
offset+2:  field_1
  ...
offset+N:  field_{N-1}
```

The tag lives in the `Value` pointer, not on the heap. The GC header's
`size` field determines the number of fields without external metadata.
Nullary constructors (arity 0) are encoded as immediate values and never
touch the heap.

**Closure** — GC header + closure header + bound values:

```
offset+0:  gc_header          (opaque=0, size = 2 + n_bound)
offset+1:  closure_header     [code_addr:16 | arity:8 | n_bound:8]
offset+2:  bound_0            (raw Value u32)
offset+3:  bound_1
  ...
```

The `arity` field is the total number of values the function body expects on
the stack. The `n_bound` field tracks how many of those are already provided.
When `n_bound + 1 == arity`, a call is saturated: all bound values are pushed
onto the stack followed by the final argument, and execution jumps to
`code_addr`.

**Bytes** — GC header + length word + raw data:

```
offset+0:  gc_header          (opaque=1, size = 2 + ceil(len/4))
offset+1:  len                (u32, byte count)
offset+2:  raw data           (packed into words, ceil(len/4) words)
  ...
```

The `opaque` flag tells the GC to skip scanning the raw data words.

## Execution model

### Globals

A program has up to 64 global slots. On `load`, the VM evaluates each
global's initializer in declaration order and stores the result in a fixed
`[Value; 64]` array.

Most globals evaluate to closures or functions, but a global can be any value
(e.g. a constructor constant).

### Stack frames

Each function call establishes a frame on the arena stack. The frame header
and the argument/binding slots live in the same contiguous region. The stack
grows downward (toward lower word indices):

```
          higher addresses (toward buf end)
                     │ ... caller's frame ...   │
                     │ saved_heap               │  frame_base + 2
                     │ saved_pc                 │  frame_base + 1
                     │ saved_fb                 │  frame_base + 0  ← frame_base
                     ├──────────────────────────┤
                     │ slot 0 (bound/arg)       │  frame_base - 1
                     │ slot 1                   │  frame_base - 2
                     │ ...                      │
                     │ slot N (let bindings)     │  ← stack_bot (grows down)
          lower addresses (toward 0)
```

The frame header is 3 words, pushed before `frame_base` is set:

| Word | Contents |
|------|----------|
| `frame_base + 0` | Caller's `frame_base` |
| `frame_base + 1` | Return address (byte offset into code) |
| `frame_base + 2` | `heap_top` at the time of the call (for reclamation) |

`LOAD(idx)` reads `arena[frame_base - (idx + 1)]`. For closures, the bound
values (captures first, then any previously applied arguments) occupy the
lowest-indexed slots, followed by the fresh argument(s).

### Call mechanics

- **`CALL_DYNAMIC`**: pops `arg` and `func`. If the call is saturated, pushes
  a 3-word frame header, sets up a new frame with the closure's bound values
  followed by `arg`, and jumps to the code address. For undersaturated calls,
  extends the closure with the extra argument instead. Foreign functions with
  arity 1 are called directly without pushing a frame.
- **`TAIL_CALL_DYNAMIC`**: pops `arg` and `func`, truncates the current
  frame and reuses it — **no frame growth**, which is how tail recursion stays
  bounded. For undersaturated or foreign calls in tail position, performs a
  return through the current frame.
- **`CALL`**: N arguments are already on the stack and the target code address
  is statically known. Pushes a frame header, sets up the N arguments as
  slots, and jumps. Used for exact-arity calls to known globals.
- **`TAIL_CALL`**: tail-position variant of `CALL`. Reuses the current frame.
- **`RET`**: pops the result, attempts heap reclamation (see below), restores
  `frame_base` and `pc` from the header, and pushes the result in the
  caller's frame. At depth 0, returns to the Rust caller.

### Foreign functions

Up to 32 host functions can be registered via `vm.register_foreign(idx, f)`.
Each foreign function has the signature `fn(&mut Vm, Value) -> Result<Value,
VmError>` (single curried argument). The `FOREIGN` instruction pushes a
function value with the foreign bit set. When called, the VM invokes the Rust
callback directly — no bytecode frame is pushed.

### Frame-local heap reclamation

The `saved_heap` word in each frame header enables a lightweight form of
memory reclamation without the garbage collector.

On `RET` (and on `TAIL_CALL_DYNAMIC` when returning through a frame), the VM
checks whether the result references any heap memory allocated during this
call. If it does not, all heap memory in the range
`[saved_heap, current_heap_top)` is reclaimed by resetting `heap_top`.

The check (`try_reclaim`) inspects the result value: if it is a reference
whose offset falls within the new allocation range, reclamation is skipped.
Immediates (integers, nullary ctors, bare functions) never block reclamation.

This is sound because the language is pure: older heap objects never contain
pointers to newer allocations. The only mutation (`FIXPOINT`) patches a
closure within the current frame and cannot create a backward reference
across frames.

### Garbage collection

When available free space drops below a threshold (64 words), the VM triggers
a mark-and-compact garbage collection (Lisp-2 style):

1. **Mark** — trace all live roots (stack words, globals) and recursively
   mark all reachable heap objects via the GC header mark bit. Opaque objects
   (byte strings) are marked but not scanned.
2. **Compute forwarding** — scan the heap linearly; for each marked object,
   write its new destination into the GC header's `fwd` field.
3. **Update pointers** — rewrite every reference in the stack, globals, and
   heap object fields to reflect the forwarding addresses.
4. **Slide** — copy each marked object to its forwarding address, compacting
   the heap. Clear the mark and forwarding bits.

After compaction, `heap_top` is reset to the end of the last live object.
Frame-local reclamation continues to work alongside the GC — it handles the
common case cheaply, while the GC handles the rest.

### Tail call optimization

When the compiler sees an application in tail position, it emits
`TAIL_CALL_DYNAMIC` (or `TAIL_CALL`) instead of `CALL_DYNAMIC`. The VM
truncates the current frame (`set_stack_bot_pos(frame_base)`) and lays down
the new arguments in-place. Since no frame header is pushed, tail-recursive
loops use O(1) stack.

### Recursive closures (FIXPOINT)

`letrec` compiles to:

1. Push a dummy value (placeholder).
2. Compile the lambda — its captures include the `letrec` binding (de Bruijn
   index 0 from the lambda's perspective).
3. Emit `FIXPOINT(cap_idx)` — peeks TOS (a closure), patches
   `closure.bound[cap_idx]` to point to the closure itself, replaces the
   dummy slot below with the closure, and pops TOS.

When `cap_idx` is `0xFF`, the self-reference patching is skipped (the closure
does not capture itself). This is the **only mutable write** in the entire VM.

### Pattern matching

`MATCH` (or `MATCH2` for the 2-branch specialization) pops the scrutinee and
indexes a dense jump table by `scrutinee_tag - base_tag`. Each entry is
3 bytes: `arity:u8 offset:u16le`. When matched:

- If arity > 0, the scrutinee is re-pushed so `BIND(n)` or `UNPACK(n)` can
  destructure it into field bindings on the stack.
- If arity = 0, execution continues directly — no stack manipulation.
- After the branch body, `SLIDE(n)` removes the bindings while keeping the
  result (non-tail only), and `JMP` skips remaining cases.

In tail position, branches emit `RET` / `TAIL_CALL` directly — no `SLIDE`
or `JMP` needed.

Out-of-range tags produce a `MatchFailure` error.

## Instruction set

All opcodes are 1-byte, followed by fixed-size inline operands (little-endian
for multi-byte values).

### Stack / locals

| Opcode | Operands | Effect |
|--------|----------|--------|
| `LOAD` | `idx:u8` | Push `slot[idx]` from current frame |
| `LOAD2` | `a:u8 b:u8` | Push `slot[a]` then `slot[b]` |
| `LOAD3` | `a:u8 b:u8 c:u8` | Push three slots |
| `GLOBAL` | `idx:u16` | Push `globals[idx]` |
| `DROP` | `n:u8` | Discard top `n` stack slots |
| `SLIDE` | `n:u8` | Keep TOS, remove `n` slots below it |
| `SLIDE1` | — | `SLIDE` specialized for n=1 |
| `DUP` | — | Duplicate TOS |
| `OVER` | — | Copy the value below TOS to top |

### Data

| Opcode | Operands | Effect |
|--------|----------|--------|
| `PACK0` | `tag:u8` | Push nullary ctor (immediate) |
| `PACK` | `tag:u8 arity:u8` | Pop `arity` fields, allocate ctor on heap, push |
| `UNPACK` | `n:u8` | Pop ctor, push its first `n` fields |
| `BIND` | `n:u8` | Pop ctor, push its first `n` fields (alias of UNPACK semantics in match context) |
| `FUNCTION` | `addr:u16 arity:u8` | Push a zero-capture function value (immediate) |
| `CLOSURE` | `addr:u16 arity:u8 n_cap:u8` | Pop `n_cap` captures, allocate closure on heap, push |
| `FIXPOINT` | `cap_idx:u8` | Patch self-reference in TOS closure (see above) |
| `FOREIGN` | `idx:u16 arity:u8` | Push a foreign function value (immediate) |

### Control flow

| Opcode | Operands | Effect |
|--------|----------|--------|
| `CALL_DYNAMIC` | — | Pop arg + func, call (see call mechanics) |
| `TAIL_CALL_DYNAMIC` | — | Pop arg + func, tail call |
| `CALL` | `addr:u16 n:u8` | Call known function at `addr` with `n` args on stack |
| `TAIL_CALL` | `addr:u16 n:u8` | Tail call known function |
| `RET` | — | Return TOS to caller |
| `MATCH` | `base:u8 n:u8 table:[3]*n` | Pop scrutinee, dispatch via jump table |
| `MATCH2` | `base:u8 table:[3]*2` | `MATCH` specialized for 2 branches |
| `JMP` | `offset:u16` | Unconditional jump |
| `ERROR` | — | Raise `MatchFailure` |

### Integer arithmetic

| Opcode | Operands | Effect |
|--------|----------|--------|
| `INT` | `value:i32` | Push integer literal |
| `INT0` | — | Push `0` |
| `INT1` | — | Push `1` |
| `ADD` | — | Pop b, pop a, push a+b |
| `SUB` | — | Pop b, pop a, push a-b |
| `MUL` | — | Pop b, pop a, push a*b |
| `DIV` | — | Pop b, pop a, push a/b |
| `NEG` | — | Pop a, push -a |
| `EQ` | — | Pop b, pop a, push `TRUE` if a=b else `FALSE` |
| `LT` | — | Pop b, pop a, push `TRUE` if a<b else `FALSE` |

### Byte strings

| Opcode | Operands | Effect |
|--------|----------|--------|
| `BYTES` | `len:u8 data:[u8]` | Push byte string literal |
| `BYTES_LEN` | — | Pop bytes, push integer length |
| `BYTES_GET` | — | Pop index, pop bytes, push byte at index |
| `BYTES_EQ` | — | Pop two byte strings, push `TRUE`/`FALSE` |
| `BYTES_CONCAT` | — | Pop two byte strings, push concatenation |

## Bytecode format

A compiled program blob starts with a header (magic `SMRQ`, version 11):

```
magic         4 bytes    "SMRQ"
version       u16le      bytecode format version
n_globals     u16le      number of global definitions
globals       repeated   { name_len:u8, name:[u8], code_offset:u16le } × n_globals
n_tags        u16le      number of constructor tag names
tags          repeated   { name_len:u8, name:[u8] } × n_tags
code          [u8]       bytecode instructions (rest of blob)
```

## Rust API

### Setup

```rust
let mut buf = [0u8; 65536];
let prog = Program::from_blob(bytecode).unwrap();
let mut vm = Vm::new(&mut buf);
vm.load(&prog).unwrap();
```

### Registering foreign functions

```rust
vm.register_foreign(0, |vm, arg| {
    // process arg, return result
    Ok(Value::integer(arg.integer_value() + 1))
});
```

### Calling functions

```rust
// Call by global index:
let result = vm.call(funcs::ADD, &[n2, n3]).unwrap();

// Apply a closure value:
let negb_closure = vm.global_value(funcs::NEGB);
let result = vm.apply(negb_closure, &[val]).unwrap();
```

### Constructing and inspecting data

```rust
// Allocate a tagged constructor:
let pair = vm.alloc_ctor(tags::TRUE, &[]).unwrap(); // nullary
let cell = vm.alloc_ctor(3, &[head, tail]).unwrap(); // with fields

// Read fields:
let head = vm.ctor_field(cell, 0);
let tail = vm.ctor_field(cell, 1);

// Nullary constructors need no allocation:
let t = Value::nullary_ctor(tags::TRUE);

// Integers:
let n = Value::integer(42);
let x = n.integer_value();
```

### Memory management

```rust
// Snapshot current usage:
let snap = vm.mem_snapshot();
// -> "heap   1234 B | stack    456 B | free  63346 B"

// Reclaim all arena memory between computations:
vm.reset();
```

### State dump / restore

The VM can serialize its live state (heap, stack, globals) into a binary
dump (magic `SMRD`, version 1) via `vm.dump_into(&mut dst)`.

## Error modes

| Error | Cause |
|-------|-------|
| `Oom` | Heap allocation or stack push would overlap the other region, even after GC |
| `MatchFailure { scrutinee_tag, pc }` | No case in a `MATCH` table matches the scrutinee tag |
| `NotCallable` | `CALL` / `TAIL_CALL` target is not a function or closure |
| `IndexOutOfBounds` | Byte string index out of range |
| `BytesOverflow` | Byte string concatenation would exceed 255 bytes |
| `InvalidBytecode` | Blob too short, magic mismatch, PC out of bounds, or unknown opcode |
| `UnsupportedVersion { version }` | Bytecode version not recognized |
| `NotRegistered` | Foreign function slot has not been registered |

## Stats (feature `stats`)

When compiled with `--features stats`, the VM records:

| Counter | Description |
|---------|-------------|
| `peak_heap_bytes` | High-water mark of heap usage |
| `peak_stack_bytes` | High-water mark of stack usage |
| `alloc_count_ctor` | Total constructor allocations |
| `alloc_count_closure` | Total closure allocations |
| `alloc_bytes_total` | Total bytes allocated on the heap |
| `exec_instruction_count` | Total instructions executed |
| `exec_call_count` | Non-tail call count (`CALL_DYNAMIC` + `CALL`) |
| `exec_tail_call_count` | Tail call count (`TAIL_CALL_DYNAMIC` + `TAIL_CALL`) |
| `exec_match_count` | `MATCH` / `MATCH2` dispatch count |
| `exec_peak_call_depth` | Deepest call stack reached |
| `reclaim_count` | Number of frames where heap was reclaimed on return |
| `reclaim_bytes_total` | Total bytes reclaimed via frame-local reclamation |
| `gc_count` | Number of GC collections triggered |
| `gc_bytes_reclaimed` | Total bytes freed by GC compaction |

Access via `vm.stats` (the `Stats` struct) and `vm.mem_snapshot()` (live
heap/stack/free snapshot, available without the feature).
