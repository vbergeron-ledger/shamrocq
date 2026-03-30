use shamrocq_compiler::codegen::compile_program;
use shamrocq_compiler::desugar::desugar_program;
use shamrocq_compiler::parser::parse;
use shamrocq_compiler::resolve::{resolve_program, GlobalTable, TagTable};

use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scheme_dir = manifest_dir.join("scheme");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    // cortex-m-rt linker script needs to find memory.x
    println!("cargo::rustc-link-search={}", manifest_dir.display());
    println!("cargo::rerun-if-changed=memory.x");

    let src = std::fs::read_to_string(scheme_dir.join("demo.scm")).expect("read demo.scm");
    println!("cargo::rerun-if-changed=scheme/demo.scm");

    let sexps = parse(&src).expect("parse");
    let defs = desugar_program(&sexps).expect("desugar");
    let mut tags = TagTable::new();
    let mut globals = GlobalTable::new();
    let rdefs = resolve_program(&defs, &mut tags, &mut globals).expect("resolve");
    let prog = compile_program(&rdefs);
    let blob = prog.serialize();

    std::fs::write(out_path.join("bytecode.bin"), &blob).expect("write bytecode.bin");

    let mut funcs_src = String::new();
    for (i, (name, _offset)) in prog.header.globals.iter().enumerate() {
        funcs_src.push_str(&format!(
            "#[allow(dead_code)]\npub const {}: u16 = {};\n",
            name.to_uppercase().replace('-', "_"),
            i,
        ));
    }
    std::fs::write(out_path.join("funcs.rs"), &funcs_src).expect("write funcs.rs");

    let mut ctors_src = String::new();
    for (name, id) in tags.entries() {
        ctors_src.push_str(&format!(
            "#[allow(dead_code)]\npub const {}: u8 = {};\n",
            name.to_uppercase().replace('-', "_"),
            id,
        ));
    }
    std::fs::write(out_path.join("ctors.rs"), &ctors_src).expect("write ctors.rs");

    let mut foreign_src = String::new();
    for (name, idx) in &prog.foreign_fns {
        foreign_src.push_str(&format!(
            "#[allow(dead_code)]\npub const {}: u16 = {};\n",
            name.to_uppercase().replace('-', "_"),
            idx,
        ));
    }
    std::fs::write(out_path.join("foreign_fns.rs"), &foreign_src).expect("write foreign_fns.rs");
}
