#![cfg(feature = "integration")]

mod common;

use common::{setup, peano, unpeano, list_to_vec, print_stats};
use shamrocq::{tags, ctors, funcs, Program, Value, Vm};

fn make_leaf(vm: &mut Vm) -> Value {
    let nil = Value::immediate(ctors::NIL);
    vm.alloc_tuple(ctors::LEAF, &[nil]).unwrap()
}

#[test]
fn tree_insert_and_member() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let ord = vm.global_value(funcs::NAT_ORD);
    let leaf = make_leaf(&mut vm);

    let n3 = peano(&mut vm, 3);
    let n1 = peano(&mut vm, 1);
    let n5 = peano(&mut vm, 5);
    let n7 = peano(&mut vm, 7);

    // Insert 3, 1, 5
    let t = vm.call(funcs::TREE_INSERT, &[ord, n3, leaf]).unwrap();
    assert_eq!(t.tag(), ctors::NODE);

    let ord = vm.global_value(funcs::NAT_ORD);
    let t = vm.call(funcs::TREE_INSERT, &[ord, n1, t]).unwrap();
    let ord = vm.global_value(funcs::NAT_ORD);
    let t = vm.call(funcs::TREE_INSERT, &[ord, n5, t]).unwrap();

    // member(3) = True
    let ord = vm.global_value(funcs::NAT_ORD);
    let r = vm.call(funcs::TREE_MEMBER, &[ord, n3, t]).unwrap();
    assert_eq!(r.tag(), tags::TRUE, "3 should be in tree");

    // member(1) = True
    let ord = vm.global_value(funcs::NAT_ORD);
    let r = vm.call(funcs::TREE_MEMBER, &[ord, n1, t]).unwrap();
    assert_eq!(r.tag(), tags::TRUE, "1 should be in tree");

    // member(5) = True
    let ord = vm.global_value(funcs::NAT_ORD);
    let r = vm.call(funcs::TREE_MEMBER, &[ord, n5, t]).unwrap();
    assert_eq!(r.tag(), tags::TRUE, "5 should be in tree");

    // member(7) = False
    let ord = vm.global_value(funcs::NAT_ORD);
    let r = vm.call(funcs::TREE_MEMBER, &[ord, n7, t]).unwrap();
    assert_eq!(r.tag(), tags::FALSE, "7 should not be in tree");

    print_stats("tree_insert_and_member", &vm);
}

#[test]
fn tree_insert_duplicate() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let ord = vm.global_value(funcs::NAT_ORD);
    let leaf = make_leaf(&mut vm);
    let n3 = peano(&mut vm, 3);

    let t = vm.call(funcs::TREE_INSERT, &[ord, n3, leaf]).unwrap();
    let size1 = vm.call(funcs::TREE_SIZE, &[t]).unwrap();
    assert_eq!(unpeano(&vm, size1), 1);

    // Insert duplicate
    let ord = vm.global_value(funcs::NAT_ORD);
    let n3b = peano(&mut vm, 3);
    let t2 = vm.call(funcs::TREE_INSERT, &[ord, n3b, t]).unwrap();
    let size2 = vm.call(funcs::TREE_SIZE, &[t2]).unwrap();
    assert_eq!(unpeano(&vm, size2), 1, "duplicate insert should not increase size");

    print_stats("tree_insert_duplicate", &vm);
}

#[test]
fn tree_size_and_height() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let leaf = make_leaf(&mut vm);

    // Insert 4, 2, 6, 1, 3 (balanced-ish)
    let vals = [4, 2, 6, 1, 3];
    let mut t = leaf;
    for &v in &vals {
        let ord = vm.global_value(funcs::NAT_ORD);
        let n = peano(&mut vm, v);
        t = vm.call(funcs::TREE_INSERT, &[ord, n, t]).unwrap();
    }

    let size = vm.call(funcs::TREE_SIZE, &[t]).unwrap();
    assert_eq!(unpeano(&vm, size), 5);

    let height = vm.call(funcs::TREE_HEIGHT, &[t]).unwrap();
    let h = unpeano(&vm, height);
    assert!(h >= 3 && h <= 5, "height should be 3-5, got {}", h);

    print_stats("tree_size_and_height", &vm);
}

#[test]
fn tree_to_list_sorted() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let leaf = make_leaf(&mut vm);

    // Insert 5, 2, 8, 1, 3 — in-order traversal should give sorted [1,2,3,5,8]
    let vals = [5, 2, 8, 1, 3];
    let mut t = leaf;
    for &v in &vals {
        let ord = vm.global_value(funcs::NAT_ORD);
        let n = peano(&mut vm, v);
        t = vm.call(funcs::TREE_INSERT, &[ord, n, t]).unwrap();
    }

    let sorted = vm.call(funcs::TREE_TO_LIST, &[t]).unwrap();
    let v = list_to_vec(&vm, sorted);
    assert_eq!(v.len(), 5);

    let nums: Vec<u32> = v.iter().map(|x| unpeano(&vm, *x)).collect();
    assert_eq!(nums, vec![1, 2, 3, 5, 8], "in-order should be sorted");

    print_stats("tree_to_list_sorted", &vm);
}
