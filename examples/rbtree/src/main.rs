#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use panic_halt as _;

use shamrocq::{Program, Value, Vm, VmError};

static BYTECODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bytecode.bin"));

mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
use bindings::{ctors, funcs};

fn vm_exit_err(e: VmError) -> ! {
    let _ = hprintln!("VM error: {:?}", e);
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}

static mut HEAP: [u8; 40_000] = [0; 40_000];

#[entry]
fn main() -> ! {
    let buf = unsafe { &mut HEAP };
    let prog = Program::from_blob(BYTECODE)
        .unwrap_or_else(|e| vm_exit_err(e));
    let mut vm = Vm::new(buf);
    vm.load_program(&prog).unwrap_or_else(|e| vm_exit_err(e));

    let n = 50;

    let tree = vm.call(funcs::BUILD_TREE, &[Value::integer(n)])
        .unwrap_or_else(|e| vm_exit_err(e));

    let d = vm.call(funcs::DEPTH, &[tree])
        .unwrap_or_else(|e| vm_exit_err(e));
    let _ = hprintln!("build_tree({}) depth = {}", n, d.integer_value());

    let s = vm.call(funcs::SIZE, &[tree])
        .unwrap_or_else(|e| vm_exit_err(e));
    let _ = hprintln!("build_tree({}) size  = {}", n, s.integer_value());

    let ok = vm.call(funcs::BUILD_AND_CHECK, &[Value::integer(n)])
        .unwrap_or_else(|e| vm_exit_err(e));
    let _ = hprintln!(
        "build_and_check({}) = {}",
        n,
        if ok.tag() == ctors::TRUE { "true" } else { "false" }
    );

    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}
