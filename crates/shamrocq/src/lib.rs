#![no_std]

pub mod arena;
pub mod stats;
pub mod value;
pub mod vm;

#[cfg(feature = "integration")]
pub static BYTECODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bytecode.bin"));

#[cfg(feature = "integration")]
pub mod funcs {
    include!(concat!(env!("OUT_DIR"), "/funcs.rs"));
}

#[cfg(feature = "integration")]
pub mod ctors {
    include!(concat!(env!("OUT_DIR"), "/ctors.rs"));
}

pub use stats::MemSnapshot;
#[cfg(feature = "stats")]
pub use stats::Stats;
pub use value::{tags, Value};
pub use vm::{ForeignFn, Program, Vm, VmError};
