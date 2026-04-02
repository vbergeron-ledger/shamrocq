# VM internals

This document describes the shamrocq runtime: how values are represented,
how memory is managed, and how the interpreter executes bytecode.

Source files: `crates/shamrocq/src/{arena,value,vm,stats}.rs`.

## Value representation

Every runtime value is a single `u32` word, split into three bit-fields:

```
 31  29  28        21  20                   0
┌──────┬────────────┬─────────────────────────┐
│ kind │    tag     │       payload           │
│  3b  │    8b     │        21b              │
└──────┴────────────┴─────────────────────────┘
```

| Kind | Bits 31:29 | Meaning |
|------|------------|---------|
| Ctor | `000` | Constructor — 8-bit tag, 21-bit word offset (0 for nullary) |
| Integer | `001` | Signed integer — tag bits extend payload to 29 bits |
| Bytes | `010` | Byte string — 8-bit length, 21-bit word offset to data |
| Closure | `110` | Heap-allocated closure / partial application — 21-bit word offset |
| Bare fn | `111` | Zero-capture function — code address in payload, no heap |

Offsets are stored as **word indices** (byte offset / 4). With 21 bits this
addresses up to 8 MB of heap, far beyond the target buffer sizes.

Integers use the 8 tag bits as an extension of the 21-bit payload, giving a
29-bit signed range (+/-268M).

This encoding keeps values register-sized and avoids pointer tagging tricks
that would break on non-32-bit targets.

### Hardcoded tags

The following tags are shared between the compiler and the VM and are always
available (defined in `value::tags`):

| Name | Value | Arity |
|------|-------|-------|
| `TRUE`  | 0 | 0 |
| `FALSE` | 1 | 0 |
| `NIL`   | 2 | 0 |
| `CONS`  | 3 | 2 |
| `O`     | 4 | 0 |
| `S`     | 5 | 1 |
| `LEFT`  | 6 | 1 |
| `RIGHT` | 7 | 1 |
| `PAIR`  | 8 | 2 |

Additional tags are assigned at compile time by the resolver's `TagTable`
and emitted in the `ctors` module inside `bindings.rs`.

## Arena

The VM does zero dynamic allocation.  The caller provides a `&mut [u8]`
buffer; the arena partitions it into two regions that grow toward each other:

```
 0                              buf.len()
 ├──── heap ────►      ◄──── stack ────┤
 │ constructors, closures    │  values (LIFO) │
 └──────────────────────────────────────┘
       heap_top ──┘    └── stack_bot
```

- **Heap** grows upward from offset 0.  Bump-only: allocations are never
  individually freed.  All allocations are 4-byte aligned.
- **Stack** grows downward from the end.  Each slot is one `u32` word (4 bytes).
- When `heap_top` meets `stack_bot` → `OutOfMemory`.
- `arena.reset()` reclaims everything (sets `heap_top = 0`,
  `stack_bot = buf.len()`).

### Heap objects

**Constructor** — arity header word + N field words:

```
offset+0:  arity (u32, low 8 bits used)
offset+4:  field_0 (raw Value u32)
offset+8:  field_1
  ...
offset+4*N: field_{N-1}
```

The tag lives in the `Value` pointer, not on the heap.  The arity header
makes the heap self-describing: `ctor_arity(val)` reads it to determine
the number of fields without external metadata.  Nullary constructors
(arity 0) are encoded as immediate values and never touch the heap.

**Closure** — header word + bound values (captures and/or applied arguments):

```
offset+0:  code_addr:u16 | arity:u8 | n_bound:u8
offset+4:  bound_0 (raw Value u32)
offset+8:  bound_1
  ...
```

Closures unify what were previously two separate kinds (Closure + Application).
The `arity` field is the total number of values the function body expects on
the stack (captures + parameters).  The `n_bound` field tracks how many of
those are already provided.  When `n_bound + 1 == arity`, a call is saturated:
all bound values are pushed onto the stack followed by the final argument,
and execution jumps to `code_addr`.

## Execution model

### Globals

A program has up to 64 global slots.  On `load`, the VM evaluates
each global's code in declaration order (executing its initializer expression)
and stores the result in a fixed `[Value; 64]` array.

Most globals evaluate to closures, but a global can be any value (e.g. a
constructor constant).

### Stack frames

Each function call establishes a frame on the arena stack.  The frame header
and the operand slots live in the same contiguous region:

```
                     ┌──────────────┐
                     │ saved_fb     │  frame_base + 0
                     │ saved_pc     │  frame_base + 4
                     │ saved_heap   │  frame_base + 8
 frame_base + 12 ──► ├──────────────┤
                     │ bound_0      │  slot 0  (capture or applied arg)
                     │ ...          │
                     │ bound_{N-1}  │  slot N-1
                     │ arg          │  slot N
                     │ let_bind_0   │  slot N+1
                     │   ...        │    (grows with BIND / let)
                     └──────────────┘  ◄── stack top
```

The frame header is 12 bytes (3 words):

| Word | Contents |
|------|----------|
| `saved_fb` | Caller's `frame_base` |
| `saved_pc` | Return address (byte offset into code) |
| `saved_heap` | `heap_top` at the time of the call (for reclamation) |

`LOAD(idx)` reads slot `idx` counting from the first slot after the header.
For closures, the bound values (captures first, then any previously applied
arguments) occupy the lowest slots, followed by the fresh argument(s).

### Call mechanics

- `CALL_DYNAMIC`: pops `arg` and `func`, pushes a 3-word frame header, sets up a
  new frame with the closure's bound values followed by `arg`, jumps to the
  code address.  For undersaturated calls, extends the closure instead.
- `TAIL_CALL_DYNAMIC`: pops `arg` and `func`, truncates the current frame and
  reuses it — **no frame growth**, which is how tail recursion stays bounded.
- `CALL`: N arguments are already on the stack and the target code address
  is statically known.  Pushes a frame header, sets up the N arguments as
  slots, and jumps.  Used for exact-arity calls to known globals.
- `TAIL_CALL`: tail-position variant of `CALL`.  Reuses the current frame.
- `RET`: pops the result, attempts heap reclamation (see below), restores
  `frame_base` and `pc` from the header, and pushes the result in
  the caller's frame.  At depth 0, returns to the Rust caller.

For closures, the bound values (captures and any previously applied
arguments) are pushed as the first stack slots in the new frame, accessible
via `LOAD`.  For foreign functions, the host Rust function is called
directly — no bytecode frame pushed.

### Frame-local heap reclamation

The `saved_heap` word in each frame header enables a lightweight form of
memory reclamation without a garbage collector.

On `RET` (and on `TAIL_CALL_DYNAMIC` when returning through a frame), the VM
checks whether the result references any heap memory allocated during this
call.  If it does not, all heap memory in the range
`[saved_heap, current_heap_top)` is reclaimed by resetting `heap_top`.

The check (`references_heap_since`) inspects the result value's kind:

- Integers and bare functions have no heap component → always safe.
- Ctors, closures, and byte strings → safe if their offset is below
  `saved_heap` (i.e. they were allocated before this call).

This is sound because the language is pure: older heap objects never contain
pointers to newer allocations.  The only mutation (`FIXPOINT`) patches a
closure within the current frame and cannot create a backward reference
across frames.

### Garbage collection

When a bump allocation would exhaust the arena, the VM triggers a
mark-and-compact garbage collection:

1. **Mark** — trace all live roots (stack, globals, frame headers) and
   recursively mark all reachable heap objects.
2. **Compact** — slide live objects toward the bottom of the heap, closing
   gaps left by dead objects.  All pointers (in stack slots, globals, and
   heap object fields) are updated in place to reflect the new positions.

After compaction, `heap_top` is reset to the end of the last live object,
freeing the rest of the heap for new allocations.  Frame-local reclamation
continues to work alongside the GC — it handles the common case cheaply,
while the GC handles the rest.

### Tail call optimization

When the compiler sees an application in tail position, it emits
`TAIL_CALL_DYNAMIC` (or `TAIL_CALL`) instead of `CALL_DYNAMIC`.  The VM truncates the
current frame (`set_stack_bot_pos(frame_base)`) and
lays down the new arguments in-place.  Since no frame header is pushed,
tail-recursive loops use O(1) stack.

### Recursive closures (FIXPOINT)

`letrec` compiles to:

1. Push a dummy value (placeholder).
2. Compile the lambda — its captures include the `letrec` binding (de Bruijn
   index 0 from the lambda's perspective).
3. Emit `FIXPOINT(cap_idx)` — mutates `closure.captures[cap_idx]` to point
   to the closure itself, then replaces the dummy slot.

This is the **only mutable write** in the entire VM.  It avoids the need for
an indirection cell or trampoline.

### Pattern matching

`MATCH` pops the scrutinee and indexes a dense jump table by
`scrutinee_tag - base_tag`.  When matched:

- If arity > 0, the scrutinee is re-pushed, then `BIND(n)` destructures it
  into `n` field bindings on the stack.
- If arity = 0, execution continues directly — no stack manipulation.
- After the branch body, `SLIDE(n)` removes the bindings while keeping the
  result (non-tail only), and `JMP` skips remaining cases.

In tail position, branches emit `RET` / `TAIL_CALL` directly — no `SLIDE`
or `JMP` needed.

Gap entries (tags in range but not matched) point to an `ERROR` instruction.

## Rust API

### Setup

```rust
let mut buf = [0u8; 65536];
let prog = Program::from_blob(bytecode).unwrap();
let mut vm = Vm::new(&mut buf);
vm.load(&prog).unwrap();
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
// Allocate a tagged constructor (e.g. cons cell):
let pair = vm.alloc_ctor(ctors::CONS, &[head, tail]).unwrap();

// Read fields:
let head = vm.ctor_field(pair, 0);
let tail = vm.ctor_field(pair, 1);

// Read arity from the heap header:
let n = vm.ctor_arity(pair);  // 2

// Nullary constructors need no allocation:
let nil = Value::ctor(tags::NIL, 0);

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

## Error modes

| Error | Cause |
|-------|-------|
| `Oom` | Heap allocation or stack push would overlap the other region |
| `StackOverflow` | Call depth exceeds arena capacity |
| `MatchFailure { tag, pc }` | No case in a `MATCH` table matches the scrutinee tag |
| `NotAClosure` | `CALL` / `TAIL_CALL` target is not callable |
| `IndexOutOfBounds` | Byte string index out of range |
| `BytesOverflow` | Byte string concatenation would exceed 255 bytes |
| `InvalidBytecode` | Blob too short, PC out of bounds, or malformed header |

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
| `exec_match_count` | `MATCH` dispatch count |
| `exec_peak_call_depth` | Deepest call stack reached |
| `reclaim_count` | Number of frames where heap was reclaimed on return |
| `reclaim_bytes_total` | Total bytes reclaimed via frame-local reclamation |
| `gc_collection_count` | Number of GC collections triggered |
| `gc_bytes_freed` | Total bytes freed by GC compaction |

Access via `vm.stats` (the `Stats` struct) and `vm.mem_snapshot()` (live
heap/stack/free snapshot, available without the feature).
