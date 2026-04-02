#![no_std]

pub mod op;
pub mod tags;

pub const MAGIC: [u8; 4] = *b"SMRQ";
pub const BYTECODE_VERSION: u16 = 11;

pub const DUMP_MAGIC: [u8; 4] = *b"SMRD";
pub const DUMP_VERSION: u16 = 1;
