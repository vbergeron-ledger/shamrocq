/// Bytecode opcodes for the shamrocq VM.
///
/// Encoding: each instruction starts with a 1-byte opcode, followed by
/// inline operands of fixed size per opcode. All multi-byte operands are
/// little-endian.
pub mod op {
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
    pub const INT_CONST: u8 = 0x10;
    pub const ADD: u8 = 0x11;
    pub const SUB: u8 = 0x12;
    pub const MUL: u8 = 0x13;
    pub const DIV: u8 = 0x14;
    pub const NEG: u8 = 0x15;
    pub const EQ: u8 = 0x16;
    pub const LT: u8 = 0x17;
}

/// Binary encoding helpers used by the compiler to emit bytecode,
/// and by the runtime to decode it.
///
/// Instruction layouts:
///   IMM       tag:u8
///   TUPLE     tag:u8  arity:u8
///   LOAD      idx:u8
///   GLOBAL    idx:u16le
///   CLOSURE   code_addr:u16le  n_captures:u8
///   APPLY
///   TAIL_APPLY
///   RET
///   MATCH     n_cases:u8  [tag:u8 arity:u8 offset:u16le]*n
///   JMP       offset:u16le
///   BIND      n:u8
///   DROP      n:u8
///   ERROR

pub struct Emitter {
    pub code: Vec<u8>,
}

impl Emitter {
    pub fn new() -> Self {
        Emitter { code: Vec::new() }
    }

    pub fn pos(&self) -> usize {
        self.code.len()
    }

    pub fn emit_imm(&mut self, tag: u8) {
        self.code.push(op::IMM);
        self.code.push(tag);
    }

    pub fn emit_tuple(&mut self, tag: u8, arity: u8) {
        self.code.push(op::TUPLE);
        self.code.push(tag);
        self.code.push(arity);
    }

    pub fn emit_load(&mut self, idx: u8) {
        self.code.push(op::LOAD);
        self.code.push(idx);
    }

    pub fn emit_global(&mut self, idx: u16) {
        self.code.push(op::GLOBAL);
        self.code.extend_from_slice(&idx.to_le_bytes());
    }

    pub fn emit_closure(&mut self, code_addr: u16, n_captures: u8) {
        self.code.push(op::CLOSURE);
        self.code.extend_from_slice(&code_addr.to_le_bytes());
        self.code.push(n_captures);
    }

    pub fn emit_apply(&mut self) {
        self.code.push(op::APPLY);
    }

    pub fn emit_tail_apply(&mut self) {
        self.code.push(op::TAIL_APPLY);
    }

    pub fn emit_ret(&mut self) {
        self.code.push(op::RET);
    }

    /// Emits a MATCH header. Returns the position of the case table
    /// so callers can patch jump offsets after emitting branches.
    pub fn emit_match_header(&mut self, n_cases: u8) -> usize {
        self.code.push(op::MATCH);
        self.code.push(n_cases);
        let table_start = self.code.len();
        for _ in 0..n_cases {
            self.code.extend_from_slice(&[0u8; 4]); // tag:u8 arity:u8 offset:u16le
        }
        table_start
    }

    pub fn patch_match_case(&mut self, table_start: usize, case_idx: usize, tag: u8, arity: u8, offset: u16) {
        let pos = table_start + case_idx * 4;
        self.code[pos] = tag;
        self.code[pos + 1] = arity;
        self.code[pos + 2..pos + 4].copy_from_slice(&offset.to_le_bytes());
    }

    pub fn emit_jmp(&mut self, offset: u16) {
        self.code.push(op::JMP);
        self.code.extend_from_slice(&offset.to_le_bytes());
    }

    /// Emits JMP with a placeholder. Returns position of the offset for patching.
    pub fn emit_jmp_placeholder(&mut self) -> usize {
        self.code.push(op::JMP);
        let pos = self.code.len();
        self.code.extend_from_slice(&[0u8; 2]);
        pos
    }

    pub fn patch_u16(&mut self, pos: usize, val: u16) {
        self.code[pos..pos + 2].copy_from_slice(&val.to_le_bytes());
    }

    pub fn emit_bind(&mut self, n: u8) {
        self.code.push(op::BIND);
        self.code.push(n);
    }

    pub fn emit_drop(&mut self, n: u8) {
        self.code.push(op::DROP);
        self.code.push(n);
    }

    /// Keep top-of-stack, remove n items below it.
    pub fn emit_slide(&mut self, n: u8) {
        self.code.push(op::SLIDE);
        self.code.push(n);
    }

    pub fn emit_error(&mut self) {
        self.code.push(op::ERROR);
    }

    /// FIXPOINT(cap_idx): Peek TOS (a closure), patch its
    /// capture[cap_idx] to point to itself, overwrite the slot
    /// 1 below TOS with the closure, then pop TOS.
    pub fn emit_fixpoint(&mut self, cap_idx: u8) {
        self.code.push(op::FIXPOINT);
        self.code.push(cap_idx);
    }

    pub fn emit_int_const(&mut self, n: i32) {
        self.code.push(op::INT_CONST);
        self.code.extend_from_slice(&n.to_le_bytes());
    }

    pub fn emit_add(&mut self) { self.code.push(op::ADD); }
    pub fn emit_sub(&mut self) { self.code.push(op::SUB); }
    pub fn emit_mul(&mut self) { self.code.push(op::MUL); }
    pub fn emit_div(&mut self) { self.code.push(op::DIV); }
    pub fn emit_neg(&mut self) { self.code.push(op::NEG); }
    pub fn emit_eq(&mut self)  { self.code.push(op::EQ); }
    pub fn emit_lt(&mut self)  { self.code.push(op::LT); }
}

/// Header prepended to the compiled bytecode blob.
/// All offsets are byte offsets into the code section.
#[derive(Debug, Clone)]
pub struct ProgramHeader {
    pub n_globals: u16,
    /// (name, code_offset) for each top-level define.
    /// The index in this vec is the global slot index.
    pub globals: Vec<(String, u16)>,
}

impl ProgramHeader {
    pub fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.n_globals.to_le_bytes());
        for (name, offset) in &self.globals {
            let name_bytes = name.as_bytes();
            assert!(name_bytes.len() < 256);
            out.push(name_bytes.len() as u8);
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(&offset.to_le_bytes());
        }
    }

    pub fn serialized_len(&self) -> usize {
        2 + self.globals.iter().map(|(n, _)| 1 + n.len() + 2).sum::<usize>()
    }
}
