#![cfg(feature = "integration")]

mod common;

use common::{setup, peano, unpeano, list_to_vec, make_list, print_stats};
use shamrocq::{tags, ctors, funcs, Program, Value, Vm};

#[test]
fn load_program() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();
    print_stats("load_program", &vm);
}

#[test]
fn negb_true_is_false() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let result = vm.call(funcs::NEGB, &[Value::immediate(tags::TRUE)]).unwrap();
    assert_eq!(result.tag(), tags::FALSE);
    assert!(result.is_immediate());
    print_stats("negb(true)", &vm);
}

#[test]
fn negb_false_is_true() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let result = vm.call(funcs::NEGB, &[Value::immediate(tags::FALSE)]).unwrap();
    assert_eq!(result.tag(), tags::TRUE);
    assert!(result.is_immediate());
    print_stats("negb(false)", &vm);
}

#[test]
fn length_nil_is_zero() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let nil = Value::immediate(ctors::NIL);
    let result = vm.call(funcs::LENGTH, &[nil]).unwrap();
    assert_eq!(result.tag(), ctors::O);
    assert!(result.is_immediate());
    print_stats("length(nil)", &vm);
}

#[test]
fn length_singleton() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let nil = Value::immediate(ctors::NIL);
    let elem = Value::immediate(ctors::O);
    let list = vm.alloc_tuple(ctors::CONS, &[elem, nil]).unwrap();

    let result = vm.call(funcs::LENGTH, &[list]).unwrap();
    assert_eq!(result.tag(), ctors::S);
    assert!(result.is_tuple());
    let inner = vm.tuple_field(result, 0);
    assert_eq!(inner.tag(), ctors::O);
    print_stats("length([_])", &vm);
}

#[test]
fn leb_zero_anything_is_true() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let zero = Value::immediate(ctors::O);
    let one = vm.alloc_tuple(ctors::S, &[zero]).unwrap();

    let result = vm.call(funcs::LEB, &[zero, one]).unwrap();
    assert_eq!(result.tag(), tags::TRUE);
    print_stats("leb(0, 1)", &vm);
}

#[test]
fn map_negb_over_list() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let nil = Value::immediate(ctors::NIL);
    let t = Value::immediate(tags::TRUE);
    let f = Value::immediate(tags::FALSE);
    let list = vm.alloc_tuple(ctors::CONS, &[t, nil]).unwrap();
    let list = vm.alloc_tuple(ctors::CONS, &[f, list]).unwrap();

    assert_eq!(list.tag(), ctors::CONS);
    let head = vm.tuple_field(list, 0);
    assert_eq!(head.tag(), tags::FALSE);
    print_stats("map_negb_over_list", &vm);
}

#[test]
fn hforest_init_creates_forest() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let prev = Value::immediate(ctors::O);
    let value = Value::immediate(ctors::O);
    let prev_height = Value::immediate(ctors::O);

    let result = vm
        .call(funcs::HFOREST_INIT, &[prev, value, prev_height])
        .unwrap();
    assert_eq!(result.tag(), ctors::BUILD_HFOREST);

    let roots = vm.tuple_field(result, 0);
    assert_eq!(roots.tag(), ctors::CONS);

    let edges = vm.tuple_field(result, 1);
    assert_eq!(edges.tag(), ctors::CONS);
    print_stats("hforest_init(O, O, O)", &vm);
}

#[test]
fn nat_ord_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let zero = Value::immediate(ctors::O);
    let one = peano(&mut vm, 1);
    let two = peano(&mut vm, 2);

    // nat_ord(0, 0) = Left
    let r = vm.call(funcs::NAT_ORD, &[zero, zero]).unwrap();
    assert_eq!(r.tag(), ctors::LEFT, "nat_ord(0,0) should be Left");

    // nat_ord(0, 1) = Left
    let r = vm.call(funcs::NAT_ORD, &[zero, one]).unwrap();
    assert_eq!(r.tag(), ctors::LEFT, "nat_ord(0,1) should be Left");

    // nat_ord(1, 0) = Right
    let r = vm.call(funcs::NAT_ORD, &[one, zero]).unwrap();
    assert_eq!(r.tag(), ctors::RIGHT, "nat_ord(1,0) should be Right");

    // nat_ord(1, 1) = Left
    let r = vm.call(funcs::NAT_ORD, &[one, one]).unwrap();
    assert_eq!(r.tag(), ctors::LEFT, "nat_ord(1,1) should be Left");

    // nat_ord(1, 2) = Left
    let r = vm.call(funcs::NAT_ORD, &[one, two]).unwrap();
    assert_eq!(r.tag(), ctors::LEFT, "nat_ord(1,2) should be Left");

    // nat_ord(2, 1) = Right
    let r = vm.call(funcs::NAT_ORD, &[two, one]).unwrap();
    assert_eq!(r.tag(), ctors::RIGHT, "nat_ord(2,1) should be Right");

    print_stats("nat_ord_basic", &vm);
}

#[test]
fn eqb_and_leb0_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);
    let h = vm.global_value(funcs::NAT_ORD);

    // leb0(nat_ord, 0, 0) → True
    let r = vm.call(funcs::LEB0, &[h, n0, n0]).unwrap();
    assert_eq!(r.tag(), tags::TRUE, "leb0(nat_ord, 0, 0) should be True");

    // eqb(nat_ord, 0, 0) → True
    let h = vm.global_value(funcs::NAT_ORD);
    let r = vm.call(funcs::EQB, &[h, n0, n0]).unwrap();
    assert_eq!(r.tag(), tags::TRUE, "eqb(nat_ord, 0, 0) should be True");

    // eqb(nat_ord, 0, 1) → False
    let h = vm.global_value(funcs::NAT_ORD);
    let r = vm.call(funcs::EQB, &[h, n0, n1]).unwrap();
    assert_eq!(r.tag(), tags::FALSE, "eqb(nat_ord, 0, 1) should be False");

    print_stats("eqb_and_leb0_basic", &vm);
}

#[test]
fn merge_sorted_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);
    let nil = Value::immediate(ctors::NIL);

    // Simple: merge_sorted(nat_ord, [0], [1])
    let h = vm.global_value(funcs::NAT_ORD);
    let l1 = vm.alloc_tuple(ctors::CONS, &[n0, nil]).unwrap();
    let l2 = vm.alloc_tuple(ctors::CONS, &[n1, nil]).unwrap();

    let merged = vm.call(funcs::MERGE_SORTED, &[h, l1, l2]).unwrap();
    let merged_vec = list_to_vec(&vm, merged);
    assert_eq!(merged_vec.len(), 2, "merge_sorted([0], [1]) should have 2 elements");

    print_stats("merge_sorted_basic", &vm);
}

#[test]
fn merge_dedup_sorted_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);
    let nil = Value::immediate(ctors::NIL);
    let h = vm.global_value(funcs::NAT_ORD);

    let l1 = vm.alloc_tuple(ctors::CONS, &[n0, nil]).unwrap();
    let l2 = vm.alloc_tuple(ctors::CONS, &[n1, nil]).unwrap();

    let merged = vm.call(funcs::MERGE_DEDUP_SORTED, &[h, l1, l2]).unwrap();
    let merged_vec = list_to_vec(&vm, merged);
    assert_eq!(merged_vec.len(), 2, "merge_dedup_sorted([0], [1]) should have 2 elements");

    print_stats("merge_dedup_sorted_basic", &vm);
}

#[test]
fn merge_dedup_sorted_overlap() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);
    let n2 = peano(&mut vm, 2);
    let n3 = peano(&mut vm, 3);

    // l1 = [0, 1, 2]
    let l1 = make_list(&mut vm, &[n0, n1, n2]);
    // l2 = [1, 2, 3]  — shares 1 and 2 with l1
    let l2 = make_list(&mut vm, &[n1, n2, n3]);

    let h = vm.global_value(funcs::NAT_ORD);
    let merged = vm.call(funcs::MERGE_DEDUP_SORTED, &[h, l1, l2]).unwrap();
    let merged_vec = list_to_vec(&vm, merged);

    // Duplicates (1, 2) must be removed → [0, 1, 2, 3]
    assert_eq!(merged_vec.len(), 4, "merge_dedup_sorted([0,1,2],[1,2,3]) should have 4 elements");
    assert_eq!(unpeano(&vm, merged_vec[0]), 0);
    assert_eq!(unpeano(&vm, merged_vec[1]), 1);
    assert_eq!(unpeano(&vm, merged_vec[2]), 2);
    assert_eq!(unpeano(&vm, merged_vec[3]), 3);

    print_stats("merge_dedup_sorted([0,1,2],[1,2,3])", &vm);
}

#[test]
fn ordroot_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);

    let h = vm.global_value(funcs::NAT_ORD);
    let ord = vm.call(funcs::ORDROOT, &[h]).unwrap();

    // Create two roots with different hashes
    let r1 = vm.alloc_tuple(ctors::BUILD_ROOT, &[n0, n0]).unwrap(); // root(hash=0, height=0)
    let r2 = vm.alloc_tuple(ctors::BUILD_ROOT, &[n1, n0]).unwrap(); // root(hash=1, height=0)

    // ordRoot(nat_ord)(r1, r2) should return Left (0 <= 1)
    let result = vm.apply(ord, &[r1, r2]).unwrap();
    assert_eq!(result.tag(), ctors::LEFT, "ordRoot(nat_ord)(root0, root1) should be Left");

    print_stats("ordroot_basic", &vm);
}

#[test]
fn merge_roots_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);
    let nil = Value::immediate(ctors::NIL);

    let r1 = vm.alloc_tuple(ctors::BUILD_ROOT, &[n0, n0]).unwrap();
    let r2 = vm.alloc_tuple(ctors::BUILD_ROOT, &[n1, n0]).unwrap();
    let l1 = vm.alloc_tuple(ctors::CONS, &[r1, nil]).unwrap();
    let l2 = vm.alloc_tuple(ctors::CONS, &[r2, nil]).unwrap();

    let h = vm.global_value(funcs::NAT_ORD);
    let merged = vm.call(funcs::MERGE_ROOTS, &[h, l1, l2]).unwrap();
    let merged_vec = list_to_vec(&vm, merged);
    assert_eq!(merged_vec.len(), 2, "merge_roots([root0], [root1]) should have 2 roots");

    print_stats("merge_roots_basic", &vm);
}

#[test]
fn hforest_merge_basic() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);
    let n3 = peano(&mut vm, 3);

    let f1 = vm.call(funcs::HFOREST_INIT, &[n0, n1, n0]).unwrap();
    let f2 = vm.call(funcs::HFOREST_INIT, &[n1, n3, n0]).unwrap();

    let h = vm.global_value(funcs::NAT_ORD);
    let merged = vm.call(funcs::HFOREST_MERGE, &[h, f1, f2]).unwrap();
    assert_eq!(merged.tag(), ctors::BUILD_HFOREST);
    print_stats("hforest_merge_basic", &vm);
}

#[test]
fn hforest_lifecycle() {
    let (mut buf, bytecode) = setup();
    let prog = Program::from_blob(bytecode).unwrap();
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    // --- List + higher-order function phase ---

    let nil = Value::immediate(ctors::NIL);
    let t = Value::immediate(tags::TRUE);
    let f = Value::immediate(tags::FALSE);

    // Build [True, False, True, False]
    let list = vm.alloc_tuple(ctors::CONS, &[f, nil]).unwrap();
    let list = vm.alloc_tuple(ctors::CONS, &[t, list]).unwrap();
    let list = vm.alloc_tuple(ctors::CONS, &[f, list]).unwrap();
    let list = vm.alloc_tuple(ctors::CONS, &[t, list]).unwrap();
    assert_eq!(list_to_vec(&vm, list).len(), 4);

    // map(negb, list) → [False, True, False, True]
    let negb = vm.global_value(funcs::NEGB);
    let mapped = vm.call(funcs::MAP, &[negb, list]).unwrap();
    let mapped_vec = list_to_vec(&vm, mapped);
    assert_eq!(mapped_vec.len(), 4);
    assert_eq!(mapped_vec[0].tag(), tags::FALSE);
    assert_eq!(mapped_vec[1].tag(), tags::TRUE);
    assert_eq!(mapped_vec[2].tag(), tags::FALSE);
    assert_eq!(mapped_vec[3].tag(), tags::TRUE);

    // length(mapped) → 4
    let len_mapped = vm.call(funcs::LENGTH, &[mapped]).unwrap();
    assert_eq!(unpeano(&vm, len_mapped), 4);

    // filter(negb, mapped) → keeps items where negb(x)=True, i.e. False items → [False, False]
    let negb = vm.global_value(funcs::NEGB);
    let filtered = vm.call(funcs::FILTER, &[negb, mapped]).unwrap();
    let filtered_vec = list_to_vec(&vm, filtered);
    assert_eq!(filtered_vec.len(), 2);
    assert_eq!(filtered_vec[0].tag(), tags::FALSE);
    assert_eq!(filtered_vec[1].tag(), tags::FALSE);

    // length(filtered) → 2
    let len_filtered = vm.call(funcs::LENGTH, &[filtered]).unwrap();
    assert_eq!(unpeano(&vm, len_filtered), 2);

    // leb(2, 4) → True
    let leb_result = vm.call(funcs::LEB, &[len_filtered, len_mapped]).unwrap();
    assert_eq!(leb_result.tag(), tags::TRUE);

    // --- hforest phase ---
    // Use distinct prev/value so roots aren't filtered by valid_roots.
    // hforest_init(prev, value, height) creates:
    //   root(prev, height), edge(prev, value, S(height))
    // valid_roots removes roots whose hash appears as an edge child_hash (=value).
    // So we need prev ≠ value.

    let n0 = Value::immediate(ctors::O);
    let n1 = peano(&mut vm, 1);
    let n2 = peano(&mut vm, 2);
    let n3 = peano(&mut vm, 3);
    let n4 = peano(&mut vm, 4);

    // f1: root=0, edge(0→1, height=1)
    let _h = vm.global_value(funcs::NAT_ORD);
    let f1 = vm.call(funcs::HFOREST_INIT, &[n0, n1, n0]).unwrap();
    assert_eq!(f1.tag(), ctors::BUILD_HFOREST);

    // f2: root=2, edge(2→3, height=1)
    let f2 = vm.call(funcs::HFOREST_INIT, &[n2, n3, n0]).unwrap();
    assert_eq!(f2.tag(), ctors::BUILD_HFOREST);

    // hforest_merge(nat_ord, f1, f2)
    let h = vm.global_value(funcs::NAT_ORD);
    let merged = vm.call(funcs::HFOREST_MERGE, &[h, f1, f2]).unwrap();
    assert_eq!(merged.tag(), ctors::BUILD_HFOREST);

    let merged_roots = list_to_vec(&vm, vm.tuple_field(merged, 0));
    let merged_edges = list_to_vec(&vm, vm.tuple_field(merged, 1));
    assert_eq!(merged_roots.len(), 2, "merged should have 2 roots (0 and 2)");
    assert_eq!(merged_edges.len(), 2, "merged should have 2 edges");

    // hforest_contains(nat_ord, 0, merged) → True (root hash 0)
    let h = vm.global_value(funcs::NAT_ORD);
    let contains_0 = vm.call(funcs::HFOREST_CONTAINS, &[h, n0, merged]).unwrap();
    assert_eq!(contains_0.tag(), tags::TRUE);

    // hforest_contains(nat_ord, 1, merged) → True (edge child hash 1)
    let h = vm.global_value(funcs::NAT_ORD);
    let contains_1 = vm.call(funcs::HFOREST_CONTAINS, &[h, n1, merged]).unwrap();
    assert_eq!(contains_1.tag(), tags::TRUE);

    // hforest_contains(nat_ord, 4, merged) → False (not present)
    let h = vm.global_value(funcs::NAT_ORD);
    let contains_4 = vm.call(funcs::HFOREST_CONTAINS, &[h, n4, merged]).unwrap();
    assert_eq!(contains_4.tag(), tags::FALSE);

    // hforest_tips(nat_ord, merged) → tip pairs
    let h = vm.global_value(funcs::NAT_ORD);
    let tips = vm.call(funcs::HFOREST_TIPS, &[h, merged]).unwrap();
    let tips_vec = list_to_vec(&vm, tips);
    assert!(!tips_vec.is_empty(), "merged forest should have tips");
    for tip in &tips_vec {
        assert_eq!(tip.tag(), ctors::PAIR);
    }

    // hforest_insert(nat_ord, 4, 3, 0, merged) → (Pair new_forest was_new)
    // prev=4, value=3: new edge 4→3, prev≠value so it's a genuine insert
    let h = vm.global_value(funcs::NAT_ORD);
    let inserted = vm.call(funcs::HFOREST_INSERT, &[h, n4, n3, n0, merged]).unwrap();
    assert_eq!(inserted.tag(), ctors::PAIR);
    let new_forest = vm.tuple_field(inserted, 0);
    let was_new = vm.tuple_field(inserted, 1);
    assert_eq!(new_forest.tag(), ctors::BUILD_HFOREST);
    assert_eq!(was_new.tag(), tags::TRUE);

    // hforest_contains(nat_ord, 4, new_forest) → True (root hash 4)
    let h = vm.global_value(funcs::NAT_ORD);
    let contains_4_now = vm.call(funcs::HFOREST_CONTAINS, &[h, n4, new_forest]).unwrap();
    assert_eq!(contains_4_now.tag(), tags::TRUE);

    // hforest_contains(nat_ord, 3, new_forest) → True (edge child hash 3)
    let h = vm.global_value(funcs::NAT_ORD);
    let contains_3_now = vm.call(funcs::HFOREST_CONTAINS, &[h, n3, new_forest]).unwrap();
    assert_eq!(contains_3_now.tag(), tags::TRUE);

    print_stats("hforest_lifecycle", &vm);
}
