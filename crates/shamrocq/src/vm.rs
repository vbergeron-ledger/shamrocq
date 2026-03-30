use crate::arena::{Arena, ArenaError};
use crate::stats::stat;
#[cfg(feature = "stats")]
use crate::stats::Stats;
use crate::stats::MemSnapshot;
use crate::value::Value;

mod op {
    pub const PACK: u8 = 0x01;
    pub const UNPACK: u8 = 0x02;
    pub const LOAD: u8 = 0x03;
    pub const GLOBAL: u8 = 0x04;
    pub const CLOSURE: u8 = 0x05;
    pub const CALL: u8 = 0x06;
    pub const TAIL_CALL: u8 = 0x07;
    pub const RET: u8 = 0x08;
    pub const MATCH: u8 = 0x09;
    pub const JMP: u8 = 0x0A;
    pub const BIND: u8 = 0x0B;
    pub const DROP: u8 = 0x0C;
    pub const ERROR: u8 = 0x0D;
    pub const SLIDE: u8 = 0x0E;
    pub const FIXPOINT: u8 = 0x0F;
    pub const INT_CONST: u8 = 0x10;
    pub const ADD: u8 = 0x11;
    pub const SUB: u8 = 0x12;
    pub const MUL: u8 = 0x13;
    pub const DIV: u8 = 0x14;
    pub const NEG: u8 = 0x15;
    pub const EQ: u8 = 0x16;
    pub const LT: u8 = 0x17;
    pub const BYTES_CONST: u8 = 0x18;
    pub const BYTES_LEN: u8 = 0x19;
    pub const BYTES_GET: u8 = 0x1A;
    pub const BYTES_EQ: u8 = 0x1B;
    pub const BYTES_CONCAT: u8 = 0x1C;
    pub const CALL_DIRECT: u8 = 0x1D;
    pub const TAIL_CALL_DIRECT: u8 = 0x1E;
    pub const FOREIGN_FN_CONST: u8 = 0x1F;
    pub const LOAD_CAPTURE: u8 = 0x20;
    pub const LOAD2: u8 = 0x21;
    pub const LOAD3: u8 = 0x22;
}

const MAGIC: [u8; 4] = *b"SMRQ";
const MIN_BYTECODE_VERSION: u16 = 1;
const MAX_BYTECODE_VERSION: u16 = 1;

#[derive(Debug)]
pub enum VmError {
    Oom,
    MatchFailure { scrutinee_tag: u8, pc: usize },
    InvalidBytecode,
    UnsupportedVersion { version: u16 },
    NotAClosure,
    StackOverflow,
    IndexOutOfBounds,
    BytesOverflow,
}

impl From<ArenaError> for VmError {
    fn from(_: ArenaError) -> Self {
        VmError::Oom
    }
}

/// A host function callable from Scheme. Takes a single (curried) argument.
pub type ForeignFn = fn(&mut Vm<'_>, Value) -> Result<Value, VmError>;

const MAX_FOREIGN_FNS: usize = 32;

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

struct CallFrame {
    return_pc: usize,
    /// Byte position of stack_bot at frame entry (raw byte offset into arena buf).
    frame_base: usize,
    /// The closure being executed (captures accessed via LOAD_CAPTURE).
    env: Value,
}

impl CallFrame {
    fn fresh() -> Self {
        CallFrame {
            return_pc: 0,
            frame_base: 0,
            env: Value::ctor(0, 0),
        }
    }
}

pub struct Vm<'buf> {
    pub arena: Arena<'buf>,
    globals: [Value; 64],
    n_globals: u16,
    code: &'buf [u8],
    call_stack: [CallFrame; MAX_CALL_DEPTH],
    foreign_fns: [Option<ForeignFn>; MAX_FOREIGN_FNS],
    #[cfg(feature = "stats")]
    pub stats: Stats,
}

const MAX_CALL_DEPTH: usize = 256;

impl<'buf> Vm<'buf> {
    pub fn new(buf: &'buf mut [u8]) -> Self {
        Vm {
            arena: Arena::new(buf),
            globals: [Value::ctor(0, 0); 64],
            n_globals: 0,
            code: &[],
            call_stack: core::array::from_fn(|_| CallFrame::fresh()),
            foreign_fns: [None; MAX_FOREIGN_FNS],
            #[cfg(feature = "stats")]
            stats: Stats::default(),
        }
    }

    /// Register a host function at the given index.
    ///
    /// The index must match the one assigned by the compiler for `define-foreign`
    /// declarations (see `foreign` module in the generated `bindings.rs`). Call this before
    /// `load_program` so the global slot resolves to the correct callable value.
    pub fn register_foreign(&mut self, idx: u16, f: ForeignFn) {
        self.foreign_fns[idx as usize] = Some(f);
    }

    pub fn reset(&mut self) {
        self.arena.reset();
    }

    pub fn mem_snapshot(&self) -> MemSnapshot {
        MemSnapshot {
            heap_bytes: self.arena.heap_used(),
            stack_bytes: self.arena.stack_used(),
            free_bytes: self.arena.free(),
        }
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
            if func.is_foreign_fn() {
                let f = self.foreign_fns[func.foreign_fn_idx() as usize]
                    .ok_or(VmError::InvalidBytecode)?;
                func = f(self, arg)?;
            } else {
                let saved_pos = self.arena.stack_bot_pos();
                let (code_addr, env) = if func.is_bare_fn() {
                    (func.code_addr(), Value::ctor(0, 0))
                } else {
                    (self.arena.closure_code(func), func)
                };
                self.arena.stack_push(arg)?;
                func = self.eval_with_frame(code_addr as usize, saved_pos, env)?;
            }
        }
        Ok(func)
    }

    pub fn ctor_field(&self, val: Value, idx: usize) -> Value {
        self.arena.ctor_field(val, idx)
    }

    pub fn alloc_ctor(&mut self, tag: u8, fields: &[Value]) -> Result<Value, VmError> {
        Ok(self.arena.alloc_ctor(tag, fields)?)
    }

    fn record_heap(&mut self) {
        stat!(self, peak_heap_bytes = max self.arena.heap_used());
    }

    fn record_stack(&mut self) {
        stat!(self, peak_stack_bytes = max self.arena.stack_used());
    }

    fn eval(&mut self, start_pc: usize) -> Result<Value, VmError> {
        let fb = self.arena.stack_bot_pos();
        self.eval_with_frame(start_pc, fb, Value::ctor(0, 0))
    }

    fn eval_with_frame(
        &mut self,
        start_pc: usize,
        frame_base: usize,
        start_env: Value,
    ) -> Result<Value, VmError> {
        let code = self.code;
        let mut pc = start_pc;
        let mut call_depth: usize = 0;
        let mut frame_base = frame_base;
        let mut env = start_env;

        loop {
            if pc >= code.len() {
                return Err(VmError::InvalidBytecode);
            }

            stat!(self, exec_instruction_count += 1);

            let opcode = code[pc];
            pc += 1;

            match opcode {
                op::PACK => {
                    let tag = code[pc];
                    let arity = code[pc + 1] as usize;
                    pc += 2;
                    if arity == 0 {
                        self.arena.stack_push(Value::ctor(tag, 0))?;
                    } else {
                        let val = self.arena.alloc_ctor_from_stack(tag, arity)?;
                        stat!(self, alloc_count_ctor += 1);
                        stat!(self, alloc_bytes_total += (arity * 4) as u32);
                        self.record_heap();
                        self.arena.stack_push(val)?;
                    }
                    self.record_stack();
                }

                op::UNPACK => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let scrutinee = self.arena.stack_pop();
                    for i in 0..n {
                        let field = self.arena.ctor_field(scrutinee, i);
                        self.arena.stack_push(field)?;
                    }
                    self.record_stack();
                }

                op::LOAD => {
                    let idx = code[pc] as usize;
                    pc += 1;
                    let val = self.arena.stack_read_at(frame_base - (idx + 1) * 4);
                    self.arena.stack_push(val)?;
                    self.record_stack();
                }

                op::LOAD2 => {
                    let idx_a = code[pc] as usize;
                    let idx_b = code[pc + 1] as usize;
                    pc += 2;
                    let a = self.arena.stack_read_at(frame_base - (idx_a + 1) * 4);
                    let b = self.arena.stack_read_at(frame_base - (idx_b + 1) * 4);
                    self.arena.stack_push(a)?;
                    self.arena.stack_push(b)?;
                    self.record_stack();
                }

                op::LOAD3 => {
                    let idx_a = code[pc] as usize;
                    let idx_b = code[pc + 1] as usize;
                    let idx_c = code[pc + 2] as usize;
                    pc += 3;
                    let a = self.arena.stack_read_at(frame_base - (idx_a + 1) * 4);
                    let b = self.arena.stack_read_at(frame_base - (idx_b + 1) * 4);
                    let c = self.arena.stack_read_at(frame_base - (idx_c + 1) * 4);
                    self.arena.stack_push(a)?;
                    self.arena.stack_push(b)?;
                    self.arena.stack_push(c)?;
                    self.record_stack();
                }

                op::LOAD_CAPTURE => {
                    let idx = code[pc] as usize;
                    pc += 1;
                    let val = self.arena.closure_capture(env, idx);
                    self.arena.stack_push(val)?;
                    self.record_stack();
                }

                op::GLOBAL => {
                    let idx = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                    pc += 2;
                    self.arena.stack_push(self.globals[idx])?;
                    self.record_stack();
                }

                op::CLOSURE => {
                    let code_addr = u16::from_le_bytes([code[pc], code[pc + 1]]);
                    let n_cap = code[pc + 2] as usize;
                    pc += 3;
                    if n_cap == 0 {
                        self.arena.stack_push(Value::bare_fn(code_addr))?;
                        self.record_stack();
                    } else {
                        let val = self.arena.alloc_closure_from_stack(code_addr, n_cap)?;
                        stat!(self, alloc_count_closure += 1);
                        stat!(self, alloc_bytes_total += ((1 + n_cap) * 4) as u32);
                        self.record_heap();
                        self.arena.stack_push(val)?;
                        self.record_stack();
                    }
                }

                op::CALL => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();
                    if !func.is_callable() {
                        return Err(VmError::NotAClosure);
                    }
                    if func.is_foreign_fn() {
                        let f = self.foreign_fns[func.foreign_fn_idx() as usize]
                            .ok_or(VmError::InvalidBytecode)?;
                        let result = f(self, arg)?;
                        self.arena.stack_push(result)?;
                    } else {
                        if call_depth >= MAX_CALL_DEPTH {
                            return Err(VmError::StackOverflow);
                        }
                        self.call_stack[call_depth] = CallFrame {
                            return_pc: pc,
                            frame_base,
                            env,
                        };
                        call_depth += 1;
                        stat!(self, exec_call_count += 1);
                        stat!(self, exec_peak_call_depth = max call_depth as u32);

                        frame_base = self.arena.stack_bot_pos();
                        let ca = if func.is_bare_fn() {
                            env = Value::ctor(0, 0);
                            func.code_addr()
                        } else {
                            env = func;
                            self.arena.closure_code(func)
                        };
                        self.arena.stack_push(arg)?;
                        self.record_stack();
                        pc = ca as usize;
                    }
                }

                op::TAIL_CALL => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();
                    if !func.is_callable() {
                        return Err(VmError::NotAClosure);
                    }
                    if func.is_foreign_fn() {
                        let f = self.foreign_fns[func.foreign_fn_idx() as usize]
                            .ok_or(VmError::InvalidBytecode)?;
                        let result = f(self, arg)?;
                        self.arena.set_stack_bot_pos(frame_base);
                        self.arena.stack_push(result)?;
                        if call_depth == 0 {
                            return Ok(result);
                        }
                        call_depth -= 1;
                        pc = self.call_stack[call_depth].return_pc;
                        frame_base = self.call_stack[call_depth].frame_base;
                        env = self.call_stack[call_depth].env;
                    } else {
                        self.arena.set_stack_bot_pos(frame_base);
                        stat!(self, exec_tail_call_count += 1);

                        let ca = if func.is_bare_fn() {
                            env = Value::ctor(0, 0);
                            func.code_addr()
                        } else {
                            env = func;
                            self.arena.closure_code(func)
                        };
                        self.arena.stack_push(arg)?;
                        self.record_stack();
                        pc = ca as usize;
                    }
                }

                op::RET => {
                    let result = self.arena.stack_pop();
                    self.arena.set_stack_bot_pos(frame_base);
                    self.arena.stack_push(result)?;

                    if call_depth == 0 {
                        return Ok(result);
                    }
                    call_depth -= 1;
                    pc = self.call_stack[call_depth].return_pc;
                    frame_base = self.call_stack[call_depth].frame_base;
                    env = self.call_stack[call_depth].env;
                }

                op::MATCH => {
                    let n_cases = code[pc] as usize;
                    pc += 1;
                    let table_start = pc;
                    pc += n_cases * 4;

                    let scrutinee = self.arena.stack_pop();
                    let scrutinee_tag = scrutinee.tag();
                    stat!(self, exec_match_count += 1);

                    let mut matched = false;
                    for i in 0..n_cases {
                        let entry = table_start + i * 4;
                        let case_tag = code[entry];
                        let case_arity = code[entry + 1] as usize;
                        let case_offset =
                            u16::from_le_bytes([code[entry + 2], code[entry + 3]]) as usize;

                        if case_tag == scrutinee_tag {
                            if case_arity > 0 {
                                self.arena.stack_push(scrutinee)?;
                                self.record_stack();
                            }
                            pc = case_offset;
                            matched = true;
                            break;
                        }
                    }

                    if !matched {
                        return Err(VmError::MatchFailure { scrutinee_tag, pc });
                    }
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
                    self.record_stack();
                }

                op::DROP => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + n * 4);
                }

                op::SLIDE => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let result = self.arena.stack_pop();
                    let bot = self.arena.stack_bot_pos();
                    self.arena.set_stack_bot_pos(bot + n * 4);
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
                        self.arena.closure_set_capture(closure, cap_idx, closure);
                    }
                    self.arena.stack_set(1, closure);
                    self.arena.stack_pop();
                }

                op::INT_CONST => {
                    let n = i32::from_le_bytes([
                        code[pc],
                        code[pc + 1],
                        code[pc + 2],
                        code[pc + 3],
                    ]);
                    pc += 4;
                    self.arena.stack_push(Value::integer(n))?;
                    self.record_stack();
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
                    self.arena.stack_push(Value::ctor(tag, 0))?;
                }

                op::LT => {
                    let b = self.arena.stack_pop().integer_value();
                    let a = self.arena.stack_pop().integer_value();
                    let tag = if a < b { crate::value::tags::TRUE } else { crate::value::tags::FALSE };
                    self.arena.stack_push(Value::ctor(tag, 0))?;
                }

                op::BYTES_CONST => {
                    let len = code[pc] as usize;
                    pc += 1;
                    let val = self.arena.alloc_bytes(&code[pc..pc + len])?;
                    pc += len;
                    self.record_heap();
                    self.arena.stack_push(val)?;
                    self.record_stack();
                }

                op::BYTES_LEN => {
                    let v = self.arena.stack_pop();
                    self.arena.stack_push(Value::integer(v.bytes_len() as i32))?;
                }

                op::BYTES_GET => {
                    let idx = self.arena.stack_pop().integer_value() as usize;
                    let v = self.arena.stack_pop();
                    if idx >= v.bytes_len() {
                        return Err(VmError::IndexOutOfBounds);
                    }
                    let data = self.arena.bytes_data(v);
                    self.arena.stack_push(Value::integer(data[idx] as i32))?;
                }

                op::BYTES_EQ => {
                    let b = self.arena.stack_pop();
                    let a = self.arena.stack_pop();
                    let eq = a.bytes_len() == b.bytes_len()
                        && self.arena.bytes_data(a) == self.arena.bytes_data(b);
                    let tag = if eq { crate::value::tags::TRUE } else { crate::value::tags::FALSE };
                    self.arena.stack_push(Value::ctor(tag, 0))?;
                }

                op::BYTES_CONCAT => {
                    let b = self.arena.stack_pop();
                    let a = self.arena.stack_pop();
                    if a.bytes_len() + b.bytes_len() > 255 {
                        return Err(VmError::BytesOverflow);
                    }
                    let val = self.arena.bytes_concat(a, b)?;
                    self.record_heap();
                    self.arena.stack_push(val)?;
                    self.record_stack();
                }

                op::CALL_DIRECT => {
                    let code_addr = u16::from_le_bytes([code[pc], code[pc + 1]]);
                    let n_args = code[pc + 2] as usize;
                    pc += 3;
                    if call_depth >= MAX_CALL_DEPTH {
                        return Err(VmError::StackOverflow);
                    }
                    self.call_stack[call_depth] = CallFrame {
                        return_pc: pc,
                        frame_base,
                        env,
                    };
                    call_depth += 1;
                    stat!(self, exec_direct_call_count += 1);
                    stat!(self, exec_peak_call_depth = max call_depth as u32);
                    frame_base = self.arena.stack_bot_pos() + n_args * 4;
                    env = Value::ctor(0, 0);
                    pc = code_addr as usize;
                }

                op::TAIL_CALL_DIRECT => {
                    let code_addr = u16::from_le_bytes([code[pc], code[pc + 1]]);
                    let n_args = code[pc + 2] as usize;
                    stat!(self, exec_tail_direct_call_count += 1);
                    let mut args_buf: [Value; 16] = [Value::ctor(0, 0); 16];
                    for i in (0..n_args).rev() {
                        args_buf[i] = self.arena.stack_pop();
                    }
                    self.arena.set_stack_bot_pos(frame_base);
                    for i in 0..n_args {
                        self.arena.stack_write_at(frame_base - (i + 1) * 4, args_buf[i]);
                    }
                    self.arena.set_stack_bot_pos(frame_base - n_args * 4);
                    self.record_stack();
                    env = Value::ctor(0, 0);
                    pc = code_addr as usize;
                }

                op::FOREIGN_FN_CONST => {
                    let idx = u16::from_le_bytes([code[pc], code[pc + 1]]);
                    pc += 2;
                    self.arena.stack_push(Value::foreign_fn(idx))?;
                }

                _ => return Err(VmError::InvalidBytecode),
            }
        }
    }
}
