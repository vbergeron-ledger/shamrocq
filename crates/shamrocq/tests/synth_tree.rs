mod common;

use common::{compile_scheme, list_to_vec, peano, unpeano, print_stats, Compiled};
use shamrocq::{Program, Value, Vm};

fn setup() -> Compiled {
    compile_scheme(&[
        "test_helpers.scm",
        "synth_list.scm",
        "synth_arith.scm",
        "synth_tree.scm",
    ])
}

fn make_leaf(vm: &mut Vm, c: &Compiled) -> Value {
    let nil = Value::nullary_ctor(c.tag("Nil"));
    vm.alloc_ctor(c.tag("Leaf"), &[nil]).unwrap()
}

#[test]
fn tree_insert_and_member() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let ord = vm.global_value(c.func("nat_ord"));
    let leaf = make_leaf(&mut vm, &c);

    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n5 = peano(&mut vm, c.tag("O"), c.tag("S"), 5);
    let n7 = peano(&mut vm, c.tag("O"), c.tag("S"), 7);

    // Insert 3, 1, 5
    let t = vm.call(c.func("tree_insert"), &[ord, n3, leaf]).unwrap();
    assert_eq!(t.tag(), c.tag("Node"));

    let ord = vm.global_value(c.func("nat_ord"));
    let t = vm.call(c.func("tree_insert"), &[ord, n1, t]).unwrap();
    let ord = vm.global_value(c.func("nat_ord"));
    let t = vm.call(c.func("tree_insert"), &[ord, n5, t]).unwrap();

    // member(3) = True
    let ord = vm.global_value(c.func("nat_ord"));
    let r = vm.call(c.func("tree_member"), &[ord, n3, t]).unwrap();
    assert_eq!(r.tag(), c.tag("True"), "3 should be in tree");

    // member(1) = True
    let ord = vm.global_value(c.func("nat_ord"));
    let r = vm.call(c.func("tree_member"), &[ord, n1, t]).unwrap();
    assert_eq!(r.tag(), c.tag("True"), "1 should be in tree");

    // member(5) = True
    let ord = vm.global_value(c.func("nat_ord"));
    let r = vm.call(c.func("tree_member"), &[ord, n5, t]).unwrap();
    assert_eq!(r.tag(), c.tag("True"), "5 should be in tree");

    // member(7) = False
    let ord = vm.global_value(c.func("nat_ord"));
    let r = vm.call(c.func("tree_member"), &[ord, n7, t]).unwrap();
    assert_eq!(r.tag(), c.tag("False"), "7 should not be in tree");

    print_stats("tree_insert_and_member", &vm);
}

#[test]
fn tree_insert_duplicate() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let ord = vm.global_value(c.func("nat_ord"));
    let leaf = make_leaf(&mut vm, &c);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);

    let t = vm.call(c.func("tree_insert"), &[ord, n3, leaf]).unwrap();
    let size1 = vm.call(c.func("tree_size"), &[t]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), size1), 1);

    // Insert duplicate
    let ord = vm.global_value(c.func("nat_ord"));
    let n3b = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let t2 = vm.call(c.func("tree_insert"), &[ord, n3b, t]).unwrap();
    let size2 = vm.call(c.func("tree_size"), &[t2]).unwrap();
    assert_eq!(
        unpeano(&vm, c.tag("S"), size2),
        1,
        "duplicate insert should not increase size"
    );

    print_stats("tree_insert_duplicate", &vm);
}

#[test]
fn tree_size_and_height() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let leaf = make_leaf(&mut vm, &c);

    // Insert 4, 2, 6, 1, 3 (balanced-ish)
    let vals = [4, 2, 6, 1, 3];
    let mut t = leaf;
    for &v in &vals {
        let ord = vm.global_value(c.func("nat_ord"));
        let n = peano(&mut vm, c.tag("O"), c.tag("S"), v);
        t = vm.call(c.func("tree_insert"), &[ord, n, t]).unwrap();
    }

    let size = vm.call(c.func("tree_size"), &[t]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), size), 5);

    let height = vm.call(c.func("tree_height"), &[t]).unwrap();
    let h = unpeano(&vm, c.tag("S"), height);
    assert!(h >= 3 && h <= 5, "height should be 3-5, got {}", h);

    print_stats("tree_size_and_height", &vm);
}

#[test]
fn tree_to_list_sorted() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let leaf = make_leaf(&mut vm, &c);

    // Insert 5, 2, 8, 1, 3 — in-order traversal should give sorted [1,2,3,5,8]
    let vals = [5, 2, 8, 1, 3];
    let mut t = leaf;
    for &v in &vals {
        let ord = vm.global_value(c.func("nat_ord"));
        let n = peano(&mut vm, c.tag("O"), c.tag("S"), v);
        t = vm.call(c.func("tree_insert"), &[ord, n, t]).unwrap();
    }

    let sorted = vm.call(c.func("tree_to_list"), &[t]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), sorted);
    assert_eq!(v.len(), 5);

    let nums: Vec<u32> = v.iter().map(|x| unpeano(&vm, c.tag("S"), *x)).collect();
    assert_eq!(nums, vec![1, 2, 3, 5, 8], "in-order should be sorted");

    print_stats("tree_to_list_sorted", &vm);
}
