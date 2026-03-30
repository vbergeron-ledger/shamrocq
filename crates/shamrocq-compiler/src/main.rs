use std::path::PathBuf;

use clap::Parser;

/// Compile Scheme files into shamrocq bytecode for bare-metal execution.
#[derive(Parser)]
#[command(name = "shamrocq-compiler", version)]
struct Cli {
    /// Output directory for generated files
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Embed constructor tag names in the bytecode blob
    #[arg(long)]
    embed_tags: bool,

    /// Maximum number of optimization pass iterations (0 = no optimization)
    #[arg(long, default_value_t = shamrocq_compiler::DEFAULT_MAX_PASS_ITERATIONS)]
    max_pass_iterations: usize,

    /// Scheme source files to compile
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let sources: Vec<String> = cli
        .files
        .iter()
        .map(|p| {
            std::fs::read_to_string(p)
                .map_err(|e| format!("cannot read {}: {}", p.display(), e))
        })
        .collect::<Result<_, _>>()?;
    let refs: Vec<&str> = sources.iter().map(|s| s.as_str()).collect();

    let (mut prog, tags) = shamrocq_compiler::compile_sources(&refs, cli.max_pass_iterations)?;
    if cli.embed_tags {
        prog.header.tags = tags.entries().into_iter().map(|(name, _)| name).collect();
    }

    prog.emit_artifacts(&tags, &cli.output)?;

    eprintln!(
        "compiled {} globals, {} ctors, {} foreign fns, {} bytes of bytecode from {} files",
        prog.header.n_globals,
        tags.entries().len(),
        prog.foreign_fns.len(),
        prog.serialize().len(),
        cli.files.len(),
    );

    Ok(())
}
