/// Bytecode opcodes for the shamrocq VM.
///
/// Encoding: each instruction starts with a 1-byte opcode, followed by
/// inline operands of fixed size per opcode. All multi-byte operands are
/// little-endian.
///
/// Instruction layouts:
///
///   -- Stack / locals --
///   LOAD              idx:u8
///   LOAD2             idx_a:u8  idx_b:u8
///   LOAD3             idx_a:u8  idx_b:u8  idx_c:u8
///   GLOBAL            idx:u16le
///   DROP              n:u8
///   SLIDE1
///   SLIDE             n:u8
///
///   -- Data --
///   PACK0             tag:u8
///   PACK              tag:u8  arity:u8
///   UNPACK            n:u8
///   BIND              n:u8
///   FUNCTION          idx:u16le  arity:u8
///   CLOSURE           code_addr:u16le  arity:u8  n_captures:u8
///   FIXPOINT          cap_idx:u8
///
///   -- Control flow (calls) --
///   CALL1
///   TAIL_CALL1
///   CALL_N            code_addr:u16le  n_args:u8
///   TAIL_CALL_N       code_addr:u16le  n_args:u8
///   RET
///
///   -- Control flow (branching) --
///   MATCH2            base_tag:u8  [arity:u8 offset:u16le]*2
///   MATCH             base_tag:u8  n_entries:u8  [arity:u8 offset:u16le]*n
///   JMP               offset:u16le
///   ERROR
///
///   -- Integer --
///   INT0
///   INT1
///   INT               value:i32le
///   ADD
///   SUB
///   MUL
///   DIV
///   NEG
///   EQ
///   LT
///
///   -- Bytes --
///   BYTES             len:u8  data:[u8]
///   BYTES_LEN
///   BYTES_GET
///   BYTES_EQ
///   BYTES_CONCAT

// Stack / locals
pub const LOAD: u8 = 0x01;
pub const LOAD2: u8 = 0x02;
pub const LOAD3: u8 = 0x03;
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

// Control flow (calls)
pub const CALL1: u8 = 0x0E;
pub const TAIL_CALL1: u8 = 0x0F;
pub const CALL_N: u8 = 0x10;
pub const TAIL_CALL_N: u8 = 0x11;
pub const RET: u8 = 0x12;

// Control flow (branching)
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

// Specialized
pub const PACK0: u8 = 0x23;
pub const INT0: u8 = 0x24;
pub const INT1: u8 = 0x25;
pub const SLIDE1: u8 = 0x26;
pub const MATCH2: u8 = 0x27;
