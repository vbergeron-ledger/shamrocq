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

fn list_length(vm: &Vm, mut v: Value) -> i32 {
    let mut n = 0;
    while v.tag() == ctors::CONS {
        v = vm.ctor_field(v, 1);
        n += 1;
    }
    n
}

static mut HEAP: [u8; 40_000] = [0; 40_000];

#[entry]
fn main() -> ! {
    let buf = unsafe { &raw mut HEAP }.cast::<[u8; 40_000]>();
    let buf = unsafe { &mut *buf };
    let prog = Program::from_blob(BYTECODE)
        .unwrap_or_else(|e| vm_exit_err(e));
    let mut vm = Vm::new(buf);
    vm.load_program(&prog).unwrap_or_else(|e| vm_exit_err(e));

    let n = 50;

    let sorted = vm
        .call(funcs::SORT_SEQ, &[Value::integer(n)])
        .unwrap_or_else(|e| vm_exit_err(e));
    let len = list_length(&vm, sorted);
    let _ = hprintln!("merge_sort(rev_range({})) -> {} elements", n, len);
    let _ = hprintln!("{}", vm.stats);

    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}
