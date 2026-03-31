I've read everything. Here's the harsh critique.

---

## The Verdict

You've built a clean, compact, `no_std` Scheme VM and that's respectable. But the performance story is, frankly, dire. The architecture has several structural problems that compound into what is essentially a "pay the maximum cost at every level" interpreter. Let me break it down.

---

## 1. The Killer: Fully Curried Calling Convention

This is your single biggest performance disaster. Every multi-argument call goes through the curried pipeline:

```254:262:crates/shamrocq-compiler/src/codegen.rs
            RExpr::App(func, arg) => {
                self.compile_expr(func, ctx, false);
                self.compile_expr(arg, ctx, false);
                if tail {
                    self.emitter.emit_tail_call();
                } else {
                    self.emitter.emit_call();
                }
            }
```

A call like `(f a b c)` desugars to `App(App(App(f, a), b), c)` which emits **three** CALL instructions. The first two create `Application` objects on the heap via `alloc_application` / `extend_application`. The third one finally realizes arity is saturated, then **copies all the args back from heap to stack**, and jumps. For a 3-arg function, you're doing:

- 2 heap allocations (never reclaimed)
- 2 header constructions with bitfield packing
- 1 full copy of accumulated args from heap to stack
- 3 opcode dispatches through the 130-line CALL handler

This means calling a function costs O(arity^2) work instead of O(arity). And you already know this -- the arity analysis pass is a skeleton:

```19:25:crates/shamrocq-compiler/src/pass/p07_arity_analysis.rs
    fn run(&self, defs: Vec<RDefine>) -> Vec<RDefine> {
        for d in &defs {
            let _arity = lambda_arity(&d.body);
        }
        defs
    }
```

It computes arities and throws them away. The comment literally says "prerequisite for future multi-argument APPLY/GRAB instructions." That future is now.

## 2. No GC, No Reclamation, No Hope for Long-Running Programs

The bump allocator never frees anything:

```33:41:crates/shamrocq/src/arena.rs
    pub fn alloc(&mut self, words: usize) -> Result<usize, ArenaError> {
        let base = Self::align4(self.heap_top);
        let end = base + words * 4;
        if end > self.stack_bot {
            return Err(ArenaError::OutOfMemory);
        }
        self.heap_top = end;
        Ok(base)
    }
```

Every partial application, every intermediate constructor, every `extend_application` copy -- all of it is permanent. `extend_application` is particularly wasteful: it allocates a fresh object and copies the old one every time an argument is added. For Coq-extracted code with deep pattern matching and recursive data structures, you will OOM quickly. The bump allocator is fine for a prototype, but it makes the curried calling convention even more punishing since every intermediate PAP is garbage that can never be collected.

## 3. MATCH Is a Linear Scan

```577:609:crates/shamrocq/src/vm.rs
                op::MATCH => {
                    let n_cases = code[pc] as usize;
                    // ...
                    for i in 0..n_cases {
                        let entry = table_start + i * 4;
                        let case_tag = code[entry];
                        // ...
                        if case_tag == scrutinee_tag {
                            // ...
                            break;
                        }
                    }
                }
```

O(n) linear scan on every match. Coq-extracted code is *drowning* in pattern matching. An `Inductive` with 20 constructors means 20 comparisons in the worst case, every time. Tags are `u8` (0..255), so a 256-entry jump table or even a sorted binary search would be trivial. For a VM that runs Coq extractions, this is inexcusable.

## 4. Dispatch Overhead: Classic Switch Threading

The main loop is a `match opcode { ... }` with 30+ arms. On each iteration:

1. Bounds check: `pc >= code.len()`
2. Fetch opcode byte
3. Jump through match/jump-table
4. Execute handler
5. Loop back to step 1

This is the slowest form of bytecode dispatch. The CPU branch predictor sees a single indirect branch site for all opcodes, so it can never predict the next handler. Every instruction pays the full pipeline flush cost. On ARM Cortex-M (your target), this is especially painful since the pipeline is short but branch misprediction still costs ~3-10 cycles per instruction.

## 5. Bounds Checking on Every Memory Access

`read_word` uses `try_into().unwrap()`:

```280:282:crates/shamrocq/src/arena.rs
    fn read_word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.buf[offset..offset + 4].try_into().unwrap())
    }
```

Every word read goes through: slice bounds check, `try_into` which checks length == 4, `unwrap` which has a panic path. `stack_push` checks `heap_top + 4` every time. The `pc >= code.len()` check runs on every instruction. You're paying for safety on every single operation in the hot loop.

## 6. Massive Code Duplication: CALL vs TAIL_CALL

`CALL` (lines 374-451) and `TAIL_CALL` (lines 453-557) share ~80% of their logic, copy-pasted. That's 180 lines of nearly-identical code in the hottest function in your program. This isn't just a maintenance problem -- it bloats the instruction cache footprint of `eval_with_frame`. On your Cortex-M targets with tiny I-caches (often 4-16KB), this matters a lot. The whole `eval_with_frame` function is ~490 lines of generated machine code competing for cache space.

## 7. record_heap / record_stack Noise

You call `self.record_heap()` and `self.record_stack()` after almost every operation. Without the `stats` feature, these are empty functions that the compiler *should* optimize away -- but the call sites add pressure on the optimizer and make the hot loop harder to read. More importantly, **with stats enabled** (your benchmark mode), you're computing `heap_used()` and `stack_used()` and doing a max-comparison on every single stack push and heap allocation. That's measuring overhead becoming part of what you measure.

## 8. O(n^2) Global Loading

```82:99:crates/shamrocq/src/vm.rs
    pub fn global_code_offset(&self, idx: u16) -> Result<u16, VmError> {
        let mut pos = 0usize;
        for i in 0..self.n_globals {
            // ... linear scan ...
        }
        Err(VmError::InvalidBytecode)
    }
```

`load_program` calls `global_code_offset(i)` for each global, and each call is O(n). Total: O(n^2). With 64 globals this is negligible, but it's sloppy.

---

## Performance Roadmap

### Tier 0: Biggest Bang for Zero Architectural Change

| Change | Impact | Effort |
|--------|--------|--------|
| **Jump-table MATCH** -- index a 256-byte table by tag instead of linear scan | High (matches are everywhere in Coq code) | Low |
| **Remove `pc` bounds check** from the hot loop (trust bytecode, validate at load time) | Medium (saves a branch per instruction) | Low |
| **`unsafe` aligned reads** for `read_word`/`write_word` on LE targets behind a `cfg` | Medium (eliminates bounds check + slice copy per memory op) | Low |
| **Factor CALL/TAIL_CALL** into shared helpers to shrink icache pressure | Medium | Low |

### Tier 1: Multi-Arg Calling Convention (The Big Win)

This is the highest-ROI change by far:

1. **Finish arity analysis** -- propagate known arities to call sites.
2. **Add `CALLN arity` / `TAIL_CALLN arity` opcodes** -- when the compiler knows a call site matches the callee's arity exactly, emit a single multi-arg call that pushes all args, sets up the frame, and jumps. Zero PAP allocations.
3. **Add `GRAB n` instruction** at function entry -- if the function receives fewer args than `n`, auto-build a PAP. This is how OCaml's ZAM and Coq's CertiCoq handle it.
4. **Over-application handling** -- when too many args are provided, call with the right arity, then apply the result to the remaining args.

Expected impact: eliminates ~90%+ of heap allocations for well-typed Coq-extracted code where arities are statically known. This alone could be a 3-10x speedup.

### Tier 2: Interpreter Dispatch

| Change | Impact | Effort |
|--------|--------|--------|
| **Tail-call threaded dispatch** -- each opcode handler tail-calls the next, giving the CPU per-opcode branch prediction | High (30-50% speedup on dispatch-bound code) | Medium |
| **Superinstructions** -- fuse common sequences (LOAD+CALL, LOAD+LOAD+CALL, PACK(0)+RET, MATCH+BIND, LOAD+MATCH) | Medium | Medium |
| **Inline caching for MATCH** -- speculate on the most recent tag | Medium for hot loops | Medium |

For Rust specifically, tail-call threading can be done with `musttail` (nightly) or by restructuring as a function-pointer table where each handler returns the next handler to call.

### Tier 3: Memory Management

| Change | Impact | Effort |
|--------|--------|--------|
| **Arena reset points** -- allow "sub-arena" regions that can be bulk-freed (e.g., per-call-frame temporary allocations) | High for long runs | Medium |
| **Copying/compacting GC** -- semi-space collector is simple and fits your single-buffer model (split buffer into two semi-spaces) | Critical for real workloads | High |
| **Stack-allocate small constructors** -- 0-2 field constructors used and discarded within a single frame don't need heap allocation | Medium | Medium |

### Tier 4: Aspirational

| Change | Impact | Effort |
|--------|--------|--------|
| **Register-based bytecode** instead of stack-based -- fewer push/pop, more direct operand addressing | High (20-40% fewer instructions) | Very High |
| **Template JIT** for hot paths -- even a trivial "copy native code snippets" JIT gives 2-5x | Very High | Very High |
| **NaN-boxing** on 64-bit host targets for testing/development (not embedded) | Medium | Medium |

---

## Recommended Priority Order

1. **Multi-arg calls** (Tier 1) -- this dominates everything else. Your arity analysis pass is already there waiting.
2. **Jump-table MATCH** (Tier 0) -- trivial to implement, immediate payoff.
3. **Unsafe word access** (Tier 0) -- remove the bounds-check tax from every memory operation.
4. **Factor CALL/TAIL_CALL** (Tier 0) -- reduce icache pressure, also makes Tier 1 easier.
5. **Arena reset points or simple GC** (Tier 3) -- without this, nothing else matters for real programs because you'll OOM.
6. **Threaded dispatch** (Tier 2) -- once the calling convention is fixed, dispatch overhead becomes the next bottleneck.

The multi-arg calling convention is the single change that would transform this from "proof of concept" to "competitive embedded VM." Everything else is polish by comparison.