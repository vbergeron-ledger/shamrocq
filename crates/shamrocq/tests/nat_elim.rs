mod common;

use shamrocq::{Program, Value, Vm};
use shamrocq_compiler::pass::PassConfig;

fn run_rev_range(config: &PassConfig, n: i32) -> Vec<i32> {
    let src = r#"
        (define rev_range (lambda (n)
          ((lambdas (fO fS n) (if (= n 0) (fO 0) (fS (- n 1))))
             (lambda (_) `(Nil))
             (lambda (m) `(Cons ,n ,(rev_range m)))
             n)))
    "#;
    let (prog, tags) = shamrocq_compiler::compile_sources_with_config(
        &[src],
        shamrocq_compiler::DEFAULT_MAX_PASS_ITERATIONS,
        config,
    ).unwrap();
    let blob = prog.serialize();
    let p = Program::from_blob(&blob).unwrap();
    let mut buf = vec![0u8; 65536];
    let mut vm = Vm::new(&mut buf);
    vm.load(&p).unwrap();
    let result = vm.call(0, &[Value::integer(n)]).unwrap();
    let cons_tag = tags.get("Cons").unwrap();
    common::list_to_vec(&vm, cons_tag, result)
        .iter()
        .map(|v| v.integer_value())
        .collect()
}

#[test]
fn rev_range_without_case_nat() {
    let mut config = PassConfig::new();
    config.set("case_nat", false);
    assert_eq!(run_rev_range(&config, 3), vec![3, 2, 1]);
}

#[test]
fn rev_range_with_case_nat() {
    let config = PassConfig::new();
    assert_eq!(run_rev_range(&config, 3), vec![3, 2, 1]);
}

#[test]
fn rev_range_zero_with_case_nat() {
    let config = PassConfig::new();
    assert_eq!(run_rev_range(&config, 0), Vec::<i32>::new());
}
