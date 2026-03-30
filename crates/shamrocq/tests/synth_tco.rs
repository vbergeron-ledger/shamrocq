mod common;

use shamrocq::{Program, Value, Vm};
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

/// Simple tail-recursive countdown. Without TCO this would need 100_000
/// call frames, far exceeding MAX_CALL_DEPTH (256).
#[test]
fn tco_simple_tail_recursion() {
    let src = r#"
        (define count-down (lambda (n)
          (if (= n 0) 0 (count-down (- n 1)))))
    "#;
    let (blob, funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 4096];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = funcs["count-down"];
    let result = vm.call(f, &[Value::integer(100_000)]).unwrap();
    assert_eq!(result.integer_value(), 0);
}

/// Tail-recursive accumulator with two arguments (tests TCO through
/// the curried multi-arg calling convention).
#[test]
fn tco_accumulator() {
    let src = r#"
        (define sum-acc (lambdas (acc n)
          (if (= n 0) acc (@ sum-acc (+ acc n) (- n 1)))))
    "#;
    let (blob, funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let f = funcs["sum-acc"];
    let result = vm.call(f, &[Value::integer(0), Value::integer(1_000)]).unwrap();
    assert_eq!(result.integer_value(), 500_500);
}

/// Mutual tail recursion: even?/odd? ping-pong. Tests TCO across
/// two functions calling each other in tail position.
#[test]
fn tco_mutual_recursion() {
    let src = r#"
        (define my-even (lambda (n)
          (if (= n 0) `(True) (my-odd (- n 1)))))
        (define my-odd (lambda (n)
          (if (= n 0) `(False) (my-even (- n 1)))))
    "#;
    let (blob, funcs) = compile_inline(src);
    let prog = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 4096];
    let mut vm = Vm::new(&mut buf);
    vm.load_program(&prog).unwrap();

    let even = funcs["my-even"];
    let result = vm.call(even, &[Value::integer(100_000)]).unwrap();
    assert_eq!(result.tag(), shamrocq_bytecode::tags::TRUE);

    vm.reset();
    vm.load_program(&prog).unwrap();
    let result = vm.call(even, &[Value::integer(99_999)]).unwrap();
    assert_eq!(result.tag(), shamrocq_bytecode::tags::FALSE);
}
