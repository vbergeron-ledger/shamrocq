use crate::arena::{Arena, FrameHeader};
use crate::bytes;
use crate::program::{Program, VmError};
use crate::stats::stat;
#[cfg(feature = "stats")]
use crate::stats::ExecStats;
use crate::stats::MemSnapshot;
use crate::value::Value;
use shamrocq_bytecode::op;
use shamrocq_bytecode::{DUMP_MAGIC, DUMP_VERSION};

/// A host function callable from Scheme. Takes a single (curried) argument.
pub type ForeignFn = fn(&mut Vm<'_>, Value) -> Result<Value, VmError>;

const MAX_FOREIGN_FNS: usize = 32;

fn unregistered_foreign_fn(_: &mut Vm<'_>, _: Value) -> Result<Value, VmError> {
    Err(VmError::NotRegistered)
}

pub struct Vm<'buf> {
    pub arena: Arena<'buf>,
    globals: [Value; 64],
    n_globals: u16,
    code: &'buf [u8],
    foreign_fns: [ForeignFn; MAX_FOREIGN_FNS],
    staging: [Value; 16],
    #[cfg(feature = "stats")]
    pub stats: ExecStats,
    #[cfg(feature = "stats")]
    cycle_reader: fn() -> u32,
}

impl<'buf> Vm<'buf> {
    pub fn new(buf: &'buf mut [u8]) -> Self {
        Vm {
            arena: Arena::from_bytes(buf),
            globals: [Value::integer(0); 64],
            n_globals: 0,
            code: &[],
            foreign_fns: [unregistered_foreign_fn; MAX_FOREIGN_FNS],
            staging: [Value::ZERO; 16],
            #[cfg(feature = "stats")]
            stats: ExecStats::default(),
            #[cfg(feature = "stats")]
            cycle_reader: || 0,
        }
    }

    #[cfg(feature = "stats")]
    pub fn combined_stats(&self) -> crate::stats::Stats {
        crate::stats::Stats::from(&self.arena.stats, &self.stats)
    }

    #[cfg(feature = "stats")]
    pub fn set_cycle_reader(&mut self, f: fn() -> u32) {
        self.cycle_reader = f;
    }

    pub fn register_foreign(&mut self, idx: u16, f: ForeignFn) {
        self.foreign_fns[idx as usize] = f
    }

    pub fn reset(&mut self) {
        self.arena.reset();
    }

    pub fn mem_snapshot(&self) -> MemSnapshot {
        MemSnapshot {
            heap_bytes: self.arena.heap_used() * 4,
            stack_bytes: self.arena.stack_used() * 4,
            free_bytes: self.arena.free() * 4,
        }
    }

    pub fn bytes_len(&self, val: Value) -> usize {
        self.arena.bytes_len(val)
    }

    fn run_gc(&mut self) {
        let n = self.n_globals as usize;
        self.arena.collect_garbage(&mut self.globals[..n]);
    }

    /// Write a binary dump of the VM state into `dst`.
    ///
    /// Format (all little-endian):
    ///   magic       4 bytes   "SMRD"
    ///   version     u16       dump format version
    ///   buf_len     u32       total arena buffer size (bytes)
    ///   heap_top    u32       heap high-water mark (bytes)
    ///   stack_bot   u32       stack bottom position (bytes)
    ///   n_globals   u16       number of active globals
    ///   globals     n_globals * u32   raw Value words
    ///   heap        heap_top bytes
    ///   stack       (buf_len - stack_bot) bytes
    pub fn dump_into(&self, dst: &mut [u8]) -> Option<usize> {
        let heap = bytes::words_as_bytes(self.arena.heap_data());
        let stack = bytes::words_as_bytes(self.arena.stack_data());
        let header = 4 + 2 + 4 + 4 + 4 + 2 + (self.n_globals as usize) * 4;
        let total = header + heap.len() + stack.len();
        if dst.len() < total {
            return None;
        }
        let mut pos = 0;

        dst[pos..pos + 4].copy_from_slice(&DUMP_MAGIC);
        pos += 4;
        dst[pos..pos + 2].copy_from_slice(&DUMP_VERSION.to_le_bytes());
        pos += 2;
        dst[pos..pos + 4].copy_from_slice(&(self.arena.buf_len() as u32 * 4).to_le_bytes());
        pos += 4;
        dst[pos..pos + 4].copy_from_slice(&(self.arena.heap_used() as u32 * 4).to_le_bytes());
        pos += 4;
        dst[pos..pos + 4].copy_from_slice(&(self.arena.stack_bot_pos() as u32 * 4).to_le_bytes());
        pos += 4;
        dst[pos..pos + 2].copy_from_slice(&self.n_globals.to_le_bytes());
        pos += 2;

        for i in 0..self.n_globals as usize {
            dst[pos..pos + 4].copy_from_slice(&self.globals[i].raw().to_le_bytes());
            pos += 4;
        }

        dst[pos..pos + heap.len()].copy_from_slice(heap);
        pos += heap.len();
        dst[pos..pos + stack.len()].copy_from_slice(stack);
        pos += stack.len();

        Some(pos)
    }

    pub fn load(&mut self, prog: &Program<'buf>) -> Result<(), VmError> {
        self.n_globals = prog.n_globals;
        self.code = prog.code;
        for i in 0..prog.n_globals {
            let offset = prog.global_code_offset(i)?;
            let fb = self.arena.stack_bot_pos();
            let val = self.eval(offset as usize, fb)?;
            self.globals[i as usize] = val;
        }
        Ok(())
    }

    pub fn call(&mut self, global_idx: u16, args: &[Value]) -> Result<Value, VmError> {
        self.apply(self.globals[global_idx as usize], args)
    }

    pub fn global_value(&self, idx: u16) -> Value {
        self.globals[idx as usize]
    }

    pub fn apply(&mut self, mut func: Value, args: &[Value]) -> Result<Value, VmError> {
        for &arg in args {
            if !func.is_callable() {
                return Err(VmError::NotCallable);
            }

            if func.is_foreign_fn() && func.fn_arity() == 1 {
                let f = self.foreign_fns[func.fn_addr() as usize];
                func = f(self, arg)?;
            } else if func.is_function() {
                let arity = func.fn_arity();
                if arity == 1 {
                    let saved_pos = self.arena.stack_bot_pos();
                    self.arena.stack_push(arg)?;
                    func = self.eval(func.fn_addr() as usize, saved_pos)?;
                } else {
                    func = self.arena.alloc_closure(func.fn_addr(), arity, &[arg])?;
                }
            } else {
                let bound = self.arena.closure_bound_count(func);
                let arity = self.arena.closure_arity(func) as usize;
                if bound + 1 < arity {
                    func = self.arena.extend_closure(func, arg)?;
                } else {
                    let saved_pos = self.arena.stack_bot_pos();
                    let code_addr = self.arena.closure_code(func);
                    for i in 0..bound {
                        let v = self.arena.closure_bound(func, i);
                        self.arena.stack_push(v)?;
                    }
                    self.arena.stack_push(arg)?;
                    func = self.eval(code_addr as usize, saved_pos)?;
                }
            }
        }
        Ok(func)
    }

    pub fn ctor_field(&self, val: Value, idx: usize) -> Value {
        self.arena.ctor_field(val, idx)
    }

    pub fn alloc_ctor(&mut self, tag: u8, fields: &[Value]) -> Result<Value, VmError> {
        match self.arena.alloc_ctor(tag, fields) {
            Ok(v) => Ok(v),
            Err(_) => {
                self.run_gc();
                Ok(self.arena.alloc_ctor(tag, fields)?)
            }
        }
    }

    fn ensure_gc_headroom(&mut self) -> Result<(), VmError> {
        const GC_THRESHOLD: usize = 64;
        if self.arena.free() < GC_THRESHOLD {
            self.run_gc();
            if self.arena.free() < GC_THRESHOLD {
                return Err(VmError::Oom);
            }
        }
        Ok(())
    }

    fn eval(&mut self, pc: usize, frame_base: usize) -> Result<Value, VmError> {
        let code = self.code;
        let mut hdr = FrameHeader { pc, frame_base };
        let mut call_depth: usize = 0;

        loop {
            if hdr.pc >= code.len() {
                return Err(VmError::InvalidBytecode);
            }

            let opcode = code[hdr.pc];
            hdr.pc += 1;

            #[cfg(feature = "stats")]
            {
                self.stats.opcode_counts[opcode as usize] += 1;
            }
            #[cfg(feature = "stats")]
            let heap_before = self.arena.heap_used();
            #[cfg(feature = "stats")]
            let cyc_before = (self.cycle_reader)();

            match opcode {
                op::PACK0 => {
                    let tag = code[hdr.pc];
                    self.arena.stack_push(Value::nullary_ctor(tag))?;
                    hdr.pc += 1;
                }

                op::PACK => {
                    self.ensure_gc_headroom()?;
                    let tag = code[hdr.pc];
                    let arity = code[hdr.pc + 1] as usize;
                    let val = self.arena.alloc_ctor_from_stack(tag, arity)?;
                    self.arena.stack_push(val)?;
                    hdr.pc += 2;
                }

                op::UNPACK => {
                    let n = code[hdr.pc] as usize;
                    let scrutinee = self.arena.stack_pop();
                    for i in 0..n {
                        let field = self.arena.ctor_field(scrutinee, i);
                        self.arena.stack_push(field)?;
                    }
                    hdr.pc += 1;
                }

                op::LOAD => {
                    let idx = code[hdr.pc] as usize;
                    let val = self.arena.stack_frame_load(&hdr, idx);
                    self.arena.stack_push(val)?;
                    hdr.pc += 1;
                }

                op::LOAD2 => {
                    let idx_a = code[hdr.pc] as usize;
                    let idx_b = code[hdr.pc + 1] as usize;
                    let a = self.arena.stack_frame_load(&hdr, idx_a);
                    let b = self.arena.stack_frame_load(&hdr, idx_b);
                    self.arena.stack_reserve(2)?;
                    self.arena.stack_push_unchecked(a);
                    self.arena.stack_push_unchecked(b);
                    hdr.pc += 2;
                }

                op::LOAD3 => {
                    let idx_a = code[hdr.pc] as usize;
                    let idx_b = code[hdr.pc + 1] as usize;
                    let idx_c = code[hdr.pc + 2] as usize;
                    let a = self.arena.stack_frame_load(&hdr, idx_a);
                    let b = self.arena.stack_frame_load(&hdr, idx_b);
                    let c = self.arena.stack_frame_load(&hdr, idx_c);
                    self.arena.stack_reserve(3)?;
                    self.arena.stack_push_unchecked(a);
                    self.arena.stack_push_unchecked(b);
                    self.arena.stack_push_unchecked(c);
                    hdr.pc += 3;
                }

                op::GLOBAL => {
                    let idx = u16::from_le_bytes([code[hdr.pc], code[hdr.pc + 1]]) as usize;
                    self.arena.stack_push(self.globals[idx])?;
                    hdr.pc += 2;
                }

                op::FUNCTION => {
                    let code_addr = u16::from_le_bytes([code[hdr.pc], code[hdr.pc + 1]]);
                    let arity = code[hdr.pc + 2];
                    self.arena.stack_push(Value::function(code_addr, arity))?;
                    hdr.pc += 3;
                }

                op::CLOSURE => {
                    self.ensure_gc_headroom()?;
                    let code_addr = u16::from_le_bytes([code[hdr.pc], code[hdr.pc + 1]]);
                    let arity = code[hdr.pc + 2];
                    let n_cap = code[hdr.pc + 3] as usize;
                    let val = self.arena.alloc_closure_from_stack(code_addr, arity, n_cap)?;
                    self.arena.stack_push(val)?;
                    hdr.pc += 4;
                }

                op::CALL_DYNAMIC => {
                    self.ensure_gc_headroom()?;
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();

                    if func.is_foreign_fn() && func.fn_arity() == 1 {
                        let f = self.foreign_fns[func.fn_addr() as usize];
                        let result = f(self, arg)?;
                        self.arena.stack_push(result)?;
                    } else if func.is_function() {
                        let arity = func.fn_arity();
                        if arity == 1 {
                            self.arena.stack_frame_push(&hdr)?;
                            call_depth += 1;
                            hdr.frame_base = self.arena.stack_bot_pos();
                            self.arena.stack_push(arg)?;
                            hdr.pc = func.fn_addr() as usize;
                            stat!(self, peak_call_depth = max call_depth as u32);
                        } else {
                            let cl = self.arena.alloc_closure(func.fn_addr(), arity, &[arg])?;
                            self.arena.stack_push(cl)?;
                        }
                    } else if func.is_closure() {
                        let bound = self.arena.closure_bound_count(func);
                        let arity = self.arena.closure_arity(func) as usize;
                        if bound + 1 < arity {
                            let cl = self.arena.extend_closure(func, arg)?;
                            self.arena.stack_push(cl)?;
                        } else {
                            let code_addr = self.arena.closure_code(func);
                            self.arena.stack_frame_push(&hdr)?;
                            call_depth += 1;
                            hdr.frame_base = self.arena.stack_bot_pos();
                            for i in 0..bound {
                                let v = self.arena.closure_bound(func, i);
                                self.arena.stack_push(v)?;
                            }
                            self.arena.stack_push(arg)?;
                            hdr.pc = code_addr as usize;
                            stat!(self, peak_call_depth = max call_depth as u32);
                        }
                    } else {
                        return Err(VmError::NotCallable);
                    }
                }

                op::TAIL_CALL_DYNAMIC => {
                    self.ensure_gc_headroom()?;
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();

                    if func.is_foreign_fn() && func.fn_arity() == 1 {
                        let f = self.foreign_fns[func.fn_addr() as usize];
                        let result = f(self, arg)?;
                        if call_depth == 0 {
                            self.arena.set_stack_bot_pos(hdr.frame_base);
                            self.arena.stack_push(result)?;
                            return Ok(result);
                        }
                        hdr = self.arena.stack_frame_pop(&hdr, result)?;
                        call_depth -= 1;
                    } else if func.is_function() {
                        let arity = func.fn_arity();
                        if arity == 1 {
                            self.arena.set_stack_bot_pos(hdr.frame_base);
                            self.arena.stack_push(arg)?;
                            hdr.pc = func.fn_addr() as usize;
                        } else {
                            let cl = self.arena.alloc_closure(func.fn_addr(), arity, &[arg])?;
                            if call_depth == 0 {
                                self.arena.set_stack_bot_pos(hdr.frame_base);
                                self.arena.stack_push(cl)?;
                                return Ok(cl);
                            }
                            hdr = self.arena.stack_frame_pop(&hdr, cl)?;
                            call_depth -= 1;
                        }
                    } else if func.is_closure() {
                        let bound = self.arena.closure_bound_count(func);
                        let arity = self.arena.closure_arity(func) as usize;
                        if bound + 1 < arity {
                            let cl = self.arena.extend_closure(func, arg)?;
                            if call_depth == 0 {
                                self.arena.set_stack_bot_pos(hdr.frame_base);
                                self.arena.stack_push(cl)?;
                                return Ok(cl);
                            }
                            hdr = self.arena.stack_frame_pop(&hdr, cl)?;
                            call_depth -= 1;
                        } else {
                            let code_addr = self.arena.closure_code(func);
                            self.arena.set_stack_bot_pos(hdr.frame_base);
                            for i in 0..bound {
                                let v = self.arena.closure_bound(func, i);
                                self.arena.stack_push(v)?;
                            }
                            self.arena.stack_push(arg)?;
                            hdr.pc = code_addr as usize;
                        }
                    } else {
                        return Err(VmError::NotCallable);
                    }
                }

                op::CALL => {
                    let code_addr = u16::from_le_bytes([code[hdr.pc], code[hdr.pc + 1]]) as usize;
                    let n_args = code[hdr.pc + 2] as usize;
                    hdr.pc += 3;

                    for i in (0..n_args).rev() {
                        self.staging[i] = self.arena.stack_pop();
                    }

                    self.arena.stack_frame_push(&hdr)?;
                    call_depth += 1;
                    hdr.frame_base = self.arena.stack_bot_pos();
                    for i in 0..n_args {
                        self.arena.stack_push(self.staging[i])?;
                    }
                    hdr.pc = code_addr;
                    stat!(self, peak_call_depth = max call_depth as u32);
                }

                op::TAIL_CALL => {
                    let code_addr = u16::from_le_bytes([code[hdr.pc], code[hdr.pc + 1]]) as usize;
                    let n_args = code[hdr.pc + 2] as usize;

                    for i in (0..n_args).rev() {
                        self.staging[i] = self.arena.stack_pop();
                    }

                    self.arena.set_stack_bot_pos(hdr.frame_base);
                    for i in 0..n_args {
                        self.arena.stack_push(self.staging[i])?;
                    }
                    hdr.pc = code_addr;
                }

                op::RET => {
                    let result = self.arena.stack_pop();
                    if call_depth == 0 {
                        self.arena.set_stack_bot_pos(hdr.frame_base);
                        self.arena.stack_push(result)?;
                        return Ok(result);
                    }
                    hdr = self.arena.stack_frame_pop(&hdr, result)?;
                    call_depth -= 1;
                }

                op::MATCH2 => {
                    let base_tag = code[hdr.pc] as usize;
                    let table_start = hdr.pc + 1;
                    hdr.pc += 1 + 2 * 3;

                    let scrutinee = self.arena.stack_pop();
                    let scrutinee_tag = scrutinee.tag();

                    let idx = (scrutinee_tag as usize).wrapping_sub(base_tag);
                    if idx >= 2 {
                        return Err(VmError::MatchFailure { scrutinee_tag, pc: hdr.pc });
                    }

                    let entry = table_start + idx * 3;
                    let case_arity = code[entry] as usize;
                    let case_offset =
                        u16::from_le_bytes([code[entry + 1], code[entry + 2]]) as usize;

                    if case_arity > 0 {
                        self.arena.stack_push(scrutinee)?;
                    }
                    hdr.pc = case_offset;
                }

                op::MATCH => {
                    let base_tag = code[hdr.pc] as usize;
                    let n_entries = code[hdr.pc + 1] as usize;
                    hdr.pc += 2;
                    let table_start = hdr.pc;
                    hdr.pc += n_entries * 3;

                    let scrutinee = self.arena.stack_pop();
                    let scrutinee_tag = scrutinee.tag();

                    let idx = (scrutinee_tag as usize).wrapping_sub(base_tag);
                    if idx >= n_entries {
                        return Err(VmError::MatchFailure { scrutinee_tag, pc: hdr.pc });
                    }

                    let entry = table_start + idx * 3;
                    let case_arity = code[entry] as usize;
                    let case_offset =
                        u16::from_le_bytes([code[entry + 1], code[entry + 2]]) as usize;

                    if case_arity > 0 {
                        self.arena.stack_push(scrutinee)?;
                    }
                    hdr.pc = case_offset;
                }

                op::JMP => {
                    hdr.pc = u16::from_le_bytes([code[hdr.pc], code[hdr.pc + 1]]) as usize;
                }

                op::BIND => {
                    let n = code[hdr.pc] as usize;
                    let scrutinee = self.arena.stack_pop();
                    for i in 0..n {
                        let field = self.arena.ctor_field(scrutinee, i);
                        self.arena.stack_push(field)?;
                    }
                    hdr.pc += 1;
                }

                op::DROP => {
                    let n = code[hdr.pc] as usize;
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + n);
                    hdr.pc += 1;
                }

                op::SLIDE1 => {
                    let result = self.arena.stack_pop();
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + 1);
                    self.arena.stack_push(result)?;
                }

                op::SLIDE => {
                    let n = code[hdr.pc] as usize;
                    let result = self.arena.stack_pop();
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + n);
                    self.arena.stack_push(result)?;
                    hdr.pc += 1;
                }

                op::ERROR => {
                    return Err(VmError::MatchFailure { scrutinee_tag: 0xFF, pc: hdr.pc });
                }

                op::FIXPOINT => {
                    let cap_idx = code[hdr.pc] as usize;
                    let closure = self.arena.stack_peek(0);
                    if cap_idx != 0xFF {
                        self.arena.closure_set_bound(closure, cap_idx, closure);
                    }
                    self.arena.stack_set(1, closure);
                    self.arena.stack_pop();
                    hdr.pc += 1;
                }

                op::INT0 => {
                    self.arena.stack_push(Value::ZERO)?;
                }

                op::INT1 => {
                    self.arena.stack_push(Value::ONE)?;
                }

                op::INT => {
                    let n = i32::from_le_bytes([
                        code[hdr.pc],
                        code[hdr.pc + 1],
                        code[hdr.pc + 2],
                        code[hdr.pc + 3],
                    ]);
                    self.arena.stack_push(Value::integer(n))?;
                    hdr.pc += 4;
                }

                op::ADD => {
                    let b = self.arena.stack_pop().integer_value();
                    let a = self.arena.stack_pop().integer_value();
                    self.arena.stack_push(Value::integer(a.wrapping_add(b)))?;
                }

                op::SUB => {
                    let b = self.arena.stack_pop().integer_value();
                    let a = self.arena.stack_pop().integer_value();
                    self.arena.stack_push(Value::integer(a.wrapping_sub(b)))?;
                }

                op::MUL => {
                    let b = self.arena.stack_pop().integer_value();
                    let a = self.arena.stack_pop().integer_value();
                    self.arena.stack_push(Value::integer(a.wrapping_mul(b)))?;
                }

                op::DIV => {
                    let b = self.arena.stack_pop().integer_value();
                    let a = self.arena.stack_pop().integer_value();
                    self.arena.stack_push(Value::integer(a.wrapping_div(b)))?;
                }

                op::NEG => {
                    let a = self.arena.stack_pop().integer_value();
                    self.arena.stack_push(Value::integer(a.wrapping_neg()))?;
                }

                op::EQ => {
                    let b = self.arena.stack_pop().integer_value();
                    let a = self.arena.stack_pop().integer_value();
                    let tag = if a == b { crate::value::tags::TRUE } else { crate::value::tags::FALSE };
                    self.arena.stack_push(Value::nullary_ctor(tag))?;
                }

                op::LT => {
                    let b = self.arena.stack_pop().integer_value();
                    let a = self.arena.stack_pop().integer_value();
                    let tag = if a < b { crate::value::tags::TRUE } else { crate::value::tags::FALSE };
                    self.arena.stack_push(Value::nullary_ctor(tag))?;
                }

                op::BYTES => {
                    self.ensure_gc_headroom()?;
                    let len = code[hdr.pc] as usize;
                    let data_start = hdr.pc + 1;
                    let val = self.arena.alloc_bytes(&code[data_start..data_start + len])?;
                    self.arena.stack_push(val)?;
                    hdr.pc = data_start + len;
                }

                op::BYTES_LEN => {
                    let v = self.arena.stack_pop();
                    self.arena.stack_push(Value::integer(self.arena.bytes_len(v) as i32))?;
                }

                op::BYTES_GET => {
                    let idx = self.arena.stack_pop().integer_value() as usize;
                    let v = self.arena.stack_pop();
                    if idx >= self.arena.bytes_len(v) {
                        return Err(VmError::IndexOutOfBounds);
                    }
                    let data = self.arena.bytes_data(v);
                    self.arena.stack_push(Value::integer(data[idx] as i32))?;
                }

                op::BYTES_EQ => {
                    let b = self.arena.stack_pop();
                    let a = self.arena.stack_pop();
                    let eq = self.arena.bytes_len(a) == self.arena.bytes_len(b)
                        && self.arena.bytes_data(a) == self.arena.bytes_data(b);
                    let tag = if eq { crate::value::tags::TRUE } else { crate::value::tags::FALSE };
                    self.arena.stack_push(Value::nullary_ctor(tag))?;
                }

                op::BYTES_CONCAT => {
                    self.ensure_gc_headroom()?;
                    let b = self.arena.stack_pop();
                    let a = self.arena.stack_pop();
                    if self.arena.bytes_len(a) + self.arena.bytes_len(b) > 255 {
                        return Err(VmError::BytesOverflow);
                    }
                    let val = self.arena.bytes_concat(a, b)?;
                    self.arena.stack_push(val)?;
                }

                op::FOREIGN => {
                    let idx = u16::from_le_bytes([code[hdr.pc], code[hdr.pc + 1]]);
                    let arity = code[hdr.pc + 2];
                    self.arena.stack_push(Value::foreign_fn(idx, arity))?;
                    hdr.pc += 3;
                }

                op::DUP => {
                    let v = self.arena.stack_peek(0);
                    self.arena.stack_push(v)?;
                }

                op::OVER => {
                    let v = self.arena.stack_peek(1);
                    self.arena.stack_push(v)?;
                }

                _ => return Err(VmError::InvalidBytecode),
            }

            #[cfg(feature = "stats")]
            {
                let heap_after = self.arena.heap_used();
                if heap_after > heap_before {
                    self.stats.opcode_heap_words[opcode as usize] += (heap_after - heap_before) as u32;
                }
                let cyc_delta = (self.cycle_reader)().wrapping_sub(cyc_before);
                self.stats.opcode_cycles[opcode as usize] += cyc_delta as u64;
            }
        }
    }
}
