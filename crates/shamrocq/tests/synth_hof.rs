mod common;

use common::{compile_scheme, peano, unpeano, print_stats, Compiled};
use shamrocq::{Program, Value, Vm};

fn setup() -> Compiled {
    compile_scheme(&["hash_forest.scm", "synth_arith.scm", "synth_hof.scm"])
}

#[test]
fn compose_negb_negb() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let negb = vm.global_value(c.func("negb"));
    let composed = vm.call(c.func("compose"), &[negb, negb]).unwrap();
    let result = vm
        .apply(composed, &[Value::nullary_ctor(c.tag("True"))])
        .unwrap();
    assert_eq!(
        result.tag(),
        c.tag("True"),
        "compose(negb, negb)(True) = True"
    );
    print_stats("compose(negb,negb)", &vm);
}

#[test]
fn flip_sub() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    // flip(sub)(2, 5) = sub(5, 2) = 3
    let sub_fn = vm.global_value(c.func("sub"));
    let flipped = vm.call(c.func("flip"), &[sub_fn]).unwrap();
    let n2 = peano(&mut vm, c.tag("O"), c.tag("S"), 2);
    let n5 = peano(&mut vm, c.tag("O"), c.tag("S"), 5);
    let result = vm.apply(flipped, &[n2, n5]).unwrap();
    assert_eq!(unpeano(&vm, c.tag("S"), result), 3);
    print_stats("flip(sub)(2,5)", &vm);
}

#[test]
fn const_fn_basic() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let t = Value::nullary_ctor(c.tag("True"));
    let f = Value::nullary_ctor(c.tag("False"));
    let always_true = vm.call(c.func("const_fn"), &[t]).unwrap();
    let result = vm.apply(always_true, &[f]).unwrap();
    assert_eq!(result.tag(), c.tag("True"));
    print_stats("const_fn(True)(False)", &vm);
}

#[test]
fn twice_negb() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    let negb = vm.global_value(c.func("negb"));
    let double_neg = vm.call(c.func("twice"), &[negb]).unwrap();
    let result = vm
        .apply(double_neg, &[Value::nullary_ctor(c.tag("False"))])
        .unwrap();
    assert_eq!(
        result.tag(),
        c.tag("False"),
        "twice(negb)(False) = False"
    );
    print_stats("twice(negb)", &vm);
}

#[test]
fn apply_n_successor() {
    let c = setup();
    let prog = Program::from_blob(&c.blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&prog).unwrap();

    // negb applied 4 times to True: negb^4(True) = True
    let negb = vm.global_value(c.func("negb"));
    let n4 = peano(&mut vm, c.tag("O"), c.tag("S"), 4);
    let t = Value::nullary_ctor(c.tag("True"));
    let result = vm.call(c.func("apply_n"), &[negb, n4, t]).unwrap();
    assert_eq!(
        result.tag(),
        c.tag("True"),
        "apply_n(negb, 4, True) = True"
    );

    let n3 = peano(&mut vm, c.tag("O"), c.tag("S"), 3);
    let result = vm.call(c.func("apply_n"), &[negb, n3, t]).unwrap();
    assert_eq!(
        result.tag(),
        c.tag("False"),
        "apply_n(negb, 3, True) = False"
    );
    print_stats("apply_n(negb,n,True)", &vm);
}
