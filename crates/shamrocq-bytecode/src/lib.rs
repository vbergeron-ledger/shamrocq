#![no_std]

pub mod op;
pub mod tags;

pub const MAGIC: [u8; 4] = *b"SMRQ";
pub const BYTECODE_VERSION: u16 = 3;
