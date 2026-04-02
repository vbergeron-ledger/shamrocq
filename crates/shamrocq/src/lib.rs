#![no_std]

pub mod arena;
pub mod bytes;
pub mod gc;
pub mod program;
pub mod stats;
pub mod value;
pub mod vm;

pub use program::{Program, VmError};
pub use stats::MemSnapshot;
#[cfg(feature = "stats")]
pub use stats::Stats;
pub use value::{tags, Value};
pub use vm::{ForeignFn, Vm};
