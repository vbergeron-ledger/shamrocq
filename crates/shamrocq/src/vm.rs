use crate::arena::{Arena, ArenaError};
use crate::bytes;
use crate::stats::stat;
#[cfg(feature = "stats")]
use crate::stats::Stats;
use crate::stats::MemSnapshot;
use crate::value::Value;
use shamrocq_bytecode::op;
use shamrocq_bytecode::{MAGIC, BYTECODE_VERSION, DUMP_MAGIC, DUMP_VERSION};

const MIN_BYTECODE_VERSION: u16 = BYTECODE_VERSION;
const MAX_BYTECODE_VERSION: u16 = BYTECODE_VERSION;

#[derive(Debug)]
pub enum VmError {
    Oom,
    MatchFailure { scrutinee_tag: u8, pc: usize },
    InvalidBytecode,
    UnsupportedVersion { version: u16 },
    NotAClosure,
    IndexOutOfBounds,
    BytesOverflow,
    NotRegistered,
}

impl From<ArenaError> for VmError {
    fn from(_: ArenaError) -> Self {
        VmError::Oom
    }
}

/// A host function callable from Scheme. Takes a single (curried) argument.
pub type ForeignFn = fn(&mut Vm<'_>, Value) -> Result<Value, VmError>;

const MAX_FOREIGN_FNS: usize = 32;

fn unregistered_foreign_fn(_: &mut Vm<'_>, _: Value) -> Result<Value, VmError> {
    Err(VmError::NotRegistered)
}

pub struct Program<'a> {
    pub n_globals: u16,
    pub global_names: &'a [u8],
    pub code: &'a [u8],
}

impl<'a> Program<'a> {
    pub fn from_blob(blob: &'a [u8]) -> Result<Self, VmError> {
        if blob.len() < 4 + 2 + 2 {
            return Err(VmError::InvalidBytecode);
        }
        if blob[0..4] != MAGIC {
            return Err(VmError::InvalidBytecode);
        }
        let version = u16::from_le_bytes([blob[4], blob[5]]);
        if version < MIN_BYTECODE_VERSION || version > MAX_BYTECODE_VERSION {
            return Err(VmError::UnsupportedVersion { version });
        }
        let n_globals = u16::from_le_bytes([blob[6], blob[7]]);
        let mut pos = 8usize;
        for _ in 0..n_globals {
            if pos >= blob.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = blob[pos] as usize;
            pos += 1 + name_len + 2;
        }
        let globals_end = pos;
        if pos + 2 > blob.len() {
            return Err(VmError::InvalidBytecode);
        }
        let n_tags = u16::from_le_bytes([blob[pos], blob[pos + 1]]) as usize;
        pos += 2;
        for _ in 0..n_tags {
            if pos >= blob.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = blob[pos] as usize;
            pos += 1 + name_len;
        }
        Ok(Program {
            n_globals,
            global_names: &blob[8..globals_end],
            code: &blob[pos..],
        })
    }

    pub fn global_code_offset(&self, idx: u16) -> Result<u16, VmError> {
        let mut pos = 0usize;
        for i in 0..self.n_globals {
            if pos >= self.global_names.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = self.global_names[pos] as usize;
            pos += 1 + name_len;
            if i == idx {
                return Ok(u16::from_le_bytes([
                    self.global_names[pos],
                    self.global_names[pos + 1],
                ]));
            }
            pos += 2;
        }
        Err(VmError::InvalidBytecode)
    }

    pub fn global_index(&self, name: &str) -> Option<u16> {
        let name_bytes = name.as_bytes();
        let mut pos = 0usize;
        for i in 0..self.n_globals {
            if pos >= self.global_names.len() {
                return None;
            }
            let name_len = self.global_names[pos] as usize;
            let entry_name = &self.global_names[pos + 1..pos + 1 + name_len];
            if entry_name == name_bytes {
                return Some(i);
            }
            pos += 1 + name_len + 2;
        }
        None
    }
}

const FRAME_HEADER_WORDS: usize = 3;

pub struct Vm<'buf> {
    pub arena: Arena<'buf>,
    globals: [Value; 64],
    n_globals: u16,
    code: &'buf [u8],
    foreign_fns: [ForeignFn; MAX_FOREIGN_FNS],
    #[cfg(feature = "stats")]
    pub stats: Stats,
}

impl<'buf> Vm<'buf> {
    pub fn new(buf: &'buf mut [u8]) -> Self {
        Vm {
            arena: Arena::from_bytes(buf),
            globals: [Value::integer(0); 64],
            n_globals: 0,
            code: &[],
            foreign_fns: [unregistered_foreign_fn; MAX_FOREIGN_FNS],
            #[cfg(feature = "stats")]
            stats: Stats::default(),
        }
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
        #[allow(unused_variables)]
        let gc_stats = self.arena.collect_garbage(&mut self.globals[..n]);
        stat!(self, gc_count += 1);
        stat!(self, gc_bytes_reclaimed += (gc_stats.words_reclaimed * 4) as u32);
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

    pub fn load_program(&mut self, prog: &Program<'buf>) -> Result<(), VmError> {
        self.n_globals = prog.n_globals;
        self.code = prog.code;
        for i in 0..prog.n_globals {
            let offset = prog.global_code_offset(i)?;
            let val = self.eval(offset as usize)?;
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
                return Err(VmError::NotAClosure);
            }

            if func.is_foreign_fn() && func.fn_arity() == 1 {
                let f = self.foreign_fns[func.fn_addr() as usize];
                func = f(self, arg)?;
            } else if func.is_function() {
                let arity = func.fn_arity();
                if arity == 1 {
                    let saved_pos = self.arena.stack_bot_pos();
                    self.arena.stack_push(arg)?;
                    func = self.eval_with_frame(func.fn_addr() as usize, saved_pos)?;
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
                    func = self.eval_with_frame(code_addr as usize, saved_pos)?;
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

    fn try_reclaim(&mut self, result: Value, saved_heap_top: usize) {
        let current = self.arena.heap_used();
        if current > saved_heap_top && !Self::references_heap_since(result, saved_heap_top) {
            stat!(self, reclaim_count += 1);
            stat!(self, reclaim_bytes_total += ((current - saved_heap_top) * 4) as u32);
            self.arena.set_heap_top(saved_heap_top);
        }
    }

    fn references_heap_since(val: Value, start: usize) -> bool {
        if !val.is_reference() {
            return false;
        }
        val.offset() >= start
    }

    fn eval(&mut self, start_pc: usize) -> Result<Value, VmError> {
        let fb = self.arena.stack_bot_pos();
        self.eval_with_frame(start_pc, fb)
    }

    fn eval_with_frame(
        &mut self,
        start_pc: usize,
        frame_base: usize,
    ) -> Result<Value, VmError> {
        let code = self.code;
        let mut pc = start_pc;
        let mut call_depth: usize = 0;
        let mut frame_base = frame_base;

        const GC_THRESHOLD: usize = 64;

        loop {
            if pc >= code.len() {
                return Err(VmError::InvalidBytecode);
            }

            if self.arena.free() < GC_THRESHOLD {
                self.run_gc();
                if self.arena.free() < GC_THRESHOLD {
                    return Err(VmError::Oom);
                }
            }

            stat!(self, exec_instruction_count += 1);

            let opcode = code[pc];
            pc += 1;

            match opcode {
                op::PACK0 => {
                    let tag = code[pc];
                    self.arena.stack_push(Value::nullary_ctor(tag))?;
                    pc += 1;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::PACK => {
                    let tag = code[pc];
                    let arity = code[pc + 1] as usize;
                    let val = self.arena.alloc_ctor_from_stack(tag, arity)?;
                    self.arena.stack_push(val)?;
                    pc += 2;
                    stat!(self, alloc_count_ctor += 1);
                    stat!(self, alloc_bytes_total += ((1 + arity) * 4) as u32);
                    stat!(self, peak_heap_bytes = max self.arena.heap_used() * 4);
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::UNPACK => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let scrutinee = self.arena.stack_pop();
                    for i in 0..n {
                        let field = self.arena.ctor_field(scrutinee, i);
                        self.arena.stack_push(field)?;
                    }
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::LOAD => {
                    let idx = code[pc] as usize;
                    pc += 1;
                    let val = self.arena.stack_read_at(frame_base - (idx + 1));
                    self.arena.stack_push(val)?;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::LOAD2 => {
                    let idx_a = code[pc] as usize;
                    let idx_b = code[pc + 1] as usize;
                    pc += 2;
                    let a = self.arena.stack_read_at(frame_base - (idx_a + 1));
                    let b = self.arena.stack_read_at(frame_base - (idx_b + 1));
                    self.arena.stack_push(a)?;
                    self.arena.stack_push(b)?;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::LOAD3 => {
                    let idx_a = code[pc] as usize;
                    let idx_b = code[pc + 1] as usize;
                    let idx_c = code[pc + 2] as usize;
                    pc += 3;
                    let a = self.arena.stack_read_at(frame_base - (idx_a + 1));
                    let b = self.arena.stack_read_at(frame_base - (idx_b + 1));
                    let c = self.arena.stack_read_at(frame_base - (idx_c + 1));
                    self.arena.stack_push(a)?;
                    self.arena.stack_push(b)?;
                    self.arena.stack_push(c)?;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::GLOBAL => {
                    let idx = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                    pc += 2;
                    self.arena.stack_push(self.globals[idx])?;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::CLOSURE => {
                    let code_addr = u16::from_le_bytes([code[pc], code[pc + 1]]);
                    let arity = code[pc + 2];
                    let n_cap = code[pc + 3] as usize;
                    pc += 4;
                    if n_cap == 0 {
                        self.arena.stack_push(Value::function(code_addr, arity))?;
                    } else {
                        let val = self.arena.alloc_closure_from_stack(code_addr, arity, n_cap)?;
                        self.arena.stack_push(val)?;
                        stat!(self, alloc_count_closure += 1);
                        stat!(self, alloc_bytes_total += ((2 + n_cap) * 4) as u32);
                        stat!(self, peak_heap_bytes = max self.arena.heap_used() * 4);
                    }
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::CALL1 => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();

                    if func.is_foreign_fn() && func.fn_arity() == 1 {
                        let f = self.foreign_fns[func.fn_addr() as usize];
                        let result = f(self, arg)?;
                        self.arena.stack_push(result)?;
                    } else if func.is_function() {
                        let arity = func.fn_arity();
                        if arity == 1 {
                            self.arena.stack_push(Value::from_raw(self.arena.heap_used() as u32))?;
                            self.arena.stack_push(Value::from_raw(pc as u32))?;
                            self.arena.stack_push(Value::from_raw(frame_base as u32))?;
                            call_depth += 1;
                            frame_base = self.arena.stack_bot_pos();
                            self.arena.stack_push(arg)?;
                            pc = func.fn_addr() as usize;
                            stat!(self, exec_call_count += 1);
                            stat!(self, exec_peak_call_depth = max call_depth as u32);
                            stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                        } else {
                            let cl = self.arena.alloc_closure(func.fn_addr(), arity, &[arg])?;
                            self.arena.stack_push(cl)?;
                            stat!(self, peak_heap_bytes = max self.arena.heap_used() * 4);
                            stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                        }
                    } else if func.is_closure() {
                        let bound = self.arena.closure_bound_count(func);
                        let arity = self.arena.closure_arity(func) as usize;
                        if bound + 1 < arity {
                            let cl = self.arena.extend_closure(func, arg)?;
                            self.arena.stack_push(cl)?;
                            stat!(self, peak_heap_bytes = max self.arena.heap_used() * 4);
                            stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                        } else {
                            let code_addr = self.arena.closure_code(func);
                            self.arena.stack_push(Value::from_raw(self.arena.heap_used() as u32))?;
                            self.arena.stack_push(Value::from_raw(pc as u32))?;
                            self.arena.stack_push(Value::from_raw(frame_base as u32))?;
                            call_depth += 1;
                            frame_base = self.arena.stack_bot_pos();
                            for i in 0..bound {
                                let v = self.arena.closure_bound(func, i);
                                self.arena.stack_push(v)?;
                            }
                            self.arena.stack_push(arg)?;
                            pc = code_addr as usize;
                            stat!(self, exec_call_count += 1);
                            stat!(self, exec_peak_call_depth = max call_depth as u32);
                            stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                        }
                    } else {
                        return Err(VmError::NotAClosure);
                    }
                }

                op::TAIL_CALL1 => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();

                    if func.is_foreign_fn() && func.fn_arity() == 1 {
                        let f = self.foreign_fns[func.fn_addr() as usize];
                        let result = f(self, arg)?;
                        if call_depth == 0 {
                            self.arena.set_stack_bot_pos(frame_base);
                            self.arena.stack_push(result)?;
                            return Ok(result);
                        }
                        let saved_fb = self.arena.stack_read_at(frame_base).raw() as usize;
                        let saved_pc = self.arena.stack_read_at(frame_base + 1).raw() as usize;
                        let saved_heap = self.arena.stack_read_at(frame_base + 2).raw() as usize;
                        self.try_reclaim(result, saved_heap);
                        self.arena.set_stack_bot_pos(frame_base + FRAME_HEADER_WORDS);
                        self.arena.stack_push(result)?;
                        call_depth -= 1;
                        pc = saved_pc;
                        frame_base = saved_fb;
                    } else if func.is_function() {
                        let arity = func.fn_arity();
                        if arity == 1 {
                            self.arena.set_stack_bot_pos(frame_base);
                            self.arena.stack_push(arg)?;
                            pc = func.fn_addr() as usize;
                            stat!(self, exec_tail_call_count += 1);
                            stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                        } else {
                            let cl = self.arena.alloc_closure(func.fn_addr(), arity, &[arg])?;
                            if call_depth == 0 {
                                self.arena.set_stack_bot_pos(frame_base);
                                self.arena.stack_push(cl)?;
                                return Ok(cl);
                            }
                            let saved_fb = self.arena.stack_read_at(frame_base).raw() as usize;
                            let saved_pc = self.arena.stack_read_at(frame_base + 1).raw() as usize;
                            let saved_heap = self.arena.stack_read_at(frame_base + 2).raw() as usize;
                            self.try_reclaim(cl, saved_heap);
                            self.arena.set_stack_bot_pos(frame_base + FRAME_HEADER_WORDS);
                            self.arena.stack_push(cl)?;
                            call_depth -= 1;
                            pc = saved_pc;
                            frame_base = saved_fb;
                        }
                    } else if func.is_closure() {
                        let bound = self.arena.closure_bound_count(func);
                        let arity = self.arena.closure_arity(func) as usize;
                        if bound + 1 < arity {
                            let cl = self.arena.extend_closure(func, arg)?;
                            stat!(self, peak_heap_bytes = max self.arena.heap_used() * 4);
                            if call_depth == 0 {
                                self.arena.set_stack_bot_pos(frame_base);
                                self.arena.stack_push(cl)?;
                                return Ok(cl);
                            }
                            let saved_fb = self.arena.stack_read_at(frame_base).raw() as usize;
                            let saved_pc = self.arena.stack_read_at(frame_base + 1).raw() as usize;
                            let saved_heap = self.arena.stack_read_at(frame_base + 2).raw() as usize;
                            self.try_reclaim(cl, saved_heap);
                            self.arena.set_stack_bot_pos(frame_base + FRAME_HEADER_WORDS);
                            self.arena.stack_push(cl)?;
                            call_depth -= 1;
                            pc = saved_pc;
                            frame_base = saved_fb;
                        } else {
                            let code_addr = self.arena.closure_code(func);
                            self.arena.set_stack_bot_pos(frame_base);
                            for i in 0..bound {
                                let v = self.arena.closure_bound(func, i);
                                self.arena.stack_push(v)?;
                            }
                            self.arena.stack_push(arg)?;
                            pc = code_addr as usize;
                            stat!(self, exec_tail_call_count += 1);
                            stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                        }
                    } else {
                        return Err(VmError::NotAClosure);
                    }
                }

                op::CALL_N => {
                    let code_addr = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                    let n_args = code[pc + 2] as usize;
                    pc += 3;

                    let mut tmp = [Value::integer(0); 16];
                    for i in (0..n_args).rev() {
                        tmp[i] = self.arena.stack_pop();
                    }

                    self.arena.stack_push(Value::from_raw(self.arena.heap_used() as u32))?;
                    self.arena.stack_push(Value::from_raw(pc as u32))?;
                    self.arena.stack_push(Value::from_raw(frame_base as u32))?;
                    call_depth += 1;
                    frame_base = self.arena.stack_bot_pos();
                    for i in 0..n_args {
                        self.arena.stack_push(tmp[i])?;
                    }
                    pc = code_addr;
                    stat!(self, exec_call_count += 1);
                    stat!(self, exec_peak_call_depth = max call_depth as u32);
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::TAIL_CALL_N => {
                    let code_addr = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                    let n_args = code[pc + 2] as usize;

                    let mut tmp = [Value::integer(0); 16];
                    for i in (0..n_args).rev() {
                        tmp[i] = self.arena.stack_pop();
                    }

                    self.arena.set_stack_bot_pos(frame_base);
                    for i in 0..n_args {
                        self.arena.stack_push(tmp[i])?;
                    }
                    pc = code_addr;
                    stat!(self, exec_tail_call_count += 1);
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::RET => {
                    let result = self.arena.stack_pop();
                    if call_depth == 0 {
                        self.arena.set_stack_bot_pos(frame_base);
                        self.arena.stack_push(result)?;
                        return Ok(result);
                    }
                    let saved_fb = self.arena.stack_read_at(frame_base).raw() as usize;
                    let saved_pc = self.arena.stack_read_at(frame_base + 1).raw() as usize;
                    let saved_heap = self.arena.stack_read_at(frame_base + 2).raw() as usize;
                    self.try_reclaim(result, saved_heap);
                    self.arena.set_stack_bot_pos(frame_base + FRAME_HEADER_WORDS);
                    self.arena.stack_push(result)?;
                    call_depth -= 1;
                    pc = saved_pc;
                    frame_base = saved_fb;
                }

                op::MATCH2 => {
                    let base_tag = code[pc] as usize;
                    let table_start = pc + 1;
                    pc += 1 + 2 * 3;

                    let scrutinee = self.arena.stack_pop();
                    let scrutinee_tag = scrutinee.tag();

                    let idx = (scrutinee_tag as usize).wrapping_sub(base_tag);
                    if idx >= 2 {
                        return Err(VmError::MatchFailure { scrutinee_tag, pc });
                    }

                    let entry = table_start + idx * 3;
                    let case_arity = code[entry] as usize;
                    let case_offset =
                        u16::from_le_bytes([code[entry + 1], code[entry + 2]]) as usize;

                    if case_arity > 0 {
                        self.arena.stack_push(scrutinee)?;
                    }
                    pc = case_offset;
                    stat!(self, exec_match_count += 1);
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::MATCH => {
                    let base_tag = code[pc] as usize;
                    let n_entries = code[pc + 1] as usize;
                    pc += 2;
                    let table_start = pc;
                    pc += n_entries * 3;

                    let scrutinee = self.arena.stack_pop();
                    let scrutinee_tag = scrutinee.tag();

                    let idx = (scrutinee_tag as usize).wrapping_sub(base_tag);
                    if idx >= n_entries {
                        return Err(VmError::MatchFailure { scrutinee_tag, pc });
                    }

                    let entry = table_start + idx * 3;
                    let case_arity = code[entry] as usize;
                    let case_offset =
                        u16::from_le_bytes([code[entry + 1], code[entry + 2]]) as usize;

                    if case_arity > 0 {
                        self.arena.stack_push(scrutinee)?;
                    }
                    pc = case_offset;
                    stat!(self, exec_match_count += 1);
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::JMP => {
                    pc = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                }

                op::BIND => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let scrutinee = self.arena.stack_pop();
                    for i in 0..n {
                        let field = self.arena.ctor_field(scrutinee, i);
                        self.arena.stack_push(field)?;
                    }
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::DROP => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + n);
                }

                op::SLIDE1 => {
                    let result = self.arena.stack_pop();
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + 1);
                    self.arena.stack_push(result)?;
                }

                op::SLIDE => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let result = self.arena.stack_pop();
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + n);
                    self.arena.stack_push(result)?;
                }

                op::ERROR => {
                    return Err(VmError::MatchFailure { scrutinee_tag: 0xFF, pc });
                }

                op::FIXPOINT => {
                    let cap_idx = code[pc] as usize;
                    pc += 1;
                    let closure = self.arena.stack_peek(0);
                    if cap_idx != 0xFF {
                        self.arena.closure_set_bound(closure, cap_idx, closure);
                    }
                    self.arena.stack_set(1, closure);
                    self.arena.stack_pop();
                }

                op::INT0 => {
                    self.arena.stack_push(Value::ZERO)?;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::INT1 => {
                    self.arena.stack_push(Value::ONE)?;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::INT => {
                    let n = i32::from_le_bytes([
                        code[pc],
                        code[pc + 1],
                        code[pc + 2],
                        code[pc + 3],
                    ]);
                    pc += 4;
                    self.arena.stack_push(Value::integer(n))?;
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
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
                    let len = code[pc] as usize;
                    pc += 1;
                    let val = self.arena.alloc_bytes(&code[pc..pc + len])?;
                    pc += len;
                    self.arena.stack_push(val)?;
                    stat!(self, peak_heap_bytes = max self.arena.heap_used() * 4);
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
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
                    let b = self.arena.stack_pop();
                    let a = self.arena.stack_pop();
                    if self.arena.bytes_len(a) + self.arena.bytes_len(b) > 255 {
                        return Err(VmError::BytesOverflow);
                    }
                    let val = self.arena.bytes_concat(a, b)?;
                    self.arena.stack_push(val)?;
                    stat!(self, peak_heap_bytes = max self.arena.heap_used() * 4);
                    stat!(self, peak_stack_bytes = max self.arena.stack_used() * 4);
                }

                op::FUNCTION => {
                    let idx = u16::from_le_bytes([code[pc], code[pc + 1]]);
                    let arity = code[pc + 2];
                    pc += 3;
                    self.arena.stack_push(Value::foreign_fn(idx, arity))?;
                }

                _ => return Err(VmError::InvalidBytecode),
            }
        }
    }
}
