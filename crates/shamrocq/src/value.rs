const KIND_CTOR: u32 = 0b000 << 29;
const KIND_INTEGER: u32 = 0b001 << 29;
const KIND_BYTES: u32 = 0b010 << 29;
const KIND_CLOSURE: u32 = 0b110 << 29;
const KIND_BARE_FN: u32 = 0b111 << 29;

const KIND_MASK: u32 = 0b111 << 29;
const CALLABLE_MASK: u32 = 0b11 << 30;
const CALLABLE_BITS: u32 = 0b11 << 30;
const TAG_SHIFT: u32 = 21;
const TAG_MASK: u32 = 0xFF;
const PAYLOAD_21: u32 = 0x001F_FFFF;
const PAYLOAD_29: u32 = 0x1FFF_FFFF;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Value(u32);

impl Value {
    pub const fn ctor(tag: u8, byte_offset: usize) -> Self {
        Value(KIND_CTOR | ((tag as u32) << TAG_SHIFT) | ((byte_offset >> 2) as u32))
    }

    pub const fn integer(n: i32) -> Self {
        Value(KIND_INTEGER | ((n as u32) & PAYLOAD_29))
    }

    pub const fn closure(byte_offset: usize) -> Self {
        Value(KIND_CLOSURE | ((byte_offset >> 2) as u32))
    }

    pub const fn bare_fn(code_addr: u16) -> Self {
        Value(KIND_BARE_FN | (code_addr as u32))
    }

    pub const fn tag(self) -> u8 {
        ((self.0 >> TAG_SHIFT) & TAG_MASK) as u8
    }

    pub const fn offset(self) -> usize {
        ((self.0 & PAYLOAD_21) as usize) << 2
    }

    pub const fn closure_offset(self) -> usize {
        ((self.0 & PAYLOAD_21) as usize) << 2
    }

    pub const fn code_addr(self) -> u16 {
        (self.0 & PAYLOAD_21) as u16
    }

    pub const fn integer_value(self) -> i32 {
        ((self.0 << 3) as i32) >> 3
    }

    pub const fn is_ctor(self) -> bool {
        self.0 & KIND_MASK == KIND_CTOR
    }

    pub const fn is_integer(self) -> bool {
        self.0 & KIND_MASK == KIND_INTEGER
    }

    pub const fn is_closure(self) -> bool {
        self.0 & KIND_MASK == KIND_CLOSURE
    }

    pub const fn is_bare_fn(self) -> bool {
        self.0 & KIND_MASK == KIND_BARE_FN
    }

    pub const fn bytes(len: u8, byte_offset: usize) -> Self {
        Value(KIND_BYTES | ((len as u32) << TAG_SHIFT) | ((byte_offset >> 2) as u32))
    }

    pub const fn bytes_len(self) -> usize {
        ((self.0 >> TAG_SHIFT) & TAG_MASK) as usize
    }

    pub const fn bytes_offset(self) -> usize {
        ((self.0 & PAYLOAD_21) as usize) << 2
    }

    pub const fn is_bytes(self) -> bool {
        self.0 & KIND_MASK == KIND_BYTES
    }

    pub const fn is_callable(self) -> bool {
        self.0 & CALLABLE_MASK == CALLABLE_BITS
    }

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
        } else if self.is_bare_fn() {
            write!(f, "Fn(pc={})", self.code_addr())
        } else {
            write!(f, "Closure(@{})", self.closure_offset())
        }
    }
}

pub mod tags {
    pub const TRUE: u8 = 0;
    pub const FALSE: u8 = 1;
}
