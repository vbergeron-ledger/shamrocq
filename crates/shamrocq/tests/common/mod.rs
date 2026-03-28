#![allow(dead_code)]

#[cfg(feature = "integration")]
use shamrocq::{ctors, BYTECODE};
use shamrocq::{Value, Vm};

use shamrocq_compiler::codegen::compile_program;
use shamrocq_compiler::desugar::desugar_program;
use shamrocq_compiler::parser::parse;
use shamrocq_compiler::resolve::{resolve_program, GlobalTable, TagTable};

use std::collections::HashMap;

pub struct Compiled {
    pub blob: Vec<u8>,
    pub funcs: HashMap<String, u16>,
}

pub fn compile_scheme(files: &[&str]) -> Compiled {
    let root = env!("CARGO_MANIFEST_DIR").to_string() + "/../../scheme/";
    let mut all_sexps = Vec::new();
    for file in files {
        let path = format!("{}{}", root, file);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", path, e));
        let sexps = parse(&src)
            .unwrap_or_else(|e| panic!("parse error in {}: {}", path, e));
        all_sexps.extend(sexps);
    }
    let defs = desugar_program(&all_sexps).unwrap();
    let mut tags = TagTable::new();
    let mut globals = GlobalTable::new();
    let rdefs = resolve_program(&defs, &mut tags, &mut globals).unwrap();
    let prog = compile_program(&rdefs);
    let funcs = prog.header.globals.iter().enumerate()
        .map(|(i, (name, _))| (name.clone(), i as u16))
        .collect();
    Compiled { blob: prog.serialize(), funcs }
}

#[cfg(feature = "integration")]
pub fn setup() -> (Vec<u8>, &'static [u8]) {
    let buf = vec![0u8; 65536];
    (buf, BYTECODE)
}

#[cfg(feature = "stats")]
pub fn print_stats(name: &str, vm: &Vm) {
    eprintln!("[{name}] {}", vm.mem_snapshot());
    eprintln!("{}", vm.stats);

    if let Ok(path) = std::env::var("BENCHMARK_FILE") {
        use std::io::Write;
        let commit = std::env::var("BENCHMARK_COMMIT").unwrap_or_else(|_| "unknown".into());
        let timestamp = std::env::var("BENCHMARK_TIMESTAMP").unwrap_or_else(|_| "unknown".into());
        let s = &vm.stats;
        let snap = vm.mem_snapshot();
        let line = format!(
            concat!(
                "{{\"timestamp\":\"{ts}\",\"commit\":\"{co}\",\"test\":\"{nm}\",",
                "\"peak_heap_bytes\":{ph},\"peak_stack_bytes\":{ps},",
                "\"alloc_count_tuple\":{at},\"alloc_count_closure\":{ac},\"alloc_bytes_total\":{ab},",
                "\"exec_instruction_count\":{ei},\"exec_apply_count\":{ea},",
                "\"exec_tail_apply_count\":{et},\"exec_match_count\":{em},\"exec_peak_call_depth\":{ed},",
                "\"final_heap_bytes\":{fh},\"final_stack_bytes\":{fs},\"final_free_bytes\":{ff}}}"
            ),
            ts = timestamp, co = commit, nm = name,
            ph = s.peak_heap_bytes, ps = s.peak_stack_bytes,
            at = s.alloc_count_tuple, ac = s.alloc_count_closure, ab = s.alloc_bytes_total,
            ei = s.exec_instruction_count, ea = s.exec_apply_count,
            et = s.exec_tail_apply_count, em = s.exec_match_count, ed = s.exec_peak_call_depth,
            fh = snap.heap_bytes, fs = snap.stack_bytes, ff = snap.free_bytes,
        );
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(&path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

#[cfg(not(feature = "stats"))]
pub fn print_stats(_name: &str, _vm: &Vm) {}

#[cfg(feature = "integration")]
pub fn peano(vm: &mut Vm, n: u32) -> Value {
    let mut v = Value::immediate(ctors::O);
    for _ in 0..n {
        v = vm.alloc_tuple(ctors::S, &[v]).unwrap();
    }
    v
}

#[cfg(feature = "integration")]
pub fn unpeano(vm: &Vm, mut v: Value) -> u32 {
    let mut n = 0;
    while v.tag() == ctors::S {
        v = vm.tuple_field(v, 0);
        n += 1;
    }
    n
}

#[cfg(feature = "integration")]
pub fn list_to_vec(vm: &Vm, mut v: Value) -> Vec<Value> {
    let mut out = Vec::new();
    while v.tag() == ctors::CONS {
        out.push(vm.tuple_field(v, 0));
        v = vm.tuple_field(v, 1);
    }
    out
}

#[cfg(feature = "integration")]
pub fn make_list(vm: &mut Vm, items: &[Value]) -> Value {
    let mut list = Value::immediate(ctors::NIL);
    for &item in items.iter().rev() {
        list = vm.alloc_tuple(ctors::CONS, &[item, list]).unwrap();
    }
    list
}
