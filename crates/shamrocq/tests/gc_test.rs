mod common;

use common::{compile_scheme, list_to_vec, print_stats};
use shamrocq::{Program, Value, Vm};

fn setup_sort() -> common::Compiled {
    compile_scheme(&["../examples/sort/scheme/sort.scm"])
}

fn setup_list() -> common::Compiled {
    compile_scheme(&["synth_list.scm", "synth_option.scm"])
}

#[test]
fn gc_reverse_small_heap() {
    let c = setup_list();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 4096];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let range = vm
        .call(c.func("lrange"), &[Value::integer(0), Value::integer(100)])
        .unwrap();
    let result = vm.call(c.func("reverse"), &[range]).unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 100);
    assert_eq!(v[0].integer_value(), 99);
    assert_eq!(v[99].integer_value(), 0);
    print_stats("gc_reverse_small_heap", &vm);
}

#[test]
fn gc_sort_small_heap() {
    let c = setup_sort();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 8192];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let result = vm
        .call(c.func("sort_seq"), &[Value::integer(20)])
        .unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 20);
    for i in 0..20 {
        assert_eq!(v[i].integer_value(), i as i32 + 1);
    }
    print_stats("gc_sort_small_heap(20)", &vm);
}

#[test]
fn gc_sort_medium_heap() {
    let c = setup_sort();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 16384];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let result = vm
        .call(c.func("sort_seq"), &[Value::integer(60)])
        .unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 60);
    for i in 0..60 {
        assert_eq!(v[i].integer_value(), i as i32 + 1);
    }
    print_stats("gc_sort_medium_heap(60)", &vm);
}

#[test]
fn gc_sort_large_heap() {
    let c = setup_sort();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 131072];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let result = vm
        .call(c.func("sort_seq"), &[Value::integer(500)])
        .unwrap();
    let v = list_to_vec(&vm, c.tag("Cons"), result);
    assert_eq!(v.len(), 500);
    for i in 0..500 {
        assert_eq!(v[i].integer_value(), i as i32 + 1);
    }
    print_stats("gc_sort_large_heap(500)", &vm);
}
