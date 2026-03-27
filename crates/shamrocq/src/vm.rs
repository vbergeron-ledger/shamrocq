use crate::arena::{Arena, ArenaError};
use crate::stats::stat;
#[cfg(feature = "stats")]
use crate::stats::Stats;
use crate::stats::MemSnapshot;
use crate::value::Value;

mod op {
    pub const IMM: u8 = 0x01;
    pub const TUPLE: u8 = 0x02;
    pub const LOAD: u8 = 0x03;
    pub const GLOBAL: u8 = 0x04;
    pub const CLOSURE: u8 = 0x05;
    pub const APPLY: u8 = 0x06;
    pub const TAIL_APPLY: u8 = 0x07;
    pub const RET: u8 = 0x08;
    pub const MATCH: u8 = 0x09;
    pub const JMP: u8 = 0x0A;
    pub const BIND: u8 = 0x0B;
    pub const DROP: u8 = 0x0C;
    pub const ERROR: u8 = 0x0D;
    pub const SLIDE: u8 = 0x0E;
    pub const FIXPOINT: u8 = 0x0F;
}

#[derive(Debug)]
pub enum VmError {
    Oom,
    MatchFailure { scrutinee_tag: u8, pc: usize },
    InvalidBytecode,
    NotAClosure,
    StackOverflow,
}

impl From<ArenaError> for VmError {
    fn from(_: ArenaError) -> Self {
        VmError::Oom
    }
}

pub struct Program<'a> {
    pub n_globals: u16,
    pub global_names: &'a [u8],
    pub code: &'a [u8],
}

impl<'a> Program<'a> {
    pub fn from_blob(blob: &'a [u8]) -> Result<Self, VmError> {
        if blob.len() < 2 {
            return Err(VmError::InvalidBytecode);
        }
        let n_globals = u16::from_le_bytes([blob[0], blob[1]]);
        let mut pos = 2usize;
        for _ in 0..n_globals {
            if pos >= blob.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = blob[pos] as usize;
            pos += 1 + name_len + 2;
        }
        Ok(Program {
            n_globals,
            global_names: &blob[2..pos],
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
    frame_base: usize,
}

impl CallFrame {
    fn fresh() -> Self {
        CallFrame {
            return_pc: 0,
            frame_base: 0,
        }
    }
}

pub struct Vm<'buf> {
    pub arena: Arena<'buf>,
    globals: [Value; 64],
    n_globals: u16,
    code: &'buf [u8],
    call_stack: [CallFrame; MAX_CALL_DEPTH],
    #[cfg(feature = "stats")]
    pub stats: Stats,
}

const MAX_CALL_DEPTH: usize = 256;

impl<'buf> Vm<'buf> {
    pub fn new(buf: &'buf mut [u8]) -> Self {
        Vm {
            arena: Arena::new(buf),
            globals: [Value::immediate(0); 64],
            n_globals: 0,
            code: &[],
            call_stack: core::array::from_fn(|_| CallFrame::fresh()),
            #[cfg(feature = "stats")]
            stats: Stats::default(),
        }
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
            if !func.is_closure() {
                return Err(VmError::NotAClosure);
            }
            let code_addr = self.arena.closure_code(func);
            let n_cap = self.arena.closure_capture_count(func);
            let saved_depth = self.arena.stack_depth();
            for i in 0..n_cap {
                self.arena.stack_push(self.arena.closure_capture(func, i))?;
            }
            self.arena.stack_push(arg)?;
            func = self.eval_with_frame(code_addr as usize, saved_depth)?;
        }
        Ok(func)
    }

    pub fn tuple_field(&self, val: Value, idx: usize) -> Value {
        self.arena.tuple_field(val, idx)
    }

    pub fn nil(&self) -> Value {
        Value::immediate(crate::value::tags::NIL)
    }

    pub fn alloc_tuple(&mut self, tag: u8, fields: &[Value]) -> Result<Value, VmError> {
        Ok(self.arena.alloc_tuple(tag, fields)?)
    }

    fn record_heap(&mut self) {
        stat!(self, peak_heap_bytes = max self.arena.heap_used());
    }

    fn record_stack(&mut self) {
        stat!(self, peak_stack_bytes = max self.arena.stack_used());
    }

    fn eval(&mut self, start_pc: usize) -> Result<Value, VmError> {
        self.eval_with_frame(start_pc, self.arena.stack_depth())
    }

    fn eval_with_frame(&mut self, start_pc: usize, frame_base: usize) -> Result<Value, VmError> {
        let code = self.code;
        let mut pc = start_pc;
        let mut call_depth: usize = 0;
        let mut frame_base = frame_base;

        loop {
            if pc >= code.len() {
                return Err(VmError::InvalidBytecode);
            }

            stat!(self, exec_instruction_count += 1);

            let opcode = code[pc];
            pc += 1;

            match opcode {
                op::IMM => {
                    let tag = code[pc];
                    pc += 1;
                    self.arena.stack_push(Value::immediate(tag))?;
                    self.record_stack();
                }

                op::TUPLE => {
                    let tag = code[pc];
                    let arity = code[pc + 1] as usize;
                    pc += 2;
                    let val = self.arena.alloc_tuple_from_stack(tag, arity)?;
                    stat!(self, alloc_count_tuple += 1);
                    stat!(self, alloc_bytes_total += (arity * 4) as u32);
                    self.record_heap();
                    self.arena.stack_push(val)?;
                    self.record_stack();
                }

                op::LOAD => {
                    let idx = code[pc] as usize;
                    pc += 1;
                    let depth_from_top = self.arena.stack_depth() - frame_base - 1 - idx;
                    let val = self.arena.stack_peek(depth_from_top);
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
                    let val = self.arena.alloc_closure_from_stack(code_addr, n_cap)?;
                    stat!(self, alloc_count_closure += 1);
                    stat!(self, alloc_bytes_total += ((1 + n_cap) * 4) as u32);
                    self.record_heap();
                    self.arena.stack_push(val)?;
                    self.record_stack();
                }

                op::APPLY => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();
                    if !func.is_closure() {
                        return Err(VmError::NotAClosure);
                    }
                    if call_depth >= MAX_CALL_DEPTH {
                        return Err(VmError::StackOverflow);
                    }
                    self.call_stack[call_depth] = CallFrame {
                        return_pc: pc,
                        frame_base,
                    };
                    call_depth += 1;
                    stat!(self, exec_apply_count += 1);
                    stat!(self, exec_peak_call_depth = max call_depth as u32);

                    let ca = self.arena.closure_code(func);
                    let n_cap = self.arena.closure_capture_count(func);
                    frame_base = self.arena.stack_depth();
                    for i in 0..n_cap {
                        self.arena.stack_push(self.arena.closure_capture(func, i))?;
                    }
                    self.arena.stack_push(arg)?;
                    self.record_stack();
                    pc = ca as usize;
                }

                op::TAIL_APPLY => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();
                    if !func.is_closure() {
                        return Err(VmError::NotAClosure);
                    }
                    self.arena.stack_truncate(frame_base);
                    stat!(self, exec_tail_apply_count += 1);

                    let ca = self.arena.closure_code(func);
                    let n_cap = self.arena.closure_capture_count(func);
                    frame_base = self.arena.stack_depth();
                    for i in 0..n_cap {
                        self.arena.stack_push(self.arena.closure_capture(func, i))?;
                    }
                    self.arena.stack_push(arg)?;
                    self.record_stack();
                    pc = ca as usize;
                }

                op::RET => {
                    let result = self.arena.stack_pop();
                    self.arena.stack_truncate(frame_base);
                    self.arena.stack_push(result)?;

                    if call_depth == 0 {
                        return Ok(result);
                    }
                    call_depth -= 1;
                    pc = self.call_stack[call_depth].return_pc;
                    frame_base = self.call_stack[call_depth].frame_base;
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
                        return Err(VmError::MatchFailure { scrutinee_tag: scrutinee_tag, pc });
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
                        let field = self.arena.tuple_field(scrutinee, i);
                        self.arena.stack_push(field)?;
                    }
                    self.record_stack();
                }

                op::DROP => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let depth = self.arena.stack_depth();
                    self.arena.stack_truncate(depth - n);
                }

                op::SLIDE => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let result = self.arena.stack_pop();
                    let depth = self.arena.stack_depth();
                    self.arena.stack_truncate(depth - n);
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

                _ => return Err(VmError::InvalidBytecode),
            }
        }
    }
}
