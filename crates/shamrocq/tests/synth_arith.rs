mod common;

use common::{compile_scheme, peano, unpeano, print_stats, Compiled};
use shamrocq::{Program, Vm};

fn setup() -> Compiled {
    compile_scheme(&["synth_arith.scm"])
}

#[test]
fn add_zero_right() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let n0 = peano(&mut vm, c.tag("O"), c.tag("S"), 0);
    let result = vm.call(c.func("add"), &[n3, n0]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 3);
    print_stats("add(3,0)", &vm);
}

#[test]
fn add_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let result = vm.call(c.func("add"), &[n2, n3]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 5);
    print_stats("add(2,3)", &vm);
}

#[test]
fn mul_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let n4 = peano(&mut vm, c.tag("O"), c.tag("S"), 4);
    let result = vm.call(c.func("mul"), &[n3, n4]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 12);
    print_stats("mul(3,4)", &vm);
}

#[test]
fn mul_by_zero() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n5 = peano(&mut vm, c.tag("O"), c.tag("S"), 5);
    let n0 = peano(&mut vm, c.tag("O"), c.tag("S"), 0);
    let result = vm.call(c.func("mul"), &[n0, n5]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 0);
    print_stats("mul(0,5)", &vm);
}

#[test]
fn sub_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n5 = peano(&mut vm, c.tag("O"), c.tag("S"), 5);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let result = vm.call(c.func("sub"), &[n5, n2]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 3);
    print_stats("sub(5,2)", &vm);
}

#[test]
fn sub_truncated() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n5 = peano(&mut vm, c.tag("O"), c.tag("S"), 5);
    let result = vm.call(c.func("sub"), &[n2, n5]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 0);
    print_stats("sub(2,5)", &vm);
}

#[test]
fn min_nat_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let n7 = peano(&mut vm, c.tag("O"), c.tag("S"), 7);
    let result = vm.call(c.func("min_nat"), &[n3, n7]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 3);
    print_stats("min_nat(3,7)", &vm);
}

#[test]
fn max_nat_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let n7 = peano(&mut vm, c.tag("O"), c.tag("S"), 7);
    let result = vm.call(c.func("max_nat"), &[n3, n7]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 7);
    print_stats("max_nat(3,7)", &vm);
}
