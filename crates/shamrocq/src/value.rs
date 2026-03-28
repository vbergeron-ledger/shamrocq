const KIND_IMM: u32 = 0b00 << 30;
const KIND_TUPLE: u32 = 0b01 << 30;
const KIND_INTEGER: u32 = 0b10 << 30;
const KIND_CLOSURE: u32 = 0b110 << 29;
const KIND_BARE_FN: u32 = 0b111 << 29;

const TAG_SHIFT: u32 = 24;
const TAG_MASK: u32 = 0x3F;
const TUPLE_PAYLOAD_MASK: u32 = 0x00FF_FFFF;
const INTEGER_MASK: u32 = 0x3FFF_FFFF;
const CALLABLE_MASK: u32 = 0xC000_0000;
const CALLABLE_BITS: u32 = 0xC000_0000;
const CALLABLE_PAYLOAD: u32 = 0x1FFF_FFFF;
const KIND3_MASK: u32 = 0xE000_0000;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Value(u32);

impl Value {
    pub const fn immediate(tag: u8) -> Self {
        Value(KIND_IMM | ((tag as u32) << TAG_SHIFT))
    }

    pub const fn tuple(tag: u8, offset: usize) -> Self {
        Value(KIND_TUPLE | ((tag as u32) << TAG_SHIFT) | (offset as u32))
    }

    pub const fn integer(n: i32) -> Self {
        Value(KIND_INTEGER | ((n as u32) & INTEGER_MASK))
    }

    pub const fn closure(offset: usize) -> Self {
        Value(KIND_CLOSURE | (offset as u32))
    }

    pub const fn bare_fn(code_addr: u16) -> Self {
        Value(KIND_BARE_FN | (code_addr as u32))
    }

    pub const fn tag(self) -> u8 {
        ((self.0 >> TAG_SHIFT) & TAG_MASK) as u8
    }

    pub const fn offset(self) -> usize {
        (self.0 & TUPLE_PAYLOAD_MASK) as usize
    }

    pub const fn closure_offset(self) -> usize {
        (self.0 & CALLABLE_PAYLOAD) as usize
    }

    pub const fn code_addr(self) -> u16 {
        (self.0 & CALLABLE_PAYLOAD) as u16
    }

    pub const fn integer_value(self) -> i32 {
        ((self.0 << 2) as i32) >> 2
    }

    pub const fn is_immediate(self) -> bool {
        self.0 & CALLABLE_MASK == KIND_IMM
    }

    pub const fn is_tuple(self) -> bool {
        self.0 & CALLABLE_MASK == KIND_TUPLE
    }

    pub const fn is_integer(self) -> bool {
        self.0 & CALLABLE_MASK == KIND_INTEGER
    }

    pub const fn is_closure(self) -> bool {
        self.0 & KIND3_MASK == KIND_CLOSURE
    }

    pub const fn is_bare_fn(self) -> bool {
        self.0 & KIND3_MASK == KIND_BARE_FN
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
        if self.is_immediate() {
            write!(f, "Imm(tag={})", self.tag())
        } else if self.is_tuple() {
            write!(f, "Tuple(tag={}, @{})", self.tag(), self.offset())
        } else if self.is_integer() {
            write!(f, "Int({})", self.integer_value())
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
