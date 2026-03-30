/// Bytecode opcodes for the shamrocq VM.
///
/// Encoding: each instruction starts with a 1-byte opcode, followed by
/// inline operands of fixed size per opcode. All multi-byte operands are
/// little-endian.
///
/// Instruction layouts:
///
///   ── Stack / locals ──
///   LOAD              idx:u8
///   LOAD2             idx_a:u8  idx_b:u8
///   LOAD3             idx_a:u8  idx_b:u8  idx_c:u8
///   LOAD_CAPTURE      idx:u8
///   GLOBAL            idx:u16le
///   DROP              n:u8
///   SLIDE             n:u8
///
///   ── Data ──
///   PACK              tag:u8  arity:u8
///   UNPACK            n:u8
///   BIND              n:u8
///   FUNCTION          idx:u16le  arity:u8
///   CLOSURE           code_addr:u16le  arity:u8  n_captures:u8
///   FIXPOINT          cap_idx:u8
///
///   ── Control flow ──
///   CALL
///   TAIL_CALL
///   CALL_DIRECT       code_addr:u16le  n_args:u8
///   TAIL_CALL_DIRECT  code_addr:u16le  n_args:u8
///   RET
///   MATCH             n_cases:u8  [tag:u8 arity:u8 offset:u16le]*n
///   JMP               offset:u16le
///   ERROR
///
///   ── Integer ──
///   INT               value:i32le
///   ADD
///   SUB
///   MUL
///   DIV
///   NEG
///   EQ
///   LT
///
///   ── Bytes ──
///   BYTES             len:u8  data:[u8]
///   BYTES_LEN
///   BYTES_GET
///   BYTES_EQ
///   BYTES_CONCAT
pub mod op {
    // Stack / locals
    pub const LOAD: u8 = 0x01;
    pub const LOAD2: u8 = 0x02;
    pub const LOAD3: u8 = 0x03;
    pub const LOAD_CAPTURE: u8 = 0x04;
    pub const GLOBAL: u8 = 0x05;
    pub const DROP: u8 = 0x06;
    pub const SLIDE: u8 = 0x07;

    // Data
    pub const PACK: u8 = 0x08;
    pub const UNPACK: u8 = 0x09;
    pub const BIND: u8 = 0x0A;
    pub const FUNCTION: u8 = 0x0B;
    pub const CLOSURE: u8 = 0x0C;
    pub const FIXPOINT: u8 = 0x0D;

    // Control flow
    pub const CALL: u8 = 0x0E;
    pub const TAIL_CALL: u8 = 0x0F;
    pub const CALL_DIRECT: u8 = 0x10;
    pub const TAIL_CALL_DIRECT: u8 = 0x11;
    pub const RET: u8 = 0x12;
    pub const MATCH: u8 = 0x13;
    pub const JMP: u8 = 0x14;
    pub const ERROR: u8 = 0x15;

    // Integer
    pub const INT: u8 = 0x16;
    pub const ADD: u8 = 0x17;
    pub const SUB: u8 = 0x18;
    pub const MUL: u8 = 0x19;
    pub const DIV: u8 = 0x1A;
    pub const NEG: u8 = 0x1B;
    pub const EQ: u8 = 0x1C;
    pub const LT: u8 = 0x1D;

    // Bytes
    pub const BYTES: u8 = 0x1E;
    pub const BYTES_LEN: u8 = 0x1F;
    pub const BYTES_GET: u8 = 0x20;
    pub const BYTES_EQ: u8 = 0x21;
    pub const BYTES_CONCAT: u8 = 0x22;
}

pub struct Emitter {
    pub code: Vec<u8>,
    pending_loads: Vec<u8>,
}

impl Emitter {
    pub fn new() -> Self {
        Emitter { code: Vec::new(), pending_loads: Vec::new() }
    }

    pub fn pos(&self) -> usize {
        debug_assert!(self.pending_loads.is_empty(), "pos() called with pending LOADs");
        self.code.len()
    }

    fn flush_pending_loads(&mut self) {
        let loads = &self.pending_loads;
        let mut i = 0;
        while i < loads.len() {
            let remaining = loads.len() - i;
            if remaining >= 3 {
                self.code.push(op::LOAD3);
                self.code.push(loads[i]);
                self.code.push(loads[i + 1]);
                self.code.push(loads[i + 2]);
                i += 3;
            } else if remaining >= 2 {
                self.code.push(op::LOAD2);
                self.code.push(loads[i]);
                self.code.push(loads[i + 1]);
                i += 2;
            } else {
                self.code.push(op::LOAD);
                self.code.push(loads[i]);
                i += 1;
            }
        }
        self.pending_loads.clear();
    }

    // Stack / locals

    pub fn emit_load(&mut self, idx: u8) {
        self.pending_loads.push(idx);
        if self.pending_loads.len() >= 3 {
            self.flush_pending_loads();
        }
    }

    pub fn emit_load_capture(&mut self, idx: u8) {
        self.flush_pending_loads();
        self.code.push(op::LOAD_CAPTURE);
        self.code.push(idx);
    }

    pub fn emit_global(&mut self, idx: u16) {
        self.flush_pending_loads();
        self.code.push(op::GLOBAL);
        self.code.extend_from_slice(&idx.to_le_bytes());
    }

    pub fn emit_drop(&mut self, n: u8) {
        self.flush_pending_loads();
        self.code.push(op::DROP);
        self.code.push(n);
    }

    /// Keep top-of-stack, remove n items below it.
    pub fn emit_slide(&mut self, n: u8) {
        self.flush_pending_loads();
        self.code.push(op::SLIDE);
        self.code.push(n);
    }

    // Data

    pub fn emit_pack(&mut self, tag: u8, arity: u8) {
        self.flush_pending_loads();
        self.code.push(op::PACK);
        self.code.push(tag);
        self.code.push(arity);
    }

    pub fn emit_unpack(&mut self, n: u8) {
        self.flush_pending_loads();
        self.code.push(op::UNPACK);
        self.code.push(n);
    }

    pub fn emit_bind(&mut self, n: u8) {
        self.flush_pending_loads();
        self.code.push(op::BIND);
        self.code.push(n);
    }

    pub fn emit_function(&mut self, idx: u16, arity: u8) {
        self.flush_pending_loads();
        self.code.push(op::FUNCTION);
        self.code.extend_from_slice(&idx.to_le_bytes());
        self.code.push(arity);
    }

    pub fn emit_closure(&mut self, code_addr: u16, arity: u8, n_captures: u8) {
        self.flush_pending_loads();
        self.code.push(op::CLOSURE);
        self.code.extend_from_slice(&code_addr.to_le_bytes());
        self.code.push(arity);
        self.code.push(n_captures);
    }

    /// FIXPOINT(cap_idx): Peek TOS (a closure), patch its
    /// capture[cap_idx] to point to itself, overwrite the slot
    /// 1 below TOS with the closure, then pop TOS.
    pub fn emit_fixpoint(&mut self, cap_idx: u8) {
        self.flush_pending_loads();
        self.code.push(op::FIXPOINT);
        self.code.push(cap_idx);
    }

    // Control flow

    pub fn emit_call(&mut self) {
        self.flush_pending_loads();
        self.code.push(op::CALL);
    }

    pub fn emit_tail_call(&mut self) {
        self.flush_pending_loads();
        self.code.push(op::TAIL_CALL);
    }

    pub fn emit_call_direct(&mut self, code_addr: u16, n_args: u8) {
        self.flush_pending_loads();
        self.code.push(op::CALL_DIRECT);
        self.code.extend_from_slice(&code_addr.to_le_bytes());
        self.code.push(n_args);
    }

    pub fn emit_tail_call_direct(&mut self, code_addr: u16, n_args: u8) {
        self.flush_pending_loads();
        self.code.push(op::TAIL_CALL_DIRECT);
        self.code.extend_from_slice(&code_addr.to_le_bytes());
        self.code.push(n_args);
    }

    pub fn emit_ret(&mut self) {
        self.flush_pending_loads();
        self.code.push(op::RET);
    }

    /// Emits a MATCH header. Returns the position of the case table
    /// so callers can patch jump offsets after emitting branches.
    pub fn emit_match_header(&mut self, n_cases: u8) -> usize {
        self.flush_pending_loads();
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
        self.flush_pending_loads();
        self.code.push(op::JMP);
        self.code.extend_from_slice(&offset.to_le_bytes());
    }

    /// Emits JMP with a placeholder. Returns position of the offset for patching.
    pub fn emit_jmp_placeholder(&mut self) -> usize {
        self.flush_pending_loads();
        self.code.push(op::JMP);
        let pos = self.code.len();
        self.code.extend_from_slice(&[0u8; 2]);
        pos
    }

    pub fn patch_u16(&mut self, pos: usize, val: u16) {
        self.code[pos..pos + 2].copy_from_slice(&val.to_le_bytes());
    }

    pub fn emit_error(&mut self) {
        self.flush_pending_loads();
        self.code.push(op::ERROR);
    }

    // Integer

    pub fn emit_int(&mut self, n: i32) {
        self.flush_pending_loads();
        self.code.push(op::INT);
        self.code.extend_from_slice(&n.to_le_bytes());
    }

    pub fn emit_add(&mut self) { self.flush_pending_loads(); self.code.push(op::ADD); }
    pub fn emit_sub(&mut self) { self.flush_pending_loads(); self.code.push(op::SUB); }
    pub fn emit_mul(&mut self) { self.flush_pending_loads(); self.code.push(op::MUL); }
    pub fn emit_div(&mut self) { self.flush_pending_loads(); self.code.push(op::DIV); }
    pub fn emit_neg(&mut self) { self.flush_pending_loads(); self.code.push(op::NEG); }
    pub fn emit_eq(&mut self)  { self.flush_pending_loads(); self.code.push(op::EQ); }
    pub fn emit_lt(&mut self)  { self.flush_pending_loads(); self.code.push(op::LT); }

    // Bytes

    pub fn emit_bytes(&mut self, data: &[u8]) {
        self.flush_pending_loads();
        self.code.push(op::BYTES);
        self.code.push(data.len() as u8);
        self.code.extend_from_slice(data);
    }
    pub fn emit_bytes_len(&mut self)    { self.flush_pending_loads(); self.code.push(op::BYTES_LEN); }
    pub fn emit_bytes_get(&mut self)    { self.flush_pending_loads(); self.code.push(op::BYTES_GET); }
    pub fn emit_bytes_eq(&mut self)     { self.flush_pending_loads(); self.code.push(op::BYTES_EQ); }
    pub fn emit_bytes_concat(&mut self) { self.flush_pending_loads(); self.code.push(op::BYTES_CONCAT); }
}

pub const MAGIC: [u8; 4] = *b"SMRQ";
pub const BYTECODE_VERSION: u16 = 2;

/// Header prepended to the compiled bytecode blob.
/// All offsets are byte offsets into the code section.
#[derive(Debug, Clone)]
pub struct ProgramHeader {
    pub n_globals: u16,
    /// (name, code_offset) for each top-level define.
    /// The index in this vec is the global slot index.
    pub globals: Vec<(String, u16)>,
    /// Constructor tag names, indexed by tag id. Empty if not embedded.
    pub tags: Vec<String>,
}

impl ProgramHeader {
    pub fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&BYTECODE_VERSION.to_le_bytes());
        out.extend_from_slice(&self.n_globals.to_le_bytes());
        for (name, offset) in &self.globals {
            let name_bytes = name.as_bytes();
            assert!(name_bytes.len() < 256);
            out.push(name_bytes.len() as u8);
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(&offset.to_le_bytes());
        }
        out.extend_from_slice(&(self.tags.len() as u16).to_le_bytes());
        for name in &self.tags {
            let name_bytes = name.as_bytes();
            assert!(name_bytes.len() < 256);
            out.push(name_bytes.len() as u8);
            out.extend_from_slice(name_bytes);
        }
    }

    pub fn serialized_len(&self) -> usize {
        4 + 2  // magic + version
          + 2 + self.globals.iter().map(|(n, _)| 1 + n.len() + 2).sum::<usize>()
          + 2 + self.tags.iter().map(|n| 1 + n.len()).sum::<usize>()
    }
}
