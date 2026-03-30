// Value kind tags (3 bits, bits 31:29)
//
//   0xx = data
//     000  Ctor         tag:8 | offset:21
//     001  Integer      value:29 (sign-extended)
//     010  Bytes        len:8 | offset:21
//     011  (unused)
//
//   1xx = callable
//     100  (unused)
//     101  Application  offset:21           heap: [arity:4|applied:4|kind:3|payload:21], arg[0..applied-1]
//     110  Closure      offset:21           heap: [code_addr:16|arity:8|n_cap:8], cap[0..n_cap-1]
//     111  Function     foreign:1|arity:4|addr:16   (no heap)

const KIND_CTOR: u32 = 0b000 << 29;
const KIND_INTEGER: u32 = 0b001 << 29;
const KIND_BYTES: u32 = 0b010 << 29;
const KIND_APPLICATION: u32 = 0b101 << 29;
const KIND_CLOSURE: u32 = 0b110 << 29;
const KIND_FUNCTION: u32 = 0b111 << 29;

const KIND_MASK: u32 = 0b111 << 29;
const CALLABLE_BIT: u32 = 1 << 31;
const TAG_SHIFT: u32 = 21;
const TAG_MASK: u32 = 0xFF;
const PAYLOAD_21: u32 = 0x001F_FFFF;
const PAYLOAD_29: u32 = 0x1FFF_FFFF;

const FN_FOREIGN_BIT: u32 = 1 << 20;
const FN_ARITY_SHIFT: u32 = 16;
const FN_ARITY_MASK: u32 = 0xF;
const FN_ADDR_MASK: u32 = 0xFFFF;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Value(u32);

impl Value {
    // -- Ctor: 000 | tag:8 | offset:21 --

    pub const fn ctor(tag: u8, byte_offset: usize) -> Self {
        Value(KIND_CTOR | ((tag as u32) << TAG_SHIFT) | ((byte_offset >> 2) as u32))
    }

    pub const fn is_ctor(self) -> bool {
        self.0 & KIND_MASK == KIND_CTOR
    }

    pub const fn tag(self) -> u8 {
        ((self.0 >> TAG_SHIFT) & TAG_MASK) as u8
    }

    pub const fn offset(self) -> usize {
        ((self.0 & PAYLOAD_21) as usize) << 2
    }

    // -- Integer: 001 | value:29 --

    pub const fn integer(n: i32) -> Self {
        Value(KIND_INTEGER | ((n as u32) & PAYLOAD_29))
    }

    pub const fn is_integer(self) -> bool {
        self.0 & KIND_MASK == KIND_INTEGER
    }

    pub const fn integer_value(self) -> i32 {
        ((self.0 << 3) as i32) >> 3
    }

    // -- Bytes: 010 | len:8 | offset:21 --

    pub const fn bytes(len: u8, byte_offset: usize) -> Self {
        Value(KIND_BYTES | ((len as u32) << TAG_SHIFT) | ((byte_offset >> 2) as u32))
    }

    pub const fn is_bytes(self) -> bool {
        self.0 & KIND_MASK == KIND_BYTES
    }

    pub const fn bytes_len(self) -> usize {
        ((self.0 >> TAG_SHIFT) & TAG_MASK) as usize
    }

    pub const fn bytes_offset(self) -> usize {
        ((self.0 & PAYLOAD_21) as usize) << 2
    }

    // -- Application: 101 | offset:21 --

    pub const fn application(byte_offset: usize) -> Self {
        Value(KIND_APPLICATION | ((byte_offset >> 2) as u32))
    }

    pub const fn is_application(self) -> bool {
        self.0 & KIND_MASK == KIND_APPLICATION
    }

    pub const fn application_offset(self) -> usize {
        ((self.0 & PAYLOAD_21) as usize) << 2
    }

    // -- Closure: 110 | offset:21 --

    pub const fn closure(byte_offset: usize) -> Self {
        Value(KIND_CLOSURE | ((byte_offset >> 2) as u32))
    }

    pub const fn is_closure(self) -> bool {
        self.0 & KIND_MASK == KIND_CLOSURE
    }

    pub const fn closure_offset(self) -> usize {
        ((self.0 & PAYLOAD_21) as usize) << 2
    }

    // -- Function: 111 | foreign:1 | arity:4 | addr:16 --

    pub const fn function(code_addr: u16, arity: u8) -> Self {
        Value(KIND_FUNCTION | ((arity as u32 & FN_ARITY_MASK) << FN_ARITY_SHIFT) | (code_addr as u32))
    }

    pub const fn foreign_fn(idx: u16, arity: u8) -> Self {
        Value(KIND_FUNCTION | FN_FOREIGN_BIT | ((arity as u32 & FN_ARITY_MASK) << FN_ARITY_SHIFT) | (idx as u32))
    }

    pub const fn is_function(self) -> bool {
        self.0 & KIND_MASK == KIND_FUNCTION
    }

    pub const fn is_foreign_fn(self) -> bool {
        self.is_function() && (self.0 & FN_FOREIGN_BIT) != 0
    }

    pub const fn fn_arity(self) -> u8 {
        ((self.0 >> FN_ARITY_SHIFT) & FN_ARITY_MASK) as u8
    }

    pub const fn fn_addr(self) -> u16 {
        (self.0 & FN_ADDR_MASK) as u16
    }

    // -- Callable detection: bit 31 == 1 (kinds 100..111) --

    pub const fn is_callable(self) -> bool {
        (self.0 & CALLABLE_BIT) != 0
    }

    // -- Raw access --

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn from_raw(raw: u32) -> Self {
        Value(raw)
    }
}

impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_ctor() {
            write!(f, "Ctor(tag={}, @{})", self.tag(), self.offset())
        } else if self.is_integer() {
            write!(f, "Int({})", self.integer_value())
        } else if self.is_bytes() {
            write!(f, "Bytes(len={}, @{})", self.bytes_len(), self.bytes_offset())
        } else if self.is_application() {
            write!(f, "App(@{})", self.application_offset())
        } else if self.is_closure() {
            write!(f, "Closure(@{})", self.closure_offset())
        } else if self.is_foreign_fn() {
            write!(f, "ForeignFn(idx={}, arity={})", self.fn_addr(), self.fn_arity())
        } else if self.is_function() {
            write!(f, "Fn(pc={}, arity={})", self.fn_addr(), self.fn_arity())
        } else {
            write!(f, "Unknown(0x{:08X})", self.0)
        }
    }
}

pub mod tags {
    pub const TRUE: u8 = 0;
    pub const FALSE: u8 = 1;
}
