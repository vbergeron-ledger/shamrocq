#![cfg(feature = "integration")]

mod common;

use common::{setup, peano, unpeano, list_to_vec, print_stats};
use shamrocq::{ctors, funcs, Program, Value, Vm};

fn make_assoc(vm: &mut Vm, pairs: &[(u32, u32)]) -> Value {
    let mut list = Value::immediate(ctors::NIL);
    for &(k, v) in pairs.iter().rev() {
        let key = peano(vm, k);
        let val = peano(vm, v);
        let pair = vm.alloc_tuple(ctors::PAIR, &[key, val]).unwrap();
        list = vm.alloc_tuple(ctors::CONS, &[pair, list]).unwrap();
    }
    list
}

#[test]
fn assoc_get_found() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let alist = make_assoc(&mut vm, &[(1, 10), (2, 20), (3, 30)]);
    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);

    let result = vm.call(funcs::ASSOC_GET, &[ord, key, alist]).unwrap();
    assert_eq!(result.tag(), ctors::SOME);
    assert_eq!(unpeano(&vm, vm.tuple_field(result, 0)), 20);
    print_stats("assoc_get(2, [(1,10),(2,20),(3,30)])", &vm);
}

#[test]
fn assoc_get_not_found() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let alist = make_assoc(&mut vm, &[(1, 10), (3, 30)]);
    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);

    let result = vm.call(funcs::ASSOC_GET, &[ord, key, alist]).unwrap();
    assert_eq!(result.tag(), ctors::NONE_);
    print_stats("assoc_get(2, [(1,10),(3,30)])", &vm);
}

#[test]
fn assoc_set_new_key() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let alist = make_assoc(&mut vm, &[(1, 10)]);
    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);
    let val = peano(&mut vm, 20);

    let updated = vm.call(funcs::ASSOC_SET, &[ord, key, val, alist]).unwrap();
    let v = list_to_vec(&vm, updated);
    assert_eq!(v.len(), 2);

    // Verify we can get the new key
    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);
    let result = vm.call(funcs::ASSOC_GET, &[ord, key, updated]).unwrap();
    assert_eq!(result.tag(), ctors::SOME);
    assert_eq!(unpeano(&vm, vm.tuple_field(result, 0)), 20);
    print_stats("assoc_set(2, 20, [(1,10)])", &vm);
}

#[test]
fn assoc_set_overwrite() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let alist = make_assoc(&mut vm, &[(1, 10), (2, 20)]);
    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);
    let val = peano(&mut vm, 99);

    let updated = vm.call(funcs::ASSOC_SET, &[ord, key, val, alist]).unwrap();
    let v = list_to_vec(&vm, updated);
    assert_eq!(v.len(), 2, "overwrite should not increase length");

    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);
    let result = vm.call(funcs::ASSOC_GET, &[ord, key, updated]).unwrap();
    assert_eq!(result.tag(), ctors::SOME);
    assert_eq!(unpeano(&vm, vm.tuple_field(result, 0)), 99);
    print_stats("assoc_set(2, 99, [(1,10),(2,20)])", &vm);
}

#[test]
fn assoc_remove_existing() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let alist = make_assoc(&mut vm, &[(1, 10), (2, 20), (3, 30)]);
    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);

    let removed = vm.call(funcs::ASSOC_REMOVE, &[ord, key, alist]).unwrap();
    let v = list_to_vec(&vm, removed);
    assert_eq!(v.len(), 2);

    let ord = vm.global_value(funcs::NAT_ORD);
    let key = peano(&mut vm, 2);
    let result = vm.call(funcs::ASSOC_GET, &[ord, key, removed]).unwrap();
    assert_eq!(result.tag(), ctors::NONE_);
    print_stats("assoc_remove(2, [(1,10),(2,20),(3,30)])", &vm);
}

#[test]
fn assoc_keys_and_values() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let alist = make_assoc(&mut vm, &[(1, 10), (2, 20), (3, 30)]);

    let keys = vm.call(funcs::ASSOC_KEYS, &[alist]).unwrap();
    let kv = list_to_vec(&vm, keys);
    assert_eq!(kv.len(), 3);
    let key_nums: Vec<u32> = kv.iter().map(|x| unpeano(&vm, *x)).collect();
    assert_eq!(key_nums, vec![1, 2, 3]);

    let values = vm.call(funcs::ASSOC_VALUES, &[alist]).unwrap();
    let vv = list_to_vec(&vm, values);
    assert_eq!(vv.len(), 3);
    let val_nums: Vec<u32> = vv.iter().map(|x| unpeano(&vm, *x)).collect();
    assert_eq!(val_nums, vec![10, 20, 30]);

    print_stats("assoc_keys_and_values", &vm);
}
