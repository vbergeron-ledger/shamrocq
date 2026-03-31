pub use shamrocq_bytecode::op;

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

    pub fn emit_call1(&mut self) {
        self.flush_pending_loads();
        self.code.push(op::CALL1);
    }

    pub fn emit_tail_call1(&mut self) {
        self.flush_pending_loads();
        self.code.push(op::TAIL_CALL1);
    }

    pub fn emit_call_n(&mut self, code_addr: u16, n_args: u8) {
        self.flush_pending_loads();
        self.code.push(op::CALL_N);
        self.code.extend_from_slice(&code_addr.to_le_bytes());
        self.code.push(n_args);
    }

    /// Emits CALL_N with a placeholder code_addr. Returns position of the u16.
    pub fn emit_call_n_placeholder(&mut self, n_args: u8) -> usize {
        self.flush_pending_loads();
        self.code.push(op::CALL_N);
        let pos = self.code.len();
        self.code.extend_from_slice(&[0u8; 2]);
        self.code.push(n_args);
        pos
    }

    pub fn emit_tail_call_n(&mut self, code_addr: u16, n_args: u8) {
        self.flush_pending_loads();
        self.code.push(op::TAIL_CALL_N);
        self.code.extend_from_slice(&code_addr.to_le_bytes());
        self.code.push(n_args);
    }

    /// Emits TAIL_CALL_N with a placeholder code_addr. Returns position of the u16.
    pub fn emit_tail_call_n_placeholder(&mut self, n_args: u8) -> usize {
        self.flush_pending_loads();
        self.code.push(op::TAIL_CALL_N);
        let pos = self.code.len();
        self.code.extend_from_slice(&[0u8; 2]);
        self.code.push(n_args);
        pos
    }

    pub fn emit_ret(&mut self) {
        self.flush_pending_loads();
        self.code.push(op::RET);
    }

    /// Emits a MATCH jump-table header. Returns the position of the table
    /// so callers can patch entries after emitting branches.
    ///
    /// Entries are initialized with sentinel offset 0xFFFF. The caller must
    /// patch each real case via `patch_match_entry`.
    pub fn emit_match_header(&mut self, base_tag: u8, n_entries: u8) -> usize {
        self.flush_pending_loads();
        self.code.push(op::MATCH);
        self.code.push(base_tag);
        self.code.push(n_entries);
        let table_start = self.code.len();
        for _ in 0..n_entries {
            self.code.extend_from_slice(&[0x00, 0xFF, 0xFF]);
        }
        table_start
    }

    pub fn patch_match_entry(&mut self, table_start: usize, slot: usize, arity: u8, offset: u16) {
        let pos = table_start + slot * 3;
        self.code[pos] = arity;
        self.code[pos + 1..pos + 3].copy_from_slice(&offset.to_le_bytes());
    }

    pub fn match_entry_is_sentinel(&self, table_start: usize, slot: usize) -> bool {
        let pos = table_start + slot * 3;
        self.code[pos + 1] == 0xFF && self.code[pos + 2] == 0xFF
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

pub use shamrocq_bytecode::{MAGIC, BYTECODE_VERSION};

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
