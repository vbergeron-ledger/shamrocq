#![allow(dead_code)]

use shamrocq::{Value, Vm};

use std::collections::HashMap;

pub struct Compiled {
    pub blob: Vec<u8>,
    pub funcs: HashMap<String, u16>,
    pub tags: HashMap<String, u8>,
}

impl Compiled {
    pub fn tag(&self, name: &str) -> u8 {
        self.tags[name]
    }

    pub fn func(&self, name: &str) -> u16 {
        self.funcs[name]
    }
}

pub fn compile_scheme(files: &[&str]) -> Compiled {
    let root = env!("CARGO_MANIFEST_DIR").to_string() + "/../../scheme/";
    let sources: Vec<String> = files.iter().map(|file| {
        let path = format!("{}{}", root, file);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", path, e))
    }).collect();
    let source_refs: Vec<&str> = sources.iter().map(|s| s.as_str()).collect();
    let (prog, tags) = shamrocq_compiler::compile_sources(
        &source_refs,
        shamrocq_compiler::DEFAULT_MAX_PASS_ITERATIONS,
    ).unwrap();
    let funcs = prog.header.globals.iter().enumerate()
        .map(|(i, (name, _))| (name.clone(), i as u16))
        .collect();
    let tag_map = tags.entries().iter()
        .map(|(name, id)| (name.clone(), *id))
        .collect();
    Compiled { blob: prog.serialize(), funcs, tags: tag_map }
}

pub fn setup(files: &[&str]) -> (Compiled, Vec<u8>, Vm<'static>) {
    let c = compile_scheme(files);
    let buf = vec![0u8; 65536];
    (c, buf, unsafe { std::mem::zeroed() })
}

pub fn peano(vm: &mut Vm, tag_o: u8, tag_s: u8, n: u32) -> Value {
    let mut v = Value::ctor(tag_o, 0);
    for _ in 0..n {
        v = vm.alloc_ctor(tag_s, &[v]).unwrap();
    }
    v
}

pub fn unpeano(vm: &Vm, tag_s: u8, mut v: Value) -> u32 {
    let mut n = 0;
    while v.tag() == tag_s {
        v = vm.ctor_field(v, 0);
        n += 1;
    }
    n
}

pub fn list_to_vec(vm: &Vm, tag_cons: u8, mut v: Value) -> Vec<Value> {
    let mut out = Vec::new();
    while v.tag() == tag_cons {
        out.push(vm.ctor_field(v, 0));
        v = vm.ctor_field(v, 1);
    }
    out
}

pub fn make_list(vm: &mut Vm, tag_nil: u8, tag_cons: u8, items: &[Value]) -> Value {
    let mut list = Value::ctor(tag_nil, 0);
    for &item in items.iter().rev() {
        list = vm.alloc_ctor(tag_cons, &[item, list]).unwrap();
    }
    list
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
                "\"alloc_count_ctor\":{at},\"alloc_count_closure\":{ac},\"alloc_bytes_total\":{ab},",
                "\"exec_instruction_count\":{ei},\"exec_call_count\":{ea},",
                "\"exec_tail_call_count\":{et},",
                "\"exec_match_count\":{em},\"exec_peak_call_depth\":{ed},",
                "\"final_heap_bytes\":{fh},\"final_stack_bytes\":{fs},\"final_free_bytes\":{ff}}}"
            ),
            ts = timestamp, co = commit, nm = name,
            ph = s.peak_heap_bytes, ps = s.peak_stack_bytes,
            at = s.alloc_count_ctor, ac = s.alloc_count_closure, ab = s.alloc_bytes_total,
            ei = s.exec_instruction_count, ea = s.exec_call_count,
            et = s.exec_tail_call_count,
            em = s.exec_match_count, ed = s.exec_peak_call_depth,
            fh = snap.heap_bytes, fs = snap.stack_bytes, ff = snap.free_bytes,
        );
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(&path) {
            use std::os::unix::io::AsRawFd;
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX); }
            let _ = writeln!(file, "{line}");
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN); }
        }
    }
}

#[cfg(not(feature = "stats"))]
pub fn print_stats(_name: &str, _vm: &Vm) {}
