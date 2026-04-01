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

fn read_option_nat(vm: &Vm, v: Value) -> Option<i32> {
    if v.tag() == ctors::SOME {
        Some(vm.ctor_field(v, 0).integer_value())
    } else {
        None
    }
}

static mut HEAP: [u8; 40_000] = [0; 40_000];

#[entry]
fn main() -> ! {
    let buf = unsafe { &mut HEAP };
    let prog = Program::from_blob(BYTECODE)
        .unwrap_or_else(|e| vm_exit_err(e));
    let mut vm = Vm::new(buf);
    vm.load_program(&prog).unwrap_or_else(|e| vm_exit_err(e));

    for &(a, b, f) in &[(1i32, 1i32, 100i32), (1, 2, 200), (2, 2, 500), (2, 3, 1000)] {
        let r = vm
            .call(funcs::TEST_ADD, &[Value::integer(a), Value::integer(b), Value::integer(f)])
            .unwrap_or_else(|e| vm_exit_err(e));
        match read_option_nat(&vm, r) {
            Some(n) => { let _ = hprintln!("church {} + {} (fuel={}) = {}", a, b, f, n); }
            None =>    { let _ = hprintln!("church {} + {} (fuel={}) = timeout", a, b, f); }
        }
    }

    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}
