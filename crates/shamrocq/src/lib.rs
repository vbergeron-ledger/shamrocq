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
pub use stats::{ArenaStats, ExecStats, Stats};
pub use value::{tags, Value};
pub use vm::{ForeignFn, Vm};

/// Declare a static byte buffer usable as a VM heap.
///
/// Expands to a `static mut` array and a function that returns
/// `&'static mut [u8]` from it. The function must only be called once.
///
/// ```ignore
/// shamrocq::static_heap!(HEAP, 40_000);
/// let buf: &'static mut [u8] = HEAP();
/// let mut vm = shamrocq::Vm::new(buf);
/// ```
#[macro_export]
macro_rules! static_heap {
    ($name:ident, $size:expr) => {
        #[allow(non_snake_case)]
        fn $name() -> &'static mut [u8] {
            static mut STORAGE: [u8; $size] = [0u8; $size];
            unsafe { &mut *(&raw mut STORAGE) }
        }
    };
}

/// Include a compiled shamrocq program (bytecode + generated bindings).
///
/// The argument is the directory containing `bytecode.bin` and `bindings.rs`
/// (typically `env!("OUT_DIR")` passed as a `const` string expression).
///
/// ```ignore
/// shamrocq::include_program!(env!("OUT_DIR"));
/// // makes available: BYTECODE, ctors, funcs, foreign
/// ```
#[macro_export]
macro_rules! include_program {
    ($dir:expr) => {
        static BYTECODE: &[u8] = include_bytes!(concat!($dir, "/bytecode.bin"));

        #[allow(dead_code)]
        mod bindings {
            include!(concat!($dir, "/bindings.rs"));
        }

        #[allow(unused_imports)]
        use bindings::{ctors, foreign, funcs};
    };
}
