mod common;

use shamrocq::{Program, Value, Vm, VmError};
use shamrocq_compiler::codegen::compile_program;
use shamrocq_compiler::desugar::desugar_program;
use shamrocq_compiler::parser::parse;
use shamrocq_compiler::resolve::{resolve_program, GlobalTable, TagTable};

fn compile_inline(src: &str) -> (Vec<u8>, std::collections::HashMap<String, u16>) {
    let sexps = parse(src).unwrap();
    let defs = desugar_program(&sexps).unwrap();
    let mut tags = TagTable::new();
    let mut globals = GlobalTable::new();
    let rdefs = resolve_program(&defs, &mut tags, &mut globals).unwrap();
    let prog = compile_program(&rdefs);
    let funcs = prog
        .header
        .globals
        .iter()
        .enumerate()
        .map(|(i, (name, _))| (name.clone(), i as u16))
        .collect();
    (prog.serialize(), funcs)
}

fn double_it(_vm: &mut Vm<'_>, arg: Value) -> Result<Value, VmError> {
    Ok(Value::integer(arg.integer_value() * 2))
}

fn negate_it(_vm: &mut Vm<'_>, arg: Value) -> Result<Value, VmError> {
    Ok(Value::integer(-arg.integer_value()))
}

fn clamp_packed(vm: &mut Vm<'_>, arg: Value) -> Result<Value, VmError> {
    let lo = vm.ctor_field(arg, 0).integer_value();
    let hi = vm.ctor_field(arg, 1).integer_value();
    let x  = vm.ctor_field(arg, 2).integer_value();
    Ok(Value::integer(x.max(lo).min(hi)))
}

#[test]
fn foreign_fn_multiarg_syntax() {
    let src = r#"
        (define-foreign clamp (lo hi x))
    "#;
    let (blob, funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.register_foreign(0, clamp_packed);
    vm.load_program(&prog).unwrap();

    let f = funcs["clamp"];
    let lo = Value::integer(0);
    let hi = Value::integer(100);
    assert_eq!(vm.call(f, &[lo, hi, Value::integer(42)]).unwrap().integer_value(), 42);
    assert_eq!(vm.call(f, &[lo, hi, Value::integer(-5)]).unwrap().integer_value(), 0);
    assert_eq!(vm.call(f, &[lo, hi, Value::integer(200)]).unwrap().integer_value(), 100);
}

#[test]
fn foreign_fn_direct_call() {
    let src = r#"
        (define-foreign double-it)
        (define use-foreign (lambda (x) (double-it x)))
    "#;
    let (blob, funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.register_foreign(0, double_it);
    vm.load_program(&prog).unwrap();

    let f = funcs["use-foreign"];
    let result = vm.call(f, &[Value::integer(21)]).unwrap();
    assert_eq!(result.integer_value(), 42);
}

#[test]
fn foreign_fn_multiple() {
    let src = r#"
        (define-foreign double-it)
        (define-foreign negate-it)
        (define double-then-negate (lambda (x) (negate-it (double-it x))))
    "#;
    let (blob, funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.register_foreign(0, double_it);
    vm.register_foreign(1, negate_it);
    vm.load_program(&prog).unwrap();

    let f = funcs["double-then-negate"];
    let result = vm.call(f, &[Value::integer(5)]).unwrap();
    assert_eq!(result.integer_value(), -10);
}

#[test]
fn foreign_fn_in_tail_position() {
    let src = r#"
        (define-foreign double-it)
        (define tail-call-foreign (lambda (x) (double-it x)))
    "#;
    let (blob, funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.register_foreign(0, double_it);
    vm.load_program(&prog).unwrap();

    let f = funcs["tail-call-foreign"];
    let result = vm.call(f, &[Value::integer(100)]).unwrap();
    assert_eq!(result.integer_value(), 200);
}

#[test]
fn foreign_fn_called_via_apply() {
    let ffn = Value::foreign_fn(0, 1);
    assert!(ffn.is_foreign_fn());
    assert!(ffn.is_callable());
    assert_eq!(ffn.fn_addr(), 0);

    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.register_foreign(0, double_it);

    let result = vm.apply(ffn, &[Value::integer(7)]).unwrap();
    assert_eq!(result.integer_value(), 14);
}

#[test]
fn foreign_fn_compiler_assigns_indices_in_order() {
    let src = r#"
        (define-foreign alpha)
        (define-foreign beta)
        (define-foreign gamma)
    "#;
    let (blob, _funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);

    fn always_zero(_vm: &mut Vm<'_>, _arg: Value) -> Result<Value, VmError> {
        Ok(Value::integer(0))
    }
    vm.register_foreign(0, always_zero);
    vm.register_foreign(1, always_zero);
    vm.register_foreign(2, always_zero);
    vm.load_program(&prog).unwrap();

    // Globals 0, 1, 2 should be foreign_fn values.
    assert!(vm.global_value(0).is_foreign_fn());
    assert!(vm.global_value(1).is_foreign_fn());
    assert!(vm.global_value(2).is_foreign_fn());
    assert_eq!(vm.global_value(0).fn_addr(), 0);
    assert_eq!(vm.global_value(1).fn_addr(), 1);
    assert_eq!(vm.global_value(2).fn_addr(), 2);
}
