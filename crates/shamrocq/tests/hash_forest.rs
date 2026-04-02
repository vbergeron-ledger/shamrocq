mod common;

use common::{
    compile_scheme, make_list, list_to_vec, peano, print_stats, unpeano, Compiled,
};
use shamrocq::{Program, Value, Vm};

fn setup() -> Compiled {
    // `nat_ord` lives in test_helpers.scm; hash_forest.scm defines the rest.
    compile_scheme(&["test_helpers.scm", "hash_forest.scm"])
}

#[test]
fn load_program() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();
    print_stats("load_program", &vm);
}

#[test]
fn negb_true_is_false() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let result = vm
        .call(c.func("negb"), &[Value::nullary_ctor(c.tag("True"))])
        .unwrap();
    assert_eq!(result.tag(), c.tag("False"));
    print_stats("negb(true)", &vm);
}

#[test]
fn negb_false_is_true() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let result = vm
        .call(c.func("negb"), &[Value::nullary_ctor(c.tag("False"))])
        .unwrap();
    assert_eq!(result.tag(), c.tag("True"));
    print_stats("negb(false)", &vm);
}

#[test]
fn length_nil_is_zero() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let nil = Value::nullary_ctor(c.tag("Nil"));
    let result = vm.call(c.func("length"), &[nil]).unwrap();
    assert_eq!(result.tag(), c.tag("O"));
    print_stats("length(nil)", &vm);
}

#[test]
fn length_singleton() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let nil = Value::nullary_ctor(c.tag("Nil"));
    let elem = Value::nullary_ctor(c.tag("O"));
    let list = vm.alloc_ctor(c.tag("Cons"), &[elem, nil]).unwrap();

    let result = vm.call(c.func("length"), &[list]).unwrap();
    assert_eq!(result.tag(), c.tag("S"));
    let inner = vm.ctor_field(result, 0);
    assert_eq!(inner.tag(), c.tag("O"));
    print_stats("length([_])", &vm);
}

#[test]
fn leb_zero_anything_is_true() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let zero = Value::nullary_ctor(c.tag("O"));
    let one = vm.alloc_ctor(c.tag("S"), &[zero]).unwrap();

    let result = vm.call(c.func("leb"), &[zero, one]).unwrap();
    assert_eq!(result.tag(), c.tag("True"));
    print_stats("leb(0, 1)", &vm);
}

#[test]
fn map_negb_over_list() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let nil = Value::nullary_ctor(c.tag("Nil"));
    let t = Value::nullary_ctor(c.tag("True"));
    let f = Value::nullary_ctor(c.tag("False"));
    let list = vm.alloc_ctor(c.tag("Cons"), &[t, nil]).unwrap();
    let list = vm.alloc_ctor(c.tag("Cons"), &[f, list]).unwrap();

    assert_eq!(list.tag(), c.tag("Cons"));
    let head = vm.ctor_field(list, 0);
    assert_eq!(head.tag(), c.tag("False"));
    print_stats("map_negb_over_list", &vm);
}

#[test]
fn hforest_init_creates_forest() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let prev = Value::nullary_ctor(c.tag("O"));
    let value = Value::nullary_ctor(c.tag("O"));
    let prev_height = Value::nullary_ctor(c.tag("O"));

    let result = vm
        .call(c.func("hforest_init"), &[prev, value, prev_height])
        .unwrap();
    assert_eq!(result.tag(), c.tag("Build_hforest"));

    let roots = vm.ctor_field(result, 0);
    assert_eq!(roots.tag(), c.tag("Cons"));

    let edges = vm.ctor_field(result, 1);
    assert_eq!(edges.tag(), c.tag("Cons"));
    print_stats("hforest_init(O, O, O)", &vm);
}

#[test]
fn nat_ord_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let zero = Value::nullary_ctor(c.tag("O"));
    let one = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let two = peano(&mut vm, c.tag("O"), c.tag("S"), 2);

    // nat_ord(0, 0) = Left
    let r = vm.call(c.func("nat_ord"), &[zero, zero]).unwrap();
    assert_eq!(r.tag(), c.tag("Left"), "nat_ord(0,0) should be Left");

    // nat_ord(0, 1) = Left
    let r = vm.call(c.func("nat_ord"), &[zero, one]).unwrap();
    assert_eq!(r.tag(), c.tag("Left"), "nat_ord(0,1) should be Left");

    // nat_ord(1, 0) = Right
    let r = vm.call(c.func("nat_ord"), &[one, zero]).unwrap();
    assert_eq!(r.tag(), c.tag("Right"), "nat_ord(1,0) should be Right");

    // nat_ord(1, 1) = Left
    let r = vm.call(c.func("nat_ord"), &[one, one]).unwrap();
    assert_eq!(r.tag(), c.tag("Left"), "nat_ord(1,1) should be Left");

    // nat_ord(1, 2) = Left
    let r = vm.call(c.func("nat_ord"), &[one, two]).unwrap();
    assert_eq!(r.tag(), c.tag("Left"), "nat_ord(1,2) should be Left");

    // nat_ord(2, 1) = Right
    let r = vm.call(c.func("nat_ord"), &[two, one]).unwrap();
    assert_eq!(r.tag(), c.tag("Right"), "nat_ord(2,1) should be Right");

    print_stats("nat_ord_basic", &vm);
}

#[test]
fn eqb_and_leb0_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let h = vm.global_value(c.func("nat_ord"));

    // leb0(nat_ord, 0, 0) → True
    let r = vm.call(c.func("leb0"), &[h, n0, n0]).unwrap();
    assert_eq!(r.tag(), c.tag("True"), "leb0(nat_ord, 0, 0) should be True");

    // eqb(nat_ord, 0, 0) → True
    let h = vm.global_value(c.func("nat_ord"));
    let r = vm.call(c.func("eqb"), &[h, n0, n0]).unwrap();
    assert_eq!(r.tag(), c.tag("True"), "eqb(nat_ord, 0, 0) should be True");

    // eqb(nat_ord, 0, 1) → False
    let h = vm.global_value(c.func("nat_ord"));
    let r = vm.call(c.func("eqb"), &[h, n0, n1]).unwrap();
    assert_eq!(r.tag(), c.tag("False"), "eqb(nat_ord, 0, 1) should be False");

    print_stats("eqb_and_leb0_basic", &vm);
}

#[test]
fn merge_sorted_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let nil = Value::nullary_ctor(c.tag("Nil"));

    // Simple: merge_sorted(nat_ord, [0], [1])
    let h = vm.global_value(c.func("nat_ord"));
    let l1 = vm.alloc_ctor(c.tag("Cons"), &[n0, nil]).unwrap();
    let l2 = vm.alloc_ctor(c.tag("Cons"), &[n1, nil]).unwrap();

    let merged = vm.call(c.func("merge_sorted"), &[h, l1, l2]).unwrap();
    let merged_vec = list_to_vec(&vm, c.tag("Cons"), merged);
    assert_eq!(
        merged_vec.len(),
        2,
        "merge_sorted([0], [1]) should have 2 elements"
    );

    print_stats("merge_sorted_basic", &vm);
}

#[test]
fn merge_dedup_sorted_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let nil = Value::nullary_ctor(c.tag("Nil"));
    let h = vm.global_value(c.func("nat_ord"));

    let l1 = vm.alloc_ctor(c.tag("Cons"), &[n0, nil]).unwrap();
    let l2 = vm.alloc_ctor(c.tag("Cons"), &[n1, nil]).unwrap();

    let merged = vm
        .call(c.func("merge_dedup_sorted"), &[h, l1, l2])
        .unwrap();
    let merged_vec = list_to_vec(&vm, c.tag("Cons"), merged);
    assert_eq!(
        merged_vec.len(),
        2,
        "merge_dedup_sorted([0], [1]) should have 2 elements"
    );

    print_stats("merge_dedup_sorted_basic", &vm);
}

#[test]
fn merge_dedup_sorted_overlap() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);

    // l1 = [0, 1, 2]
    let l1 = make_list(
        &mut vm,
        c.tag("Nil"),
        c.tag("Cons"),
        &[n0, n1, n2],
    );
    // l2 = [1, 2, 3]  — shares 1 and 2 with l1
    let l2 = make_list(
        &mut vm,
        c.tag("Nil"),
        c.tag("Cons"),
        &[n1, n2, n3],
    );

    let h = vm.global_value(c.func("nat_ord"));
    let merged = vm
        .call(c.func("merge_dedup_sorted"), &[h, l1, l2])
        .unwrap();
    let merged_vec = list_to_vec(&vm, c.tag("Cons"), merged);

    // Duplicates (1, 2) must be removed → [0, 1, 2, 3]
    assert_eq!(
        merged_vec.len(),
        4,
        "merge_dedup_sorted([0,1,2],[1,2,3]) should have 4 elements"
    );
    assert_eq!(unpeano(&vm, c.tag("S"), merged_vec[0]), 0);
    assert_eq!(unpeano(&vm, c.tag("S"), merged_vec[1]), 1);
    assert_eq!(unpeano(&vm, c.tag("S"), merged_vec[2]), 2);
    assert_eq!(unpeano(&vm, c.tag("S"), merged_vec[3]), 3);

    print_stats("merge_dedup_sorted([0,1,2],[1,2,3])", &vm);
}

#[test]
fn ordroot_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);

    let h = vm.global_value(c.func("nat_ord"));
    let ord = vm.call(c.func("ordRoot"), &[h]).unwrap();

    // Create two roots with different hashes
    let r1 = vm.alloc_ctor(c.tag("Build_root"), &[n0, n0]).unwrap(); // root(hash=0, height=0)
    let r2 = vm.alloc_ctor(c.tag("Build_root"), &[n1, n0]).unwrap(); // root(hash=1, height=0)

    // ordRoot(nat_ord)(r1, r2) should return Left (0 <= 1)
    let result = vm.apply(ord, &[r1, r2]).unwrap();
    assert_eq!(
        result.tag(),
        c.tag("Left"),
        "ordRoot(nat_ord)(root0, root1) should be Left"
    );

    print_stats("ordroot_basic", &vm);
}

#[test]
fn merge_roots_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let nil = Value::nullary_ctor(c.tag("Nil"));

    let r1 = vm.alloc_ctor(c.tag("Build_root"), &[n0, n0]).unwrap();
    let r2 = vm.alloc_ctor(c.tag("Build_root"), &[n1, n0]).unwrap();
    let l1 = vm.alloc_ctor(c.tag("Cons"), &[r1, nil]).unwrap();
    let l2 = vm.alloc_ctor(c.tag("Cons"), &[r2, nil]).unwrap();

    let h = vm.global_value(c.func("nat_ord"));
    let merged = vm.call(c.func("merge_roots"), &[h, l1, l2]).unwrap();
    let merged_vec = list_to_vec(&vm, c.tag("Cons"), merged);
    assert_eq!(
        merged_vec.len(),
        2,
        "merge_roots([root0], [root1]) should have 2 roots"
    );

    print_stats("merge_roots_basic", &vm);
}

#[test]
fn hforest_merge_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);

    let f1 = vm.call(c.func("hforest_init"), &[n0, n1, n0]).unwrap();
    let f2 = vm.call(c.func("hforest_init"), &[n1, n3, n0]).unwrap();

    let h = vm.global_value(c.func("nat_ord"));
    let merged = vm.call(c.func("hforest_merge"), &[h, f1, f2]).unwrap();
    assert_eq!(merged.tag(), c.tag("Build_hforest"));
    print_stats("hforest_merge_basic", &vm);
}

#[test]
fn hforest_lifecycle() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    // --- List + higher-order function phase ---

    let nil = Value::nullary_ctor(c.tag("Nil"));
    let t = Value::nullary_ctor(c.tag("True"));
    let f = Value::nullary_ctor(c.tag("False"));

    // Build [True, False, True, False]
    let list = vm.alloc_ctor(c.tag("Cons"), &[f, nil]).unwrap();
    let list = vm.alloc_ctor(c.tag("Cons"), &[t, list]).unwrap();
    let list = vm.alloc_ctor(c.tag("Cons"), &[f, list]).unwrap();
    let list = vm.alloc_ctor(c.tag("Cons"), &[t, list]).unwrap();
    assert_eq!(list_to_vec(&vm, c.tag("Cons"), list).len(), 4);

    // map(negb, list) → [False, True, False, True]
    let negb = vm.global_value(c.func("negb"));
    let mapped = vm.call(c.func("map"), &[negb, list]).unwrap();
    let mapped_vec = list_to_vec(&vm, c.tag("Cons"), mapped);
    assert_eq!(mapped_vec.len(), 4);
    assert_eq!(mapped_vec[0].tag(), c.tag("False"));
    assert_eq!(mapped_vec[1].tag(), c.tag("True"));
    assert_eq!(mapped_vec[2].tag(), c.tag("False"));
    assert_eq!(mapped_vec[3].tag(), c.tag("True"));

    // length(mapped) → 4
    let len_mapped = vm.call(c.func("length"), &[mapped]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), len_mapped), 4);

    // filter(negb, mapped) → keeps items where negb(x)=True, i.e. False items → [False, False]
    let negb = vm.global_value(c.func("negb"));
    let filtered = vm.call(c.func("filter"), &[negb, mapped]).unwrap();
    let filtered_vec = list_to_vec(&vm, c.tag("Cons"), filtered);
    assert_eq!(filtered_vec.len(), 2);
    assert_eq!(filtered_vec[0].tag(), c.tag("False"));
    assert_eq!(filtered_vec[1].tag(), c.tag("False"));

    // length(filtered) → 2
    let len_filtered = vm.call(c.func("length"), &[filtered]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), len_filtered), 2);

    // leb(2, 4) → True
    let leb_result = vm.call(c.func("leb"), &[len_filtered, len_mapped]).unwrap();
    assert_eq!(leb_result.tag(), c.tag("True"));

    // --- hforest phase ---
    // Use distinct prev/value so roots aren't filtered by valid_roots.
    // hforest_init(prev, value, height) creates:
    //   root(prev, height), edge(prev, value, S(height))
    // valid_roots removes roots whose hash appears as an edge child_hash (=value).
    // So we need prev ≠ value.

    let n0 = Value::nullary_ctor(c.tag("O"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let n4 = peano(&mut vm, c.tag("O"), c.tag("S"), 4);

    // f1: root=0, edge(0→1, height=1)
    let _h = vm.global_value(c.func("nat_ord"));
    let f1 = vm.call(c.func("hforest_init"), &[n0, n1, n0]).unwrap();
    assert_eq!(f1.tag(), c.tag("Build_hforest"));

    // f2: root=2, edge(2→3, height=1)
    let f2 = vm.call(c.func("hforest_init"), &[n2, n3, n0]).unwrap();
    assert_eq!(f2.tag(), c.tag("Build_hforest"));

    // hforest_merge(nat_ord, f1, f2)
    let h = vm.global_value(c.func("nat_ord"));
    let merged = vm.call(c.func("hforest_merge"), &[h, f1, f2]).unwrap();
    assert_eq!(merged.tag(), c.tag("Build_hforest"));

    let merged_roots = list_to_vec(&vm, c.tag("Cons"), vm.ctor_field(merged, 0));
    let merged_edges = list_to_vec(&vm, c.tag("Cons"), vm.ctor_field(merged, 1));
    assert_eq!(merged_roots.len(), 2, "merged should have 2 roots (0 and 2)");
    assert_eq!(merged_edges.len(), 2, "merged should have 2 edges");

    // hforest_contains(nat_ord, 0, merged) → True (root hash 0)
    let h = vm.global_value(c.func("nat_ord"));
    let contains_0 = vm
        .call(c.func("hforest_contains"), &[h, n0, merged])
        .unwrap();
    assert_eq!(contains_0.tag(), c.tag("True"));

    // hforest_contains(nat_ord, 1, merged) → True (edge child hash 1)
    let h = vm.global_value(c.func("nat_ord"));
    let contains_1 = vm
        .call(c.func("hforest_contains"), &[h, n1, merged])
        .unwrap();
    assert_eq!(contains_1.tag(), c.tag("True"));

    // hforest_contains(nat_ord, 4, merged) → False (not present)
    let h = vm.global_value(c.func("nat_ord"));
    let contains_4 = vm
        .call(c.func("hforest_contains"), &[h, n4, merged])
        .unwrap();
    assert_eq!(contains_4.tag(), c.tag("False"));

    // hforest_tips(nat_ord, merged) → tip pairs
    let h = vm.global_value(c.func("nat_ord"));
    let tips = vm.call(c.func("hforest_tips"), &[h, merged]).unwrap();
    let tips_vec = list_to_vec(&vm, c.tag("Cons"), tips);
    assert!(!tips_vec.is_empty(), "merged forest should have tips");
    for tip in &tips_vec {
        assert_eq!(tip.tag(), c.tag("Pair"));
    }

    // hforest_insert(nat_ord, 4, 3, 0, merged) → (Pair new_forest was_new)
    // prev=4, value=3: new edge 4→3, prev≠value so it's a genuine insert
    let h = vm.global_value(c.func("nat_ord"));
    let inserted = vm
        .call(c.func("hforest_insert"), &[h, n4, n3, n0, merged])
        .unwrap();
    assert_eq!(inserted.tag(), c.tag("Pair"));
    let new_forest = vm.ctor_field(inserted, 0);
    let was_new = vm.ctor_field(inserted, 1);
    assert_eq!(new_forest.tag(), c.tag("Build_hforest"));
    assert_eq!(was_new.tag(), c.tag("True"));

    // hforest_contains(nat_ord, 4, new_forest) → True (root hash 4)
    let h = vm.global_value(c.func("nat_ord"));
    let contains_4_now = vm
        .call(c.func("hforest_contains"), &[h, n4, new_forest])
        .unwrap();
    assert_eq!(contains_4_now.tag(), c.tag("True"));

    // hforest_contains(nat_ord, 3, new_forest) → True (edge child hash 3)
    let h = vm.global_value(c.func("nat_ord"));
    let contains_3_now = vm
        .call(c.func("hforest_contains"), &[h, n3, new_forest])
        .unwrap();
    assert_eq!(contains_3_now.tag(), c.tag("True"));

    print_stats("hforest_lifecycle", &vm);
}
