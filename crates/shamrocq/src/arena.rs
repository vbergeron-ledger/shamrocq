use crate::value::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum ArenaError {
    OutOfMemory,
}

pub struct Arena<'a> {
    buf: &'a mut [u8],
    heap_top: usize,
    stack_bot: usize,
}

impl<'a> Arena<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        let len = buf.len();
        Arena {
            buf,
            heap_top: 0,
            stack_bot: len,
        }
    }

    pub fn reset(&mut self) {
        self.heap_top = 0;
        self.stack_bot = self.buf.len();
    }

    fn align4(offset: usize) -> usize {
        (offset + 3) & !3
    }

    pub fn alloc(&mut self, words: usize) -> Result<usize, ArenaError> {
        let base = Self::align4(self.heap_top);
        let end = base + words * 4;
        if end > self.stack_bot {
            return Err(ArenaError::OutOfMemory);
        }
        self.heap_top = end;
        Ok(base)
    }

    pub fn alloc_ctor(&mut self, tag: u8, fields: &[Value]) -> Result<Value, ArenaError> {
        let offset = self.alloc(fields.len())?;
        for (i, &f) in fields.iter().enumerate() {
            self.write_word(offset + i * 4, f.raw());
        }
        Ok(Value::ctor(tag, offset))
    }

    /// Allocate a constructor by popping `arity` values from the arena stack.
    /// Fields are expected on the stack in order (first field deepest, last on top).
    pub fn alloc_ctor_from_stack(&mut self, tag: u8, arity: usize) -> Result<Value, ArenaError> {
        let offset = self.alloc(arity)?;
        for i in (0..arity).rev() {
            let field = self.stack_pop();
            self.write_word(offset + i * 4, field.raw());
        }
        Ok(Value::ctor(tag, offset))
    }

    pub fn alloc_closure(
        &mut self,
        code_addr: u16,
        captures: &[Value],
    ) -> Result<Value, ArenaError> {
        let n = captures.len();
        let offset = self.alloc(1 + n)?;
        self.write_word(offset, ((code_addr as u32) << 16) | (n as u32));
        for (i, &c) in captures.iter().enumerate() {
            self.write_word(offset + (1 + i) * 4, c.raw());
        }
        Ok(Value::closure(offset))
    }

    /// Allocate a closure by popping `n_cap` capture values from the arena stack.
    /// Captures are expected on the stack in order (first capture deepest, last on top).
    pub fn alloc_closure_from_stack(
        &mut self,
        code_addr: u16,
        n_cap: usize,
    ) -> Result<Value, ArenaError> {
        let offset = self.alloc(1 + n_cap)?;
        self.write_word(offset, ((code_addr as u32) << 16) | (n_cap as u32));
        for i in (0..n_cap).rev() {
            let cap = self.stack_pop();
            self.write_word(offset + (1 + i) * 4, cap.raw());
        }
        Ok(Value::closure(offset))
    }

    pub fn ctor_field(&self, val: Value, idx: usize) -> Value {
        let base = val.offset();
        Value::from_raw(self.read_word(base + idx * 4))
    }

    pub fn closure_code(&self, val: Value) -> u16 {
        let header = self.read_word(val.closure_offset());
        (header >> 16) as u16
    }

    pub fn closure_capture_count(&self, val: Value) -> usize {
        let header = self.read_word(val.closure_offset());
        (header & 0xFFFF) as usize
    }

    pub fn closure_capture(&self, val: Value, idx: usize) -> Value {
        let base = val.closure_offset();
        Value::from_raw(self.read_word(base + (1 + idx) * 4))
    }

    pub fn closure_set_capture(&mut self, closure: Value, idx: usize, val: Value) {
        let base = closure.closure_offset();
        self.write_word(base + (1 + idx) * 4, val.raw());
    }

    pub fn alloc_bytes(&mut self, data: &[u8]) -> Result<Value, ArenaError> {
        let len = data.len();
        let words = (len + 3) / 4;
        let offset = self.alloc(words)?;
        self.buf[offset..offset + len].copy_from_slice(data);
        Ok(Value::bytes(len as u8, offset))
    }

    pub fn bytes_data(&self, val: Value) -> &[u8] {
        let offset = val.bytes_offset();
        let len = val.bytes_len();
        &self.buf[offset..offset + len]
    }

    pub fn bytes_concat(&mut self, a: Value, b: Value) -> Result<Value, ArenaError> {
        let a_off = a.bytes_offset();
        let a_len = a.bytes_len();
        let b_off = b.bytes_offset();
        let b_len = b.bytes_len();
        let total = a_len + b_len;
        let words = (total + 3) / 4;
        let offset = self.alloc(words)?;
        self.buf.copy_within(a_off..a_off + a_len, offset);
        self.buf.copy_within(b_off..b_off + b_len, offset + a_len);
        Ok(Value::bytes(total as u8, offset))
    }

    // -- stack (grows downward) --

    pub fn stack_push(&mut self, val: Value) -> Result<(), ArenaError> {
        if self.stack_bot < self.heap_top + 4 {
            return Err(ArenaError::OutOfMemory);
        }
        self.stack_bot -= 4;
        self.write_word(self.stack_bot, val.raw());
        Ok(())
    }

    pub fn stack_pop(&mut self) -> Value {
        let val = Value::from_raw(self.read_word(self.stack_bot));
        self.stack_bot += 4;
        val
    }

    pub fn stack_peek(&self, depth: usize) -> Value {
        Value::from_raw(self.read_word(self.stack_bot + depth * 4))
    }

    pub fn stack_set(&mut self, depth: usize, val: Value) {
        self.write_word(self.stack_bot + depth * 4, val.raw());
    }

    pub fn stack_depth(&self) -> usize {
        (self.buf.len() - self.stack_bot) / 4
    }

    pub fn stack_truncate(&mut self, depth: usize) {
        self.stack_bot = self.buf.len() - depth * 4;
    }

    // -- raw word access (little-endian) --

    fn write_word(&mut self, offset: usize, val: u32) {
        let bytes = val.to_le_bytes();
        self.buf[offset..offset + 4].copy_from_slice(&bytes);
    }

    fn read_word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.buf[offset..offset + 4].try_into().unwrap())
    }

    pub fn heap_used(&self) -> usize {
        self.heap_top
    }

    pub fn stack_used(&self) -> usize {
        self.buf.len() - self.stack_bot
    }

    pub fn free(&self) -> usize {
        self.stack_bot - self.heap_top
    }
}
