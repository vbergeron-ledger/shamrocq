mod common;

use common::{compile_scheme, list_to_vec, peano, unpeano, print_stats, Compiled};
use shamrocq::{Program, Value, Vm};

fn setup() -> Compiled {
    compile_scheme(&[
        "test_helpers.scm",
        "hash_forest.scm",
        "synth_arith.scm",
        "synth_option.scm",
        "synth_assoc.scm",
    ])
}

fn make_assoc(vm: &mut Vm, c: &Compiled, pairs: &[(u32, u32)]) -> Value {
    let mut list = Value::nullary_ctor(c.tag("Nil"));
    for &(k, v) in pairs.iter().rev() {
        let key = peano(vm, c.tag("O"), c.tag("S"), k);
        let val = peano(vm, c.tag("O"), c.tag("S"), v);
        let pair = vm.alloc_ctor(c.tag("Pair"), &[key, val]).unwrap();
        list = vm.alloc_ctor(c.tag("Cons"), &[pair, list]).unwrap();
    }
    list
}

#[test]
fn assoc_get_found() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let alist = make_assoc(&mut vm, &c, &[(1, 10), (2, 20), (3, 30)]);
    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);

    let result = vm.call(c.func("assoc_get"), &[ord, key, alist]).unwrap();
    assert_eq!(result.tag(), c.tag("Some"));
    assert_eq!(
        unpeano(&vm, c.tag("S"), vm.ctor_field(result, 0)),
        20
    );
    print_stats("assoc_get(2, [(1,10),(2,20),(3,30)])", &vm);
}

#[test]
fn assoc_get_not_found() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let alist = make_assoc(&mut vm, &c, &[(1, 10), (3, 30)]);
    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);

    let result = vm.call(c.func("assoc_get"), &[ord, key, alist]).unwrap();
    assert_eq!(result.tag(), c.tag("None_"));
    print_stats("assoc_get(2, [(1,10),(3,30)])", &vm);
}

#[test]
fn assoc_set_new_key() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let alist = make_assoc(&mut vm, &c, &[(1, 10)]);
    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let val = peano(&mut vm, c.tag("O"), c.tag("S"), 20);

    let updated = vm
        .call(c.func("assoc_set"), &[ord, key, val, alist])
        .unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), updated);
    assert_eq!(v.len(), 2);

    // Verify we can get the new key
    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let result = vm.call(c.func("assoc_get"), &[ord, key, updated]).unwrap();
    assert_eq!(result.tag(), c.tag("Some"));
    assert_eq!(
        unpeano(&vm, c.tag("S"), vm.ctor_field(result, 0)),
        20
    );
    print_stats("assoc_set(2, 20, [(1,10)])", &vm);
}

#[test]
fn assoc_set_overwrite() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let alist = make_assoc(&mut vm, &c, &[(1, 10), (2, 20)]);
    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let val = peano(&mut vm, c.tag("O"), c.tag("S"), 99);

    let updated = vm
        .call(c.func("assoc_set"), &[ord, key, val, alist])
        .unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), updated);
    assert_eq!(v.len(), 2, "overwrite should not increase length");

    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let result = vm.call(c.func("assoc_get"), &[ord, key, updated]).unwrap();
    assert_eq!(result.tag(), c.tag("Some"));
    assert_eq!(
        unpeano(&vm, c.tag("S"), vm.ctor_field(result, 0)),
        99
    );
    print_stats("assoc_set(2, 99, [(1,10),(2,20)])", &vm);
}

#[test]
fn assoc_remove_existing() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let alist = make_assoc(&mut vm, &c, &[(1, 10), (2, 20), (3, 30)]);
    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);

    let removed = vm
        .call(c.func("assoc_remove"), &[ord, key, alist])
        .unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), removed);
    assert_eq!(v.len(), 2);

    let ord = vm.global_value(c.func("nat_ord"));
    let key = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let result = vm.call(c.func("assoc_get"), &[ord, key, removed]).unwrap();
    assert_eq!(result.tag(), c.tag("None_"));
    print_stats("assoc_remove(2, [(1,10),(2,20),(3,30)])", &vm);
}

#[test]
fn assoc_keys_and_values() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let alist = make_assoc(&mut vm, &c, &[(1, 10), (2, 20), (3, 30)]);

    let keys = vm.call(c.func("assoc_keys"), &[alist]).unwrap();
    let kv = list_to_vec(&vm, c.tag("Cons"), keys);
    assert_eq!(kv.len(), 3);
    let key_nums: Vec<u32> = kv.iter().map(|x| unpeano(&vm, c.tag("S"), *x)).collect();
    assert_eq!(key_nums, vec![1, 2, 3]);

    let values = vm.call(c.func("assoc_values"), &[alist]).unwrap();
    let vv = list_to_vec(&vm, c.tag("Cons"), values);
    assert_eq!(vv.len(), 3);
    let val_nums: Vec<u32> = vv.iter().map(|x| unpeano(&vm, c.tag("S"), *x)).collect();
    assert_eq!(val_nums, vec![10, 20, 30]);

    print_stats("assoc_keys_and_values", &vm);
}
