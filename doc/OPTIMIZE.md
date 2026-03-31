# Performance analysis and roadmap

## Known issues

### 1. Fully Curried Calling Convention

Every multi-argument call goes through the curried pipeline. A call like
`(f a b c)` desugars to `App(App(App(f, a), b), c)` which emits three CALL
instructions. The first two create `Application` objects on the heap via
`alloc_application` / `extend_application`. The third one saturates the
arity, copies all the args back from heap to stack, and jumps. For a
3-argument function this means:

- 2 heap allocations (never reclaimed)
- 2 header constructions with bitfield packing
- 1 full copy of accumulated args from heap to stack
- 3 opcode dispatches through the CALL handler

Calling a function costs O(arity^2) work instead of O(arity). The arity
analysis pass (`p07_arity_analysis`) computes arities but doesn't use them
yet.

### 2. No GC

The bump allocator never frees anything. Every partial application, every
intermediate constructor, every `extend_application` copy is permanent.
For Coq-extracted code with deep recursion and data structures, this leads
to OOM.

### 3. Dispatch Overhead

The main loop is a `match opcode { ... }` with 30+ arms. The CPU branch
predictor sees a single indirect branch site for all opcodes, so it cannot
predict the next handler. Each iteration also checks `pc >= code.len()`.

### 4. Bounds Checking on Every Memory Access

`read_word` goes through `try_into().unwrap()` with slice bounds checks.
`stack_push` checks `heap_top + 4` every time. These are safe but costly
in the hot loop.

### 5. CALL / TAIL_CALL Code Duplication

CALL and TAIL_CALL share ~80% of their logic copy-pasted (~180 lines).
This bloats the instruction cache footprint of `eval_with_frame`.

## Done

- **Jump-table MATCH** (bytecode v3) -- MATCH uses a dense jump table
  indexed by `scrutinee_tag - base_tag`. O(1) dispatch instead of O(n)
  linear scan. Entry format is 3 bytes (arity + offset) instead of 4
  (tag + arity + offset). Gap entries point to an ERROR instruction.

- **CALL_N / TAIL_CALL_N -- Phase 1** (bytecode v4) -- When the compiler
  sees `App^N(Global(g), args)` where N equals the known arity of g, it
  emits all N arguments followed by `CALL_N flat_entry N`. The VM jumps
  directly to a flat entry point compiled with `frame_depth = arity`,
  bypassing the curried closure chain entirely. No PAP allocations, no
  intermediate CALL1 dispatches. Curried CALL1 remains for partial
  application, unknown callees, and arity-1 functions. CALL/TAIL_CALL
  renamed to CALL1/TAIL_CALL1.

---

## Roadmap

### Tier 0: Low Effort

| Change | Impact | Effort |
|--------|--------|--------|
| **Remove `pc` bounds check** from the hot loop (trust bytecode, validate at load time) | Medium | Low |
| **`unsafe` aligned reads** for `read_word`/`write_word` on LE targets behind a `cfg` | Medium | Low |
| **Factor CALL/TAIL_CALL** into shared helpers to shrink icache pressure | Medium | Low |

### Tier 1: Multi-Arg Calling Convention (CALL N / GRAB K)

Highest-ROI change. Modeled on the ZAM (Zinc Abstract Machine) used by
OCaml, adapted to shamrocq's stack-based arena.

#### Current situation

Frame header is 12 bytes: `[saved_fb, saved_pc, saved_env]`. CALL pops
one arg and one func, checks arity, either enters the callee or builds a
PAP on the heap. Multi-arg calls chain through N separate CALLs with
O(arity) heap allocations.

#### Proposed instructions

**`CALL N`** (caller side): N arguments and the callee are on the stack.

```
stack before:  [..., func, arg_0, arg_1, ..., arg_{N-1}]
```

CALL N saves arg_0..arg_{N-1}, pops them and func, pushes the return
frame `[saved_fb, saved_pc, saved_env, N]` (16 bytes -- the extra word
stores the arg count so the callee can inspect it), sets frame_base,
re-pushes the N args, and jumps to the callee's code address.

**`TAIL_CALL N`**: same but reuses the current frame (truncate to
frame_base, re-push args, jump). No frame growth.

**`GRAB K`** (callee side): placed at the entry of a function that
expects K arguments. Reads N (the arg count provided by CALL N) from the
frame header and dispatches:

| Condition | Behavior |
|-----------|----------|
| N == K (exact) | Continue into the body. All K args are locals 0..K-1. |
| N < K (under-applied) | Build a PAP capturing the closure + the N args already provided. Push the PAP as the result and execute RET. The function body is never entered. |
| N > K (over-applied) | Consume K args as locals. Store the remaining N-K as "extra args" in the frame. The body runs normally; RET handles the rest (see below). |

#### RET behavior in each case

**Exact application (N == K):** No change. RET pops the result, restores
the saved frame header, pushes the result in the caller's frame.

**Under-application (N < K):** GRAB itself builds the PAP and falls
through to RET. RET sees a normal return -- it doesn't know or care that
GRAB short-circuited. No change to RET.

**Over-application (N > K):** The function body produces a result that is
itself callable. RET checks the extra-args count in the frame header. If
extra_args > 0, instead of returning to the caller, RET treats the result
as a new callee: it pops the K consumed args, and re-enters CALL with the
remaining extra args still on the stack. In practice this means RET
becomes:

```
result = pop()
extra = frame_header.extra_args
if extra == 0:
    normal return (restore caller frame, push result)
else:
    // The extra args are still below frame_base.
    // Re-dispatch: apply result to the next arg.
    // Either loop (if result has arity 1) or GRAB again.
```

This is the only place where RET gets more complex. The extra-args count
can be stored in the upper 8 bits of the saved_pc word (which only uses
16 bits for the code address), avoiding a 4th frame-header word:

```
frame header:  [saved_fb:u32, (extra:u8 << 16 | saved_pc:u16):u32, saved_env:u32]
```

This keeps the frame header at 12 bytes.

#### Incrementality

The design is deliberately layered so each phase is independently useful:

**Phase 1 -- Exact-arity known calls (compiler only, no GRAB).**
When the compiler sees `App^N(Global(g), args)` and N equals the known
arity of g, it emits all N args followed by `CALL N addr`. The VM handler
pushes the frame and jumps. No GRAB instruction needed -- the compiler
guarantees N == K. Under-application and over-application fall back to the
existing curried CALL. This alone eliminates the vast majority of PAP
allocations in Coq-extracted code, since most calls are to known globals
at exact arity.

**Phase 2 -- GRAB for unknown callees.**
Add GRAB K at the entry of all multi-arity functions. CALL N now works
with closures and higher-order calls, not just known globals. GRAB handles
under-application (builds PAP) so the curried CALL path is no longer
needed for that case.

**Phase 3 -- Over-application in RET.**
Extend the frame header with extra_args. RET checks extra_args and
re-dispatches. This handles cases like `(compose f g x)` where compose
has arity 2 but is called with 3 args.

Phase 1 is the high-value target. Phases 2 and 3 are correctness
refinements for edge cases in higher-order code.

#### Compiler changes per phase

Phase 1 requires:
- Propagate arity info from `p07_arity_analysis` to codegen.
- In `compile_expr` for `App`, walk the App spine to detect
  `App^N(Global(g), ...)` where N == arity(g).
- Emit `LOAD arg_0; ...; LOAD arg_{N-1}; CALL_N code_addr N` instead
  of the chain of `LOAD func; LOAD arg; CALL` pairs.
- Emit a flat body for multi-arity globals (frame_depth = arity, no
  captures, de Bruijn indices map directly to LOAD slots).
- Non-matching call sites are unchanged.

Phase 2 requires:
- Emit GRAB K at the start of every multi-arity function body.
- CALL N works with any callable, not just known code addresses.

Phase 3 requires:
- Pack extra_args into the frame header.
- Modify RET to check and re-dispatch.

#### Foreign functions and CALL N

The current `ForeignFn` signature is:

```rust
pub type ForeignFn = fn(&mut Vm<'_>, Value) -> Result<Value, VmError>;
```

This takes a single curried argument. Three options for multi-arg foreign
calls:

**Option A: Keep curried FFI (recommended for Phase 1).** Foreign
functions remain arity-1 from the VM's perspective. If Scheme code calls
a multi-arg foreign function, it goes through the normal curried path.
Foreign functions are rare and cheap (they run native Rust), so the
overhead is negligible. No FFI change needed.

**Option B: Stack-based FFI.** Change the signature to
`fn(&mut Vm<'_>, n_args: u8) -> Result<Value, VmError>`. The function
reads its arguments from the VM stack. This is more powerful but changes
the host API and requires all existing foreign functions to be rewritten.

**Option C: Variadic FFI.** Keep the single-arg signature for arity-1
functions. Add a second registration path for multi-arg functions that
takes `fn(&mut Vm<'_>, &[Value]) -> Result<Value, VmError>`. The VM
dispatches based on the registered arity. This is the most flexible but
adds complexity.

Option A is the right default. Foreign functions are glue code (I/O,
hardware access) that typically take one boxed argument. If profiling
later shows a hot multi-arg foreign function, Option B can be added as a
separate optimization without affecting the core calling convention.

### Tier 2: Interpreter Dispatch

| Change | Impact | Effort |
|--------|--------|--------|
| **Tail-call threaded dispatch** | High | Medium |
| **Superinstructions** (LOAD+CALL, PACK(0)+RET, MATCH+BIND, etc.) | Medium | Medium |

### Tier 3: Memory Management

| Change | Impact | Effort |
|--------|--------|--------|
| **Arena reset points** -- bulk-free per-call-frame temporaries | High for long runs | Medium |
| **Copying/compacting GC** -- semi-space collector fits the single-buffer model | Critical for real workloads | High |

### Tier 4: Aspirational

| Change | Impact | Effort |
|--------|--------|--------|
| **Register-based bytecode** | High | Very High |
| **Template JIT** | Very High | Very High |
| **NaN-boxing** on 64-bit host targets | Medium | Medium |

---

## Priority order

1. **Multi-arg calls** (Tier 1)
2. **Unsafe word access** (Tier 0)
3. **Factor CALL/TAIL_CALL** (Tier 0)
4. **Arena reset points or GC** (Tier 3)
5. **Threaded dispatch** (Tier 2)