#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprint, hprintln};
use panic_halt as _;

use shamrocq::{Program, Value, Vm, VmError};

static BYTECODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bytecode.bin"));

mod funcs {
    include!(concat!(env!("OUT_DIR"), "/funcs.rs"));
}
mod ctors {
    include!(concat!(env!("OUT_DIR"), "/ctors.rs"));
}
mod foreign {
    include!(concat!(env!("OUT_DIR"), "/foreign_fns.rs"));
}

fn print_int(_vm: &mut Vm<'_>, arg: Value) -> Result<Value, VmError> {
    let _ = hprintln!("{}", arg.integer_value());
    Ok(arg)
}

fn make_list(vm: &mut Vm, items: &[Value]) -> Value {
    let mut list = Value::ctor(ctors::NIL, 0);
    for &item in items.iter().rev() {
        list = vm.alloc_ctor(ctors::CONS, &[item, list]).unwrap();
    }
    list
}

fn print_int_list(vm: &Vm, mut v: Value) {
    let _ = hprint!("[");
    let mut first = true;
    while v.tag() == ctors::CONS {
        if !first {
            let _ = hprint!(", ");
        }
        let _ = hprint!("{}", vm.ctor_field(v, 0).integer_value());
        v = vm.ctor_field(v, 1);
        first = false;
    }
    let _ = hprintln!("]");
}

#[entry]
fn main() -> ! {
    let mut buf = [0u8; 10240];
    let prog = Program::from_blob(BYTECODE).unwrap();
    let mut vm = Vm::new(&mut buf);

    vm.register_foreign(foreign::PRINT_INT, print_int);
    vm.load_program(&prog).unwrap();

    // fib(10) = 55
    let r = vm.call(funcs::FIB, &[Value::integer(10)]).unwrap();
    let _ = hprintln!("fib(10) = {}", r.integer_value());

    // factorial(10) = 3628800
    let r = vm.call(funcs::FACTORIAL, &[Value::integer(10)]).unwrap();
    let _ = hprintln!("factorial(10) = {}", r.integer_value());

    // sum(range(1, 11)) = 55
    let nums = vm
        .call(funcs::RANGE, &[Value::integer(1), Value::integer(11)])
        .unwrap();
    let r = vm.call(funcs::SUM, &[nums]).unwrap();
    let _ = hprintln!("sum(1..11) = {}", r.integer_value());

    // map(fib, [1,2,3,4,5,6,7,8]) — built and printed without any heap allocator
    let items = [
        Value::integer(1),
        Value::integer(2),
        Value::integer(3),
        Value::integer(4),
        Value::integer(5),
        Value::integer(6),
        Value::integer(7),
        Value::integer(8),
    ];
    let list = make_list(&mut vm, &items);
    let fib_val = vm.global_value(funcs::FIB);
    let mapped = vm.call(funcs::MAP, &[fib_val, list]).unwrap();
    let _ = hprint!("map(fib, 1..=8) = ");
    print_int_list(&vm, mapped);

    // length
    let list2 = make_list(&mut vm, &items);
    let len = vm.call(funcs::LENGTH, &[list2]).unwrap();
    let _ = hprintln!("length = {}", len.integer_value());

    // FFI round-trip
    let _ = hprint!("print-int via FFI: ");
    vm.call(funcs::PRINT_INT, &[Value::integer(42)]).unwrap();

    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}
