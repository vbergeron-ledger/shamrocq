#![cfg(feature = "integration")]

mod common;

use common::{setup, peano, unpeano, list_to_vec, make_list, print_stats};
use shamrocq::{ctors, funcs, Program, Value, Vm};

#[test]
fn append_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = peano(&mut vm, 0);
    let n1 = peano(&mut vm, 1);
    let n2 = peano(&mut vm, 2);
    let n3 = peano(&mut vm, 3);
    let l1 = make_list(&mut vm, &[n0, n1]);
    let l2 = make_list(&mut vm, &[n2, n3]);

    let result = vm.call(funcs::APPEND, &[l1, l2]).unwrap();
    let v = list_to_vec(&vm, result);
    assert_eq!(v.len(), 4);
    assert_eq!(unpeano(&vm, v[0]), 0);
    assert_eq!(unpeano(&vm, v[1]), 1);
    assert_eq!(unpeano(&vm, v[2]), 2);
    assert_eq!(unpeano(&vm, v[3]), 3);
    print_stats("append([0,1],[2,3])", &vm);
}

#[test]
fn append_empty() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let nil = Value::immediate(ctors::NIL);
    let n1 = peano(&mut vm, 1);
    let l = make_list(&mut vm, &[n1]);

    let result = vm.call(funcs::APPEND, &[nil, l]).unwrap();
    let v = list_to_vec(&vm, result);
    assert_eq!(v.len(), 1);
    print_stats("append([],[1])", &vm);
}

#[test]
fn reverse_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n1 = peano(&mut vm, 1);
    let n2 = peano(&mut vm, 2);
    let n3 = peano(&mut vm, 3);
    let l = make_list(&mut vm, &[n1, n2, n3]);

    let result = vm.call(funcs::REVERSE, &[l]).unwrap();
    let v = list_to_vec(&vm, result);
    assert_eq!(v.len(), 3);
    assert_eq!(unpeano(&vm, v[0]), 3);
    assert_eq!(unpeano(&vm, v[1]), 2);
    assert_eq!(unpeano(&vm, v[2]), 1);
    print_stats("reverse([1,2,3])", &vm);
}

#[test]
fn nth_found() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = peano(&mut vm, 0);
    let n1 = peano(&mut vm, 1);
    let n2 = peano(&mut vm, 2);
    let l = make_list(&mut vm, &[n0, n1, n2]);

    let idx = peano(&mut vm, 1);
    let result = vm.call(funcs::NTH, &[idx, l]).unwrap();
    assert_eq!(result.tag(), ctors::SOME);
    assert_eq!(unpeano(&vm, vm.tuple_field(result, 0)), 1);
    print_stats("nth(1,[0,1,2])", &vm);
}

#[test]
fn nth_out_of_bounds() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = peano(&mut vm, 0);
    let l = make_list(&mut vm, &[n0]);

    let idx = peano(&mut vm, 5);
    let result = vm.call(funcs::NTH, &[idx, l]).unwrap();
    assert_eq!(result.tag(), ctors::NONE_);
    print_stats("nth(5,[0])", &vm);
}

#[test]
fn zip_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n1 = peano(&mut vm, 1);
    let n2 = peano(&mut vm, 2);
    let n3 = peano(&mut vm, 3);
    let n4 = peano(&mut vm, 4);
    let l1 = make_list(&mut vm, &[n1, n2]);
    let l2 = make_list(&mut vm, &[n3, n4]);

    let result = vm.call(funcs::ZIP, &[l1, l2]).unwrap();
    let v = list_to_vec(&vm, result);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].tag(), ctors::PAIR);
    assert_eq!(unpeano(&vm, vm.tuple_field(v[0], 0)), 1);
    assert_eq!(unpeano(&vm, vm.tuple_field(v[0], 1)), 3);
    assert_eq!(unpeano(&vm, vm.tuple_field(v[1], 0)), 2);
    assert_eq!(unpeano(&vm, vm.tuple_field(v[1], 1)), 4);
    print_stats("zip([1,2],[3,4])", &vm);
}

#[test]
fn zip_uneven() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n1 = peano(&mut vm, 1);
    let n2 = peano(&mut vm, 2);
    let n3 = peano(&mut vm, 3);
    let l1 = make_list(&mut vm, &[n1, n2, n3]);
    let l2 = make_list(&mut vm, &[n1]);

    let result = vm.call(funcs::ZIP, &[l1, l2]).unwrap();
    let v = list_to_vec(&vm, result);
    assert_eq!(v.len(), 1);
    print_stats("zip([1,2,3],[1])", &vm);
}
