mod common;

use common::{compile_scheme, peano, unpeano, print_stats, Compiled};
use shamrocq::{Program, Value, Vm};

fn setup() -> Compiled {
    compile_scheme(&["hash_forest.scm", "synth_option.scm"])
}

#[test]
fn option_map_some() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let negb = vm.global_value(c.func("negb"));
    let some_true = vm.alloc_ctor(c.tag("Some"), &[Value::nullary_ctor(c.tag("True"))]).unwrap();
    let result = vm.call(c.func("option_map"), &[negb, some_true]).unwrap();
    assert_eq!(result.tag(), c.tag("Some"));
    assert_eq!(vm.ctor_field(result, 0).tag(), c.tag("False"));
    print_stats("option_map(negb, Some(True))", &vm);
}

#[test]
fn option_map_none() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let negb = vm.global_value(c.func("negb"));
    let none = Value::nullary_ctor(c.tag("None_"));
    let result = vm.call(c.func("option_map"), &[negb, none]).unwrap();
    assert_eq!(result.tag(), c.tag("None_"));
    print_stats("option_map(negb, None)", &vm);
}

#[test]
fn option_bind_some() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    // option_bind(wrap_some, Some(True)) = wrap_some(True) = Some(True)
    let wrap = vm.global_value(c.func("wrap_some"));
    let some_true = vm.alloc_ctor(c.tag("Some"), &[Value::nullary_ctor(c.tag("True"))]).unwrap();
    let result = vm.call(c.func("option_bind"), &[wrap, some_true]).unwrap();
    assert_eq!(result.tag(), c.tag("Some"));
    assert_eq!(vm.ctor_field(result, 0).tag(), c.tag("True"));
    print_stats("option_bind(wrap_some, Some(True))", &vm);
}

#[test]
fn option_bind_none() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let negb = vm.global_value(c.func("negb"));
    let none = Value::nullary_ctor(c.tag("None_"));
    let result = vm.call(c.func("option_bind"), &[negb, none]).unwrap();
    assert_eq!(result.tag(), c.tag("None_"));
    print_stats("option_bind(negb, None)", &vm);
}

#[test]
fn option_default_some() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n5 = peano(&mut vm, c.tag("O"), c.tag("S"), 5);
    let n0 = peano(&mut vm, c.tag("O"), c.tag("S"), 0);
    let some_5 = vm.alloc_ctor(c.tag("Some"), &[n5]).unwrap();
    let result = vm.call(c.func("option_default"), &[n0, some_5]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 5);
    print_stats("option_default(0, Some(5))", &vm);
}

#[test]
fn option_default_none() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let n0 = peano(&mut vm, c.tag("O"), c.tag("S"), 0);
    let none = Value::nullary_ctor(c.tag("None_"));
    let result = vm.call(c.func("option_default"), &[n0, none]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 0);
    print_stats("option_default(0, None)", &vm);
}

#[test]
fn option_is_some_and_none() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let some = vm.alloc_ctor(c.tag("Some"), &[Value::nullary_ctor(c.tag("O"))]).unwrap();
    let none = Value::nullary_ctor(c.tag("None_"));

    let r1 = vm.call(c.func("option_is_some"), &[some]).unwrap();
    assert_eq!(r1.tag(), c.tag("True"));

    let r2 = vm.call(c.func("option_is_some"), &[none]).unwrap();
    assert_eq!(r2.tag(), c.tag("False"));

    let r3 = vm.call(c.func("option_is_none"), &[none]).unwrap();
    assert_eq!(r3.tag(), c.tag("True"));

    let r4 = vm.call(c.func("option_is_none"), &[some]).unwrap();
    assert_eq!(r4.tag(), c.tag("False"));

    print_stats("option_is_some/none", &vm);
}
