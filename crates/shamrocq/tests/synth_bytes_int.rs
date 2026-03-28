mod common;

use common::compile_scheme;
use shamrocq::{Program, Vm};

fn setup_bytes() -> common::Compiled {
    compile_scheme(&["synth_bytes.scm"])
}

#[test]
fn str_hello_literal() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let hello = vm.global_value(c.funcs["str_hello"]);
    assert!(hello.is_bytes());
    assert_eq!(hello.bytes_len(), 5);
    assert_eq!(vm.arena.bytes_data(hello), b"hello");
}

#[test]
fn str_empty_literal() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let empty = vm.global_value(c.funcs["str_empty"]);
    assert!(empty.is_bytes());
    assert_eq!(empty.bytes_len(), 0);
    assert_eq!(vm.arena.bytes_data(empty), b"");
}

#[test]
fn str_len_basic() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let hello = vm.global_value(c.funcs["str_hello"]);
    let result = vm.call(c.funcs["str_len"], &[hello]).unwrap();
    assert_eq!(result.integer_value(), 5);
}

#[test]
fn str_first_byte() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let hello = vm.global_value(c.funcs["str_hello"]);
    let result = vm.call(c.funcs["str_first"], &[hello]).unwrap();
    assert_eq!(result.integer_value(), b'h' as i32);
}

#[test]
fn str_eq_same() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let hello = vm.global_value(c.funcs["str_hello"]);
    let hello2 = vm.arena.alloc_bytes(b"hello").unwrap();
    let result = vm.call(c.funcs["str_eq"], &[hello, hello2]).unwrap();
    assert!(result.is_ctor());
    assert_eq!(result.tag(), 0); // TRUE
}

#[test]
fn str_eq_different() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let hello = vm.global_value(c.funcs["str_hello"]);
    let world = vm.arena.alloc_bytes(b"world").unwrap();
    let result = vm.call(c.funcs["str_eq"], &[hello, world]).unwrap();
    assert!(result.is_ctor());
    assert_eq!(result.tag(), 1); // FALSE
}

#[test]
fn str_cat_basic() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let a = vm.arena.alloc_bytes(b"foo").unwrap();
    let b = vm.arena.alloc_bytes(b"bar").unwrap();
    let result = vm.call(c.funcs["str_cat"], &[a, b]).unwrap();
    assert!(result.is_bytes());
    assert_eq!(vm.arena.bytes_data(result), b"foobar");
}

#[test]
fn str_starts_with_h_true() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let hello = vm.global_value(c.funcs["str_hello"]);
    let result = vm.call(c.funcs["str_starts_with_h"], &[hello]).unwrap();
    assert!(result.is_ctor());
    assert_eq!(result.tag(), 0); // TRUE
}

#[test]
fn str_starts_with_h_false() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let world = vm.arena.alloc_bytes(b"world").unwrap();
    let result = vm.call(c.funcs["str_starts_with_h"], &[world]).unwrap();
    assert!(result.is_ctor());
    assert_eq!(result.tag(), 1); // FALSE
}

#[test]
fn str_starts_with_h_empty() {
    let c = setup_bytes();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let empty = vm.global_value(c.funcs["str_empty"]);
    let result = vm.call(c.funcs["str_starts_with_h"], &[empty]).unwrap();
    assert!(result.is_ctor());
    assert_eq!(result.tag(), 1); // FALSE
}
