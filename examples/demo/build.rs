use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = std::env::var("OUT_DIR").unwrap();

    println!("cargo::rustc-link-search={}", manifest_dir.display());
    println!("cargo::rerun-if-changed=memory.x");
    println!("cargo::rerun-if-changed=scheme/demo.scm");

    let src = std::fs::read_to_string(manifest_dir.join("scheme/demo.scm")).expect("read demo.scm");
    shamrocq_compiler::compile_to_dir(
        &[&src],
        shamrocq_compiler::DEFAULT_MAX_PASS_ITERATIONS,
        Path::new(&out_dir),
    ).expect("compile");
}
