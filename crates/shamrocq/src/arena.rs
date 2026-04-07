use crate::bytes;
use crate::stats::stat;
#[cfg(feature = "stats")]
use crate::stats::ArenaStats;
use crate::value::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum ArenaError {
    OutOfMemory,
}

// GC header layout: [0:1 | mark:1 | opaque:1 | fwd:16 | size:13]
const GC_MARK_BIT: u32 = 1 << 30;
const GC_OPAQUE_BIT: u32 = 1 << 29;
const GC_FWD_SHIFT: u32 = 13;
const GC_FWD_MASK: u32 = 0xFFFF;
const GC_SIZE_MASK: u32 = 0x1FFF;

const FRAME_HEADER_WORDS: usize = 3;

pub struct FrameHeader {
    pub pc: usize,
    pub frame_base: usize,
}

pub struct Arena<'a> {
    buf: &'a mut [u32],
    heap_top: usize,
    stack_bot: usize,
    #[cfg(feature = "stats")]
    pub stats: ArenaStats,
}

impl<'a> Arena<'a> {
    pub fn new(buf: &'a mut [u32]) -> Self {
        let len = buf.len();
        Arena {
            buf,
            heap_top: 0,
            stack_bot: len,
            #[cfg(feature = "stats")]
            stats: ArenaStats::default(),
        }
    }

    pub fn from_bytes(buf: &'a mut [u8]) -> Self {
        Self::new(bytes::as_words_mut(buf))
    }

    pub fn reset(&mut self) {
        self.heap_top = 0;
        self.stack_bot = self.buf.len();
    }

    pub fn alloc(&mut self, words: usize) -> Result<usize, ArenaError> {
        let base = self.heap_top;
        let end = base + words;
        if end > self.stack_bot {
            return Err(ArenaError::OutOfMemory);
        }
        self.heap_top = end;
        stat!(self, alloc_bytes_total += (words * 4) as u32);
        stat!(self, peak_heap_bytes = max self.heap_top * 4);
        Ok(base)
    }

    // -- GC header --

    fn write_gc_header(&mut self, offset: usize, opaque: bool, size: usize) {
        let w = if opaque { GC_OPAQUE_BIT } else { 0 } | (size as u32 & GC_SIZE_MASK);
        self.write_word(offset, w);
    }

    pub fn gc_object_size(&self, offset: usize) -> usize {
        (self.read_word(offset) & GC_SIZE_MASK) as usize
    }

    pub fn gc_is_opaque(&self, offset: usize) -> bool {
        (self.read_word(offset) & GC_OPAQUE_BIT) != 0
    }

    pub fn gc_set_mark(&mut self, offset: usize) {
        let w = self.read_word(offset);
        self.write_word(offset, w | GC_MARK_BIT);
    }

    pub fn gc_is_marked(&self, offset: usize) -> bool {
        (self.read_word(offset) & GC_MARK_BIT) != 0
    }

    pub fn gc_clear_mark(&mut self, offset: usize) {
        let w = self.read_word(offset);
        self.write_word(offset, w & !GC_MARK_BIT);
    }

    pub fn gc_set_forwarding(&mut self, offset: usize, dest: usize) {
        let w = self.read_word(offset);
        let cleared = w & !(GC_FWD_MASK << GC_FWD_SHIFT);
        self.write_word(offset, cleared | (((dest as u32) & GC_FWD_MASK) << GC_FWD_SHIFT));
    }

    pub fn gc_read_forwarding(&self, offset: usize) -> usize {
        ((self.read_word(offset) >> GC_FWD_SHIFT) & GC_FWD_MASK) as usize
    }

    pub fn gc_clear_forwarding(&mut self, offset: usize) {
        let w = self.read_word(offset);
        self.write_word(offset, w & !(GC_FWD_MASK << GC_FWD_SHIFT));
    }

    // -- Ctor --
    // Heap layout: [gc_header] [field_0] [field_1] ... [field_{arity-1}]
    // gc_header: opaque=0, size = 1 + arity

    pub fn alloc_ctor(&mut self, tag: u8, fields: &[Value]) -> Result<Value, ArenaError> {
        let n = fields.len();
        let offset = self.alloc(1 + n)?;
        self.write_gc_header(offset, false, 1 + n);
        for (i, &f) in fields.iter().enumerate() {
            self.write_word(offset + 1 + i, f.raw());
        }
        Ok(Value::ctor(tag, offset))
    }

    pub fn alloc_ctor_from_stack(&mut self, tag: u8, arity: usize) -> Result<Value, ArenaError> {
        let offset = self.alloc(1 + arity)?;
        self.write_gc_header(offset, false, 1 + arity);
        for i in (0..arity).rev() {
            let field = self.stack_pop();
            self.write_word(offset + 1 + i, field.raw());
        }
        stat!(self, alloc_count_ctor += 1);
        Ok(Value::ctor(tag, offset))
    }

    pub fn ctor_field(&self, val: Value, idx: usize) -> Value {
        let base = val.offset();
        Value::from_raw(self.read_word(base + 1 + idx))
    }

    pub fn ctor_arity(&self, val: Value) -> usize {
        self.gc_object_size(val.offset()) - 1
    }

    // -- Closure --
    // Heap layout: [gc_header] [closure_header] [bound_0] ... [bound_{n-1}]
    // gc_header: opaque=0, size = 2 + n_bound
    // closure_header: [code_addr:16 | arity:8 | n_bound:8]

    fn closure_header_offset(val: Value) -> usize {
        val.closure_offset() + 1
    }

    pub fn alloc_closure(
        &mut self,
        code_addr: u16,
        arity: u8,
        bound: &[Value],
    ) -> Result<Value, ArenaError> {
        let n = bound.len();
        let offset = self.alloc(2 + n)?;
        self.write_gc_header(offset, false, 2 + n);
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (n as u32);
        self.write_word(offset + 1, header);
        for (i, &v) in bound.iter().enumerate() {
            self.write_word(offset + 2 + i, v.raw());
        }
        Ok(Value::closure(offset))
    }

    pub fn alloc_closure_from_stack(
        &mut self,
        code_addr: u16,
        arity: u8,
        n_bound: usize,
    ) -> Result<Value, ArenaError> {
        let offset = self.alloc(2 + n_bound)?;
        self.write_gc_header(offset, false, 2 + n_bound);
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (n_bound as u32);
        self.write_word(offset + 1, header);
        for i in (0..n_bound).rev() {
            let val = self.stack_pop();
            self.write_word(offset + 2 + i, val.raw());
        }
        stat!(self, alloc_count_closure += 1);
        Ok(Value::closure(offset))
    }

    pub fn closure_code(&self, val: Value) -> u16 {
        let header = self.read_word(Self::closure_header_offset(val));
        (header >> 16) as u16
    }

    pub fn closure_arity(&self, val: Value) -> u8 {
        let header = self.read_word(Self::closure_header_offset(val));
        ((header >> 8) & 0xFF) as u8
    }

    pub fn closure_bound_count(&self, val: Value) -> usize {
        let header = self.read_word(Self::closure_header_offset(val));
        (header & 0xFF) as usize
    }

    pub fn closure_bound(&self, val: Value, idx: usize) -> Value {
        let base = val.closure_offset();
        Value::from_raw(self.read_word(base + 2 + idx))
    }

    pub fn closure_set_bound(&mut self, closure: Value, idx: usize, val: Value) {
        let base = closure.closure_offset();
        self.write_word(base + 2 + idx, val.raw());
    }

    pub fn extend_closure(
        &mut self,
        closure: Value,
        extra_arg: Value,
    ) -> Result<Value, ArenaError> {
        let old_bound = self.closure_bound_count(closure);
        let new_bound = old_bound + 1;
        let old_clo_header = self.read_word(Self::closure_header_offset(closure));
        let code_addr = (old_clo_header >> 16) as u16;
        let arity = ((old_clo_header >> 8) & 0xFF) as u8;
        let offset = self.alloc(2 + new_bound)?;
        self.write_gc_header(offset, false, 2 + new_bound);
        let header = ((code_addr as u32) << 16) | ((arity as u32) << 8) | (new_bound as u32);
        self.write_word(offset + 1, header);
        for i in 0..old_bound {
            let v = self.closure_bound(closure, i);
            self.write_word(offset + 2 + i, v.raw());
        }
        self.write_word(offset + 2 + old_bound, extra_arg.raw());
        Ok(Value::closure(offset))
    }

    // -- Bytes --
    // Heap layout: [gc_header] [len:32] [raw data...]
    // gc_header: opaque=1, size = 2 + ceil(len/4)

    pub fn alloc_bytes(&mut self, data: &[u8]) -> Result<Value, ArenaError> {
        let len = data.len();
        let data_words = (len + 3) / 4;
        let offset = self.alloc(2 + data_words)?;
        self.write_gc_header(offset, true, 2 + data_words);
        self.write_word(offset + 1, len as u32);
        let dst = bytes::words_as_bytes_mut(&mut self.buf[offset + 2..offset + 2 + data_words]);
        dst[..len].copy_from_slice(data);
        Ok(Value::bytes(offset))
    }

    pub fn bytes_len(&self, val: Value) -> usize {
        self.read_word(val.bytes_offset() + 1) as usize
    }

    pub fn bytes_data(&self, val: Value) -> &[u8] {
        let offset = val.bytes_offset();
        let len = self.read_word(offset + 1) as usize;
        let data_words = (len + 3) / 4;
        let word_slice = &self.buf[offset + 2..offset + 2 + data_words];
        &bytes::words_as_bytes(word_slice)[..len]
    }

    pub fn bytes_concat(&mut self, a: Value, b: Value) -> Result<Value, ArenaError> {
        let a_off = a.bytes_offset();
        let a_len = self.read_word(a_off + 1) as usize;
        let b_off = b.bytes_offset();
        let b_len = self.read_word(b_off + 1) as usize;
        let total = a_len + b_len;
        let data_words = (total + 3) / 4;
        let offset = self.alloc(2 + data_words)?;
        self.write_gc_header(offset, true, 2 + data_words);
        self.write_word(offset + 1, total as u32);
        let buf_ptr = self.buf.as_mut_ptr();
        unsafe {
            let dst = (buf_ptr.add(offset + 2)) as *mut u8;
            let src_a = (buf_ptr.add(a_off + 2)) as *const u8;
            let src_b = (buf_ptr.add(b_off + 2)) as *const u8;
            core::ptr::copy_nonoverlapping(src_a, dst, a_len);
            core::ptr::copy_nonoverlapping(src_b, dst.add(a_len), b_len);
        }
        Ok(Value::bytes(offset))
    }

    // -- frame management --

    pub fn stack_frame_load(&self, hdr: &FrameHeader, idx: usize) -> Value {
        Value::from_raw(self.buf[hdr.frame_base - (idx + 1)])
    }

    pub fn stack_frame_push(&mut self, hdr: &FrameHeader) -> Result<(), ArenaError> {
        self.stack_push(Value::from_raw(self.heap_top as u32))?;
        self.stack_push(Value::from_raw(hdr.pc as u32))?;
        self.stack_push(Value::from_raw(hdr.frame_base as u32))
    }

    pub fn stack_frame_pop(&mut self, hdr: &FrameHeader, result: Value) -> Result<FrameHeader, ArenaError> {
        let fb = self.stack_read_at(hdr.frame_base).raw() as usize;
        let pc = self.stack_read_at(hdr.frame_base + 1).raw() as usize;
        let heap = self.stack_read_at(hdr.frame_base + 2).raw() as usize;
        self.try_reclaim(result, heap);
        self.set_stack_bot_pos(hdr.frame_base + FRAME_HEADER_WORDS);
        self.stack_push(result)?;
        Ok(FrameHeader { pc, frame_base: fb })
    }

    fn try_reclaim(&mut self, result: Value, saved_heap_top: usize) {
        let current = self.heap_top;
        let refs_new_heap = result.is_reference() && result.offset() >= saved_heap_top;
        if current > saved_heap_top && !refs_new_heap {
            stat!(self, reclaim_count += 1);
            stat!(self, reclaim_bytes_total += ((current - saved_heap_top) * 4) as u32);
            self.heap_top = saved_heap_top;
        }
    }

    // -- stack (grows downward) --

    #[inline(always)]
    pub fn stack_push(&mut self, val: Value) -> Result<(), ArenaError> {
        if self.stack_bot <= self.heap_top {
            return Err(ArenaError::OutOfMemory);
        }
        self.stack_bot -= 1;
        self.buf[self.stack_bot] = val.raw();
        stat!(self, peak_stack_bytes = max (self.buf.len() - self.stack_bot) * 4);
        Ok(())
    }

    #[inline(always)]
    pub fn stack_push_unchecked(&mut self, val: Value) {
        self.stack_bot -= 1;
        self.buf[self.stack_bot] = val.raw();
        stat!(self, peak_stack_bytes = max (self.buf.len() - self.stack_bot) * 4);
    }

    #[inline(always)]
    pub fn stack_reserve(&self, n: usize) -> Result<(), ArenaError> {
        if self.stack_bot.wrapping_sub(n) <= self.heap_top {
            return Err(ArenaError::OutOfMemory);
        }
        Ok(())
    }

    #[inline(always)]
    pub fn stack_pop(&mut self) -> Value {
        let val = Value::from_raw(self.buf[self.stack_bot]);
        self.stack_bot += 1;
        val
    }

    #[inline(always)]
    pub fn stack_peek(&self, depth: usize) -> Value {
        Value::from_raw(self.buf[self.stack_bot + depth])
    }

    pub fn stack_set(&mut self, depth: usize, val: Value) {
        self.buf[self.stack_bot + depth] = val.raw();
    }

    pub fn stack_depth(&self) -> usize {
        self.buf.len() - self.stack_bot
    }

    pub fn stack_truncate(&mut self, depth: usize) {
        self.stack_bot = self.buf.len() - depth;
    }

    pub fn stack_bot_pos(&self) -> usize {
        self.stack_bot
    }

    pub fn set_stack_bot_pos(&mut self, pos: usize) {
        self.stack_bot = pos;
    }

    pub fn stack_read_at(&self, word_pos: usize) -> Value {
        Value::from_raw(self.buf[word_pos])
    }

    pub fn stack_write_at(&mut self, word_pos: usize, val: Value) {
        self.buf[word_pos] = val.raw();
    }

    // -- raw word access --

    #[inline(always)]
    pub(crate) fn write_word(&mut self, offset: usize, val: u32) {
        self.buf[offset] = val;
    }

    #[inline(always)]
    pub(crate) fn read_word(&self, offset: usize) -> u32 {
        self.buf[offset]
    }

    pub fn heap_used(&self) -> usize {
        self.heap_top
    }

    pub fn set_heap_top(&mut self, pos: usize) {
        self.heap_top = pos;
    }

    pub fn stack_used(&self) -> usize {
        self.buf.len() - self.stack_bot
    }

    pub fn free(&self) -> usize {
        self.stack_bot - self.heap_top
    }

    pub fn buf_len(&self) -> usize {
        self.buf.len()
    }

    pub fn heap_data(&self) -> &[u32] {
        &self.buf[..self.heap_top]
    }

    pub fn stack_data(&self) -> &[u32] {
        &self.buf[self.stack_bot..]
    }
}
