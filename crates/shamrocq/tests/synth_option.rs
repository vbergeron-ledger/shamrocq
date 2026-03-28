#![cfg(feature = "integration")]

mod common;

use common::{setup, peano, unpeano, print_stats};
use shamrocq::{tags, ctors, funcs, Program, Value, Vm};

#[test]
fn option_map_some() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let negb = vm.global_value(funcs::NEGB);
    let some_true = vm.alloc_tuple(ctors::SOME, &[Value::immediate(tags::TRUE)]).unwrap();
    let result = vm.call(funcs::OPTION_MAP, &[negb, some_true]).unwrap();
    assert_eq!(result.tag(), ctors::SOME);
    assert_eq!(vm.tuple_field(result, 0).tag(), tags::FALSE);
    print_stats("option_map(negb, Some(True))", &vm);
}

#[test]
fn option_map_none() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let negb = vm.global_value(funcs::NEGB);
    let none = Value::immediate(ctors::NONE_);
    let result = vm.call(funcs::OPTION_MAP, &[negb, none]).unwrap();
    assert_eq!(result.tag(), ctors::NONE_);
    print_stats("option_map(negb, None)", &vm);
}

#[test]
fn option_bind_some() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    // option_bind(wrap_some, Some(True)) = wrap_some(True) = Some(True)
    let wrap = vm.global_value(funcs::WRAP_SOME);
    let some_true = vm.alloc_tuple(ctors::SOME, &[Value::immediate(tags::TRUE)]).unwrap();
    let result = vm.call(funcs::OPTION_BIND, &[wrap, some_true]).unwrap();
    assert_eq!(result.tag(), ctors::SOME);
    assert_eq!(vm.tuple_field(result, 0).tag(), tags::TRUE);
    print_stats("option_bind(wrap_some, Some(True))", &vm);
}

#[test]
fn option_bind_none() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let negb = vm.global_value(funcs::NEGB);
    let none = Value::immediate(ctors::NONE_);
    let result = vm.call(funcs::OPTION_BIND, &[negb, none]).unwrap();
    assert_eq!(result.tag(), ctors::NONE_);
    print_stats("option_bind(negb, None)", &vm);
}

#[test]
fn option_default_some() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n5 = peano(&mut vm, 5);
    let n0 = peano(&mut vm, 0);
    let some_5 = vm.alloc_tuple(ctors::SOME, &[n5]).unwrap();
    let result = vm.call(funcs::OPTION_DEFAULT, &[n0, some_5]).unwrap();
    assert_eq!(unpeano(&vm, result), 5);
    print_stats("option_default(0, Some(5))", &vm);
}

#[test]
fn option_default_none() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = peano(&mut vm, 0);
    let none = Value::immediate(ctors::NONE_);
    let result = vm.call(funcs::OPTION_DEFAULT, &[n0, none]).unwrap();
    assert_eq!(unpeano(&vm, result), 0);
    print_stats("option_default(0, None)", &vm);
}

#[test]
fn option_is_some_and_none() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let some = vm.alloc_tuple(ctors::SOME, &[Value::immediate(ctors::O)]).unwrap();
    let none = Value::immediate(ctors::NONE_);

    let r1 = vm.call(funcs::OPTION_IS_SOME, &[some]).unwrap();
    assert_eq!(r1.tag(), tags::TRUE);

    let r2 = vm.call(funcs::OPTION_IS_SOME, &[none]).unwrap();
    assert_eq!(r2.tag(), tags::FALSE);

    let r3 = vm.call(funcs::OPTION_IS_NONE, &[none]).unwrap();
    assert_eq!(r3.tag(), tags::TRUE);

    let r4 = vm.call(funcs::OPTION_IS_NONE, &[some]).unwrap();
    assert_eq!(r4.tag(), tags::FALSE);

    print_stats("option_is_some/none", &vm);
}
