mod common;

use common::{compile_scheme, list_to_vec, make_list, peano, unpeano, print_stats, Compiled};
use shamrocq::{Program, Value, Vm};

fn setup() -> Compiled {
    compile_scheme(&["synth_list.scm", "synth_option.scm"])
}

#[test]
fn append_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = peano(&mut vm, c.tag("O"), c.tag("S"), 0);
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let l1 = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n0, n1]);
    let l2 = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n2, n3]);

    let result = vm.call(c.func("append"), &[l1, l2]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 4);
    assert_eq!(unpeano(&vm, c.tag("S"), v[0]), 0);
    assert_eq!(unpeano(&vm, c.tag("S"), v[3]), 3);
    print_stats("append([0,1],[2,3])", &vm);
}

#[test]
fn append_empty() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let nil = Value::nullary_ctor(c.tag("Nil"));
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let l = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n1]);

    let result = vm.call(c.func("append"), &[nil, l]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 1);
    print_stats("append([],[1])", &vm);
}

#[test]
fn reverse_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let l = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n1, n2, n3]);

    let result = vm.call(c.func("reverse"), &[l]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 3);
    assert_eq!(unpeano(&vm, c.tag("S"), v[0]), 3);
    assert_eq!(unpeano(&vm, c.tag("S"), v[2]), 1);
    print_stats("reverse([1,2,3])", &vm);
}

#[test]
fn nth_found() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = peano(&mut vm, c.tag("O"), c.tag("S"), 0);
    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let l = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n0, n1, n2]);

    let idx = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let result = vm.call(c.func("nth"), &[idx, l]).unwrap();
    assert_eq!(result.tag(), c.tag("Some"));
    assert_eq!(unpeano(&vm, c.tag("S"), vm.ctor_field(result, 0)), 1);
    print_stats("nth(1,[0,1,2])", &vm);
}

#[test]
fn nth_out_of_bounds() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = peano(&mut vm, c.tag("O"), c.tag("S"), 0);
    let l = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n0]);

    let idx = peano(&mut vm, c.tag("O"), c.tag("S"), 5);
    let result = vm.call(c.func("nth"), &[idx, l]).unwrap();
    assert_eq!(result.tag(), c.tag("None_"));
    print_stats("nth(5,[0])", &vm);
}

#[test]
fn zip_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let n4 = peano(&mut vm, c.tag("O"), c.tag("S"), 4);
    let l1 = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n1, n2]);
    let l2 = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n3, n4]);

    let result = vm.call(c.func("zip"), &[l1, l2]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].tag(), c.tag("Pair"));
    assert_eq!(unpeano(&vm, c.tag("S"), vm.ctor_field(v[0], 0)), 1);
    assert_eq!(unpeano(&vm, c.tag("S"), vm.ctor_field(v[0], 1)), 3);
    print_stats("zip([1,2],[3,4])", &vm);
}

#[test]
fn zip_uneven() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n1 = peano(&mut vm, c.tag("O"), c.tag("S"), 1);
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let l1 = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n1, n2, n3]);
    let l2 = make_list(&mut vm, c.tag("Nil"), c.tag("Cons"), &[n1]);

    let result = vm.call(c.func("zip"), &[l1, l2]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 1);
    print_stats("zip([1,2,3],[1])", &vm);
}

#[test]
fn lrange_100() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let result = vm.call(c.func("lrange"), &[Value::integer(0), Value::integer(100)]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 100);
    assert_eq!(v[0].integer_value(), 0);
    assert_eq!(v[99].integer_value(), 99);
    print_stats("lrange(0,100)", &vm);
}

#[test]
fn lrange_200() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let result = vm.call(c.func("lrange"), &[Value::integer(0), Value::integer(200)]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 200);
    print_stats("lrange(0,200)", &vm);
}

#[test]
fn map_over_lrange_100() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let range = vm.call(c.func("lrange"), &[Value::integer(1), Value::integer(101)]).unwrap();
    let wrap = vm.call(c.func("wrap_some"), &[]).unwrap();
    let result = vm.call(c.func("list_map"), &[wrap, range]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 100);
    assert_eq!(v[0].tag(), c.tag("Some"));
    print_stats("map(wrap_some, lrange(1,101))", &vm);
}

#[test]
fn filter_lrange_100() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let range = vm.call(c.func("lrange"), &[Value::integer(0), Value::integer(100)]).unwrap();
    let is_pos = vm.call(c.func("is_positive"), &[]).unwrap();
    let result = vm.call(c.func("list_filter"), &[is_pos, range]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 99);
    print_stats("filter(is_positive, lrange(0,100))", &vm);
}

#[test]
fn reverse_lrange_100() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let range = vm.call(c.func("lrange"), &[Value::integer(0), Value::integer(100)]).unwrap();
    let result = vm.call(c.func("reverse"), &[range]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 100);
    assert_eq!(v[0].integer_value(), 99);
    assert_eq!(v[99].integer_value(), 0);
    print_stats("reverse(lrange(0,100))", &vm);
}
