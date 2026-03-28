mod common;

use common::{compile_scheme, print_stats, Compiled};
use shamrocq::{Program, Value, Vm};

fn i(n: i32) -> Value {
    Value::integer(n)
}

fn as_int(v: Value) -> i32 {
    v.integer_value()
}

fn setup_int() -> Compiled {
    compile_scheme(&["synth_int.scm"])
}

#[test]
fn value_integer_roundtrip() {
    for &n in &[0, 1, -1, 42, -42, 1000, -1000, 0x1FFF_FFFF, -0x2000_0000] {
        let v = Value::integer(n);
        assert!(v.is_integer());
        assert!(!v.is_immediate());
        assert!(!v.is_tuple());
        assert!(!v.is_callable());
        assert_eq!(v.integer_value(), n, "roundtrip failed for {}", n);
    }
}

#[test]
fn int_abs_basic() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = c.funcs["int_abs"];
    assert_eq!(as_int(vm.call(f, &[i(-42)]).unwrap()), 42);
    assert_eq!(as_int(vm.call(f, &[i(42)]).unwrap()), 42);
    assert_eq!(as_int(vm.call(f, &[i(0)]).unwrap()), 0);
}

#[test]
fn int_max_min() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let max = c.funcs["int_max"];
    let min = c.funcs["int_min"];
    assert_eq!(as_int(vm.call(max, &[i(3), i(7)]).unwrap()), 7);
    assert_eq!(as_int(vm.call(max, &[i(7), i(3)]).unwrap()), 7);
    assert_eq!(as_int(vm.call(min, &[i(3), i(7)]).unwrap()), 3);
    assert_eq!(as_int(vm.call(min, &[i(-5), i(2)]).unwrap()), -5);
}

#[test]
fn int_factorial() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = c.funcs["int_factorial"];
    assert_eq!(as_int(vm.call(f, &[i(0)]).unwrap()), 1);
    assert_eq!(as_int(vm.call(f, &[i(1)]).unwrap()), 1);
    assert_eq!(as_int(vm.call(f, &[i(5)]).unwrap()), 120);
    assert_eq!(as_int(vm.call(f, &[i(10)]).unwrap()), 3628800);
    print_stats("int_factorial(10)", &vm);
}

#[test]
fn int_sum_to() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = c.funcs["int_sum_to"];
    assert_eq!(as_int(vm.call(f, &[i(0)]).unwrap()), 0);
    assert_eq!(as_int(vm.call(f, &[i(1)]).unwrap()), 1);
    assert_eq!(as_int(vm.call(f, &[i(10)]).unwrap()), 55);
    assert_eq!(as_int(vm.call(f, &[i(100)]).unwrap()), 5050);
    print_stats("int_sum_to(100)", &vm);
}

#[test]
fn int_pow() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = c.funcs["int_pow"];
    assert_eq!(as_int(vm.call(f, &[i(2), i(0)]).unwrap()), 1);
    assert_eq!(as_int(vm.call(f, &[i(2), i(10)]).unwrap()), 1024);
    assert_eq!(as_int(vm.call(f, &[i(3), i(5)]).unwrap()), 243);
    print_stats("int_pow(2,10)", &vm);
}

#[test]
fn int_gcd() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = c.funcs["int_gcd"];
    assert_eq!(as_int(vm.call(f, &[i(12), i(8)]).unwrap()), 4);
    assert_eq!(as_int(vm.call(f, &[i(17), i(13)]).unwrap()), 1);
    assert_eq!(as_int(vm.call(f, &[i(100), i(75)]).unwrap()), 25);
    print_stats("int_gcd(100,75)", &vm);
}

#[test]
fn int_fib() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = c.funcs["int_fib"];
    assert_eq!(as_int(vm.call(f, &[i(0)]).unwrap()), 0);
    assert_eq!(as_int(vm.call(f, &[i(1)]).unwrap()), 1);
    assert_eq!(as_int(vm.call(f, &[i(10)]).unwrap()), 55);
    assert_eq!(as_int(vm.call(f, &[i(20)]).unwrap()), 6765);
    print_stats("int_fib(20)", &vm);
}

#[test]
fn int_arithmetic_expression() {
    let c = setup_int();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    // (5! + sum(10)) * gcd(12,8) == (120 + 55) * 4 == 700
    let fact5 = as_int(vm.call(c.funcs["int_factorial"], &[i(5)]).unwrap());
    let sum10 = as_int(vm.call(c.funcs["int_sum_to"], &[i(10)]).unwrap());
    let gcd = as_int(vm.call(c.funcs["int_gcd"], &[i(12), i(8)]).unwrap());
    assert_eq!((fact5 + sum10) * gcd, 700);
}
