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
///   DUP
///   OVER
///
///   -- Data --
///   PACK0             tag:u8
///   PACK              tag:u8  arity:u8
///   UNPACK            n:u8
///   BIND              n:u8
///   FOREIGN           idx:u16le  arity:u8
///   FUNCTION          code_addr:u16le  arity:u8
///   CLOSURE           code_addr:u16le  arity:u8  n_captures:u8
///   FIXPOINT          cap_idx:u8
///
///   -- Control flow (calls) --
///   CALL_DYNAMIC
///   TAIL_CALL_DYNAMIC
///   CALL              code_addr:u16le  n_args:u8
///   TAIL_CALL         code_addr:u16le  n_args:u8
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

macro_rules! opcodes {
    ($($name:ident = $val:expr),* $(,)?) => {
        $(pub const $name: u8 = $val;)*

        pub fn name(opcode: u8) -> &'static str {
            match opcode {
                $($val => stringify!($name),)*
                _ => "?",
            }
        }
    };
}

opcodes! {
    // Stack / locals
    LOAD              = 0x01,
    LOAD2             = 0x02,
    LOAD3             = 0x03,
    GLOBAL            = 0x05,
    DROP              = 0x06,
    SLIDE             = 0x07,
    // Data
    PACK              = 0x08,
    UNPACK            = 0x09,
    BIND              = 0x0A,
    FOREIGN           = 0x0B,
    CLOSURE           = 0x0C,
    FIXPOINT          = 0x0D,
    // Control flow (calls)
    CALL_DYNAMIC      = 0x0E,
    TAIL_CALL_DYNAMIC = 0x0F,
    CALL              = 0x10,
    TAIL_CALL         = 0x11,
    RET               = 0x12,
    // Control flow (branching)
    MATCH             = 0x13,
    JMP               = 0x14,
    ERROR             = 0x15,
    // Integer
    INT               = 0x16,
    ADD               = 0x17,
    SUB               = 0x18,
    MUL               = 0x19,
    DIV               = 0x1A,
    NEG               = 0x1B,
    EQ                = 0x1C,
    LT                = 0x1D,
    // Bytes
    BYTES             = 0x1E,
    BYTES_LEN         = 0x1F,
    BYTES_GET         = 0x20,
    BYTES_EQ          = 0x21,
    BYTES_CONCAT      = 0x22,
    // Specialized
    PACK0             = 0x23,
    INT0              = 0x24,
    INT1              = 0x25,
    SLIDE1            = 0x26,
    MATCH2            = 0x27,
    FUNCTION          = 0x28,
    DUP               = 0x29,
    OVER              = 0x2A,
}
