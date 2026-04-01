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

fn build_tlv_stream(out: &mut [u8]) -> usize {
    let mut pos = 0;
    let mut i: u8 = 0;
    while pos + 5 <= out.len() {
        out[pos] = 0x01;
        out[pos + 1] = 0x00;
        out[pos + 2] = 0x02;
        out[pos + 3] = 0x00;
        out[pos + 4] = i;
        pos += 5;
        i = i.wrapping_add(1);
    }
    pos
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

    let mut tlv_buf = [0u8; 1000];
    let len = build_tlv_stream(&mut tlv_buf);

    let input = vm.arena.alloc_bytes(&tlv_buf[..len])
        .unwrap_or_else(|e| vm_exit_err(e.into()));
    let fuel = Value::integer(10_000);

    let result = vm.call(funcs::PARSE_BUFFER, &[input, fuel])
        .unwrap_or_else(|e| vm_exit_err(e));

    let _ = hprintln!("result tag = {}", result.tag());
    if result.tag() == ctors::INL {
        let pair = vm.ctor_field(result, 0);
        let items = vm.ctor_field(pair, 1);
        let count = vm.call(funcs::COUNT_TLVS, &[items])
            .unwrap_or_else(|e| vm_exit_err(e));
        let _ = hprintln!("parsed {} TLV items from {} bytes", count.integer_value(), len);
    } else if result.tag() == ctors::INR {
        let err = vm.ctor_field(result, 0);
        let _ = hprintln!("parse error: tag={}", err.tag());
    } else {
        let _ = hprintln!("unexpected result tag: {}", result.tag());
    }
    let _ = hprintln!("{}", vm.stats);

    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}
