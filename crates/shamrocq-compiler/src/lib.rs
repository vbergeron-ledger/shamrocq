pub mod bytecode;
pub mod codegen;
pub mod desugar;
pub mod parser;
pub mod pass;
pub mod resolve;

use codegen::CompiledProgram;
use resolve::TagTable;

pub const DEFAULT_MAX_PASS_ITERATIONS: usize = 1024;

pub fn compile_sources(
    sources: &[&str],
    max_pass_iterations: usize,
) -> Result<(CompiledProgram, TagTable), String> {
    let mut all_sexps = Vec::new();
    for src in sources {
        all_sexps.extend(parser::parse(src).map_err(|e| e.to_string())?);
    }
    let mut defs = desugar::desugar_program(&all_sexps)?;

    for _ in 0..max_pass_iterations {
        let prev = defs.clone();
        for p in pass::expr_passes() {
            defs = p.run(defs);
        }
        if defs == prev {
            break;
        }
    }

    let mut tags = TagTable::new();
    let mut globals = resolve::GlobalTable::new();
    let mut rdefs = resolve::resolve_program(&defs, &mut tags, &mut globals)?;

    for _ in 0..max_pass_iterations {
        let prev = rdefs.clone();
        for p in pass::resolved_passes() {
            rdefs = p.run(rdefs);
        }
        if rdefs == prev {
            break;
        }
    }

    Ok((codegen::compile_program(&rdefs), tags))
}

pub fn compile_to_dir(
    sources: &[&str],
    max_pass_iterations: usize,
    dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let (prog, tags) = compile_sources(sources, max_pass_iterations)?;
    prog.emit_artifacts(&tags, dir)?;
    Ok(())
}
