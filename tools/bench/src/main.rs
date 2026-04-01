//! Benchmark runner for shamrocq baremetal examples.
//!
//! Builds each example under `examples/`, runs the resulting ELF in QEMU
//! (ARM Cortex-M semihosting), captures the VM statistics printed via
//! `hprintln!`, and presents a summary table.
//!
//! # Usage
//!
//! ```text
//! cargo run -p shamrocq-bench                      # all benchmarks
//! cargo run -p shamrocq-bench -- sort rbtree       # specific benchmarks
//! cargo run -p shamrocq-bench -- --profile         # + QEMU function profiling
//! cargo run -p shamrocq-bench -- --profile sort    # profile one benchmark
//! ```
//!
//! # Profiling mode (`--profile`)
//!
//! Enables QEMU translation-block execution tracing (`-d exec,nochain`).
//! The trace is post-processed against ELF symbols (via `arm-none-eabi-objdump`)
//! to produce a per-function hotspot breakdown, showing where the CPU spends
//! time inside the compiled VM interpreter.
//!
//! # Output
//!
//! - Per-benchmark semihosting output (program results + VM stats)
//! - Summary table: peak heap, peak stack, instructions, calls, allocations,
//!   reclaimed bytes, wall-clock time
//! - With `--profile`: function-level hotspot table and binary section sizes
//! - Results are saved to `bench-results/<name>.txt`
//!
//! # Prerequisites
//!
//! - `qemu-system-arm` (QEMU ARM system emulator)
//! - `arm-none-eabi-size` and `arm-none-eabi-objdump` (for `--profile`)
//! - Each example must be a standalone Cargo project targeting `thumbv7m-none-eabi`

use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use regex::Regex;

const ALL_BENCHMARKS: &[&str] = &["sort", "rbtree", "eval", "parser"];
const TARGET_TRIPLE: &str = "thumbv7m-none-eabi";
const QEMU_BIN: &str = "qemu-system-arm";
const QEMU_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    let args = Args::parse();
    let root = find_workspace_root();
    let results_dir = root.join("bench-results");
    fs::create_dir_all(&results_dir).expect("cannot create bench-results/");

    let benchmarks: Vec<&str> = if args.filter.is_empty() {
        ALL_BENCHMARKS.to_vec()
    } else {
        args.filter.iter().map(|s| s.as_str()).collect()
    };

    println!("{}", Style::bold("=== shamrocq benchmarks ==="));
    if args.profile {
        println!("{}", Style::dim("  profiling mode (QEMU exec trace enabled)"));
    }
    println!();

    let mut results = Vec::new();
    for name in &benchmarks {
        match run_benchmark(&root, name, args.profile, &results_dir) {
            Ok(r) => results.push(r),
            Err(e) => eprintln!("{} {e}", Style::red(&format!("[{name}]"))),
        }
    }

    if args.profile {
        println!("{}", Style::bold("=== profiles ==="));
        println!();
        for r in &results {
            if let Some(trace) = &r.trace_file {
                print_profile(r.name, &r.elf, trace);
            }
        }
    }

    print_summary(&results);
    println!(
        "{}",
        Style::dim(&format!("Results saved to {}/", results_dir.display()))
    );
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct Args {
    profile: bool,
    filter: Vec<String>,
}

impl Args {
    fn parse() -> Self {
        let mut profile = false;
        let mut filter = Vec::new();
        for arg in env::args().skip(1) {
            match arg.as_str() {
                "--profile" | "-p" => profile = true,
                "--help" | "-h" => {
                    eprintln!("usage: bench [--profile] [name ...]");
                    std::process::exit(0);
                }
                _ if arg.starts_with('-') => {
                    eprintln!("unknown flag: {arg}");
                    std::process::exit(1);
                }
                _ => filter.push(arg),
            }
        }
        Args { profile, filter }
    }
}

// ---------------------------------------------------------------------------
// Benchmark runner
// ---------------------------------------------------------------------------

struct BenchResult {
    name: &'static str,
    elf: PathBuf,
    stats: Option<VmStats>,
    wall_time: Duration,
    trace_file: Option<PathBuf>,
}

fn run_benchmark(
    root: &Path,
    name: &str,
    profile: bool,
    results_dir: &Path,
) -> Result<BenchResult, String> {
    let example_dir = root.join("examples").join(name);
    if !example_dir.is_dir() {
        return Err(format!("examples/{name} not found"));
    }

    // -- build --
    eprint!("{} building... ", Style::bold(&format!("[{name}]")));
    let build = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&example_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("cargo build: {e}"))?;

    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr);
        let errors: Vec<&str> = stderr.lines().filter(|l| l.contains("error")).collect();
        return Err(format!("build failed:\n{}", errors.join("\n")));
    }

    // -- locate ELF --
    let elf = find_elf(&example_dir)?;

    // -- run in QEMU --
    eprint!("running... ");
    let trace_file = if profile {
        Some(results_dir.join(format!("{name}.trace")))
    } else {
        None
    };

    let start = Instant::now();
    let output = run_qemu(&elf, trace_file.as_deref())?;
    let wall_time = start.elapsed();

    // -- parse stats from output --
    let stats = VmStats::parse(&output);

    // -- save output --
    let out_path = results_dir.join(format!("{name}.txt"));
    fs::write(&out_path, &output).ok();

    eprintln!(
        "{} ({:.1}s) -> {}",
        Style::green("done"),
        wall_time.as_secs_f64(),
        out_path.display()
    );
    println!("{output}");

    let name: &'static str = ALL_BENCHMARKS
        .iter()
        .find(|&&b| b == name)
        .copied()
        .unwrap_or_else(|| Box::leak(name.to_string().into_boxed_str()));

    Ok(BenchResult {
        name,
        elf,
        stats,
        wall_time,
        trace_file,
    })
}

fn find_elf(example_dir: &Path) -> Result<PathBuf, String> {
    let release_dir = example_dir
        .join("target")
        .join(TARGET_TRIPLE)
        .join("release");
    let entries: Vec<_> = fs::read_dir(&release_dir)
        .map_err(|e| format!("cannot read {}: {e}", release_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let ft = e.file_type().ok();
            let name = e.file_name();
            let name = name.to_string_lossy();
            ft.map_or(false, |ft| ft.is_file())
                && !name.ends_with(".d")
                && !name.ends_with(".o")
                && !name.contains('.')
        })
        .collect();

    entries
        .into_iter()
        .next()
        .map(|e| e.path())
        .ok_or_else(|| format!("no ELF found in {}", release_dir.display()))
}

fn run_qemu(elf: &Path, trace_file: Option<&Path>) -> Result<String, String> {
    let mut cmd = Command::new(QEMU_BIN);
    cmd.args([
        "-machine",
        "lm3s6965evb",
        "-semihosting-config",
        "enable=on",
        "-nographic",
        "-kernel",
    ]);
    cmd.arg(elf);

    if let Some(trace) = trace_file {
        cmd.args(["-d", "exec,nochain", "-D"]);
        cmd.arg(trace);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("{QEMU_BIN}: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut lines = Vec::new();
        for line in reader.lines() {
            match line {
                Ok(l) => lines.push(l),
                Err(_) => break,
            }
        }
        let _ = tx.send(lines);
    });

    match rx.recv_timeout(QEMU_TIMEOUT) {
        Ok(lines) => {
            let _ = child.wait();
            handle.join().ok();
            let filtered: Vec<&str> = lines
                .iter()
                .map(|s| s.as_str())
                .filter(|l| !l.contains("Timer with period zero"))
                .collect();
            Ok(filtered.join("\n"))
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err("QEMU timeout".into())
        }
    }
}

// ---------------------------------------------------------------------------
// VM stats parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct VmStats {
    peak_heap: u64,
    peak_stack: u64,
    alloc_ctors: u64,
    alloc_closures: u64,
    alloc_bytes: u64,
    instructions: u64,
    calls: u64,
    tail_calls: u64,
    matches: u64,
    peak_depth: u64,
    reclaim_count: u64,
    reclaim_bytes: u64,
}

impl VmStats {
    fn parse(output: &str) -> Option<Self> {
        let mut s = VmStats::default();
        let mut found = false;
        for line in output.lines() {
            let line = line.trim();
            if let Some(v) = Self::extract(line, "peak heap") {
                s.peak_heap = v;
                found = true;
            } else if let Some(v) = Self::extract(line, "peak stack") {
                s.peak_stack = v;
            } else if let Some(v) = Self::extract_bare(line, "ctors") {
                s.alloc_ctors = v;
            } else if let Some(v) = Self::extract_bare(line, "closures") {
                s.alloc_closures = v;
            } else if let Some(v) = Self::extract(line, "total bytes") {
                if s.alloc_bytes == 0 {
                    s.alloc_bytes = v;
                } else {
                    s.reclaim_bytes = v;
                }
            } else if let Some(v) = Self::extract_bare(line, "instructions") {
                s.instructions = v;
            } else if line.starts_with("calls") {
                if let Some(v) = Self::extract_bare(line, "calls") {
                    s.calls = v;
                }
            } else if let Some(v) = Self::extract_bare(line, "tail calls") {
                s.tail_calls = v;
            } else if let Some(v) = Self::extract_bare(line, "matches") {
                s.matches = v;
            } else if let Some(v) = Self::extract_bare(line, "peak depth") {
                s.peak_depth = v;
            } else if let Some(v) = Self::extract_bare(line, "count") {
                s.reclaim_count = v;
            } else if let Some(v) = Self::extract(line, "bytes total") {
                s.reclaim_bytes = v;
            }
        }
        found.then_some(s)
    }

    fn extract(line: &str, key: &str) -> Option<u64> {
        if !line.contains(key) {
            return None;
        }
        line.split_whitespace()
            .rev()
            .find_map(|w| w.parse::<u64>().ok().or_else(|| {
                if w == "B" {
                    line.split_whitespace().rev().nth(1)?.parse().ok()
                } else {
                    None
                }
            }))
    }

    fn extract_bare(line: &str, key: &str) -> Option<u64> {
        if !line.contains(key) {
            return None;
        }
        line.split_whitespace()
            .rev()
            .find_map(|w| w.parse::<u64>().ok())
    }
}

// ---------------------------------------------------------------------------
// Binary sections
// ---------------------------------------------------------------------------

struct Section {
    name: String,
    size: u64,
}

fn parse_sections(elf: &Path) -> Vec<Section> {
    let output = Command::new("arm-none-eabi-size")
        .args(["-A"])
        .arg(elf)
        .output()
        .ok();

    let output = match output {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return Vec::new(),
    };

    let interesting = [".text", ".rodata", ".data", ".bss"];
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && interesting.contains(&parts[0]) {
                Some(Section {
                    name: parts[0].to_string(),
                    size: parts[1].parse().unwrap_or(0),
                })
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Profiling (QEMU exec trace → function hotspots)
// ---------------------------------------------------------------------------

fn print_profile(name: &str, elf: &Path, trace_file: &Path) {
    let func_map = parse_elf_symbols(elf);
    if func_map.is_empty() {
        eprintln!("  (no symbols from {elf:?})");
        return;
    }

    let counts = parse_trace(trace_file);
    if counts.is_empty() {
        eprintln!("  (empty trace)");
        return;
    }

    let mut by_func: HashMap<&str, u64> = HashMap::new();
    for (&pc, &cnt) in &counts {
        let fname = lookup_function(&func_map, pc);
        *by_func.entry(fname).or_default() += cnt;
    }

    let total: u64 = by_func.values().sum();
    let mut sorted: Vec<_> = by_func.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    println!("{}", Style::bold(&format!("[{name}] function profile")));
    println!("  Total TB executions: {total}");
    println!();
    println!("  {:<55} {:>8} {:>6}", "function", "count", "%");
    println!("  {:-<55} {:-<8} {:-<6}", "", "", "");

    for (func, cnt) in sorted.iter().take(10) {
        let short = shorten_symbol(func);
        let pct = *cnt as f64 / total as f64 * 100.0;
        println!("  {:<55} {:>8} {:>5.1}%", short, cnt, pct);
    }

    let sections = parse_sections(elf);
    if !sections.is_empty() {
        println!();
        println!("  {}", Style::dim("binary sections:"));
        for s in &sections {
            println!("    {:<12} {:>6} B", s.name, s.size);
        }
    }
    println!();
}

fn parse_elf_symbols(elf: &Path) -> Vec<(u64, String)> {
    let output = Command::new("arm-none-eabi-objdump")
        .args(["-d", "--demangle"])
        .arg(elf)
        .output()
        .ok();

    let output = match output {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return Vec::new(),
    };

    let re = Regex::new(r"^([0-9a-f]+)\s+<(.+?)>:").unwrap();
    let mut syms: Vec<(u64, String)> = output
        .lines()
        .filter_map(|line| {
            let caps = re.captures(line)?;
            let addr = u64::from_str_radix(&caps[1], 16).ok()?;
            Some((addr, caps[2].to_string()))
        })
        .collect();
    syms.sort_by_key(|&(a, _)| a);
    syms
}

fn parse_trace(trace_file: &Path) -> HashMap<u64, u64> {
    let re = Regex::new(r"/0{8}([0-9a-f]+)/").unwrap();
    let mut counts: HashMap<u64, u64> = HashMap::new();

    let file = match fs::File::open(trace_file) {
        Ok(f) => f,
        Err(_) => return counts,
    };

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if let Some(caps) = re.captures(&line) {
            if let Ok(pc) = u64::from_str_radix(&caps[1], 16) {
                *counts.entry(pc).or_default() += 1;
            }
        }
    }
    counts
}

fn lookup_function<'a>(syms: &'a [(u64, String)], pc: u64) -> &'a str {
    match syms.binary_search_by_key(&pc, |&(a, _)| a) {
        Ok(i) => &syms[i].1,
        Err(0) => "???",
        Err(i) => &syms[i - 1].1,
    }
}

fn shorten_symbol(s: &str) -> String {
    let parts: Vec<&str> = s.split("::").collect();
    let short = if parts.len() > 2 {
        parts[parts.len() - 2..].join("::")
    } else {
        s.to_string()
    };
    if short.len() > 55 {
        format!("{}...", &short[..52])
    } else {
        short
    }
}

// ---------------------------------------------------------------------------
// Summary table
// ---------------------------------------------------------------------------

fn print_summary(results: &[BenchResult]) {
    println!("{}", Style::bold("=== summary ==="));
    println!(
        "{:<8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>7}",
        "bench", "heap", "stack", "instrs", "calls", "allocs", "reclaim", "time"
    );
    println!(
        "{:<8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>7}",
        "--------", "--------", "--------", "--------", "--------", "--------", "--------",
        "-------"
    );

    for r in results {
        if let Some(s) = &r.stats {
            println!(
                "{:<8} {:>7}B {:>7}B {:>8} {:>8} {:>7}B {:>7}B {:>5.1}s",
                r.name,
                s.peak_heap,
                s.peak_stack,
                s.instructions,
                s.calls,
                s.alloc_bytes,
                s.reclaim_bytes,
                r.wall_time.as_secs_f64(),
            );
        } else {
            println!(
                "{:<8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>5.1}s",
                r.name, "?", "?", "?", "?", "?", "?",
                r.wall_time.as_secs_f64(),
            );
        }
    }
    println!();
}

// ---------------------------------------------------------------------------
// Terminal styling
// ---------------------------------------------------------------------------

struct Style;

impl Style {
    fn bold(s: &str) -> StyledStr {
        StyledStr("\x1b[1m", s, "\x1b[0m")
    }
    fn dim(s: &str) -> StyledStr {
        StyledStr("\x1b[2m", s, "\x1b[0m")
    }
    fn green(s: &str) -> StyledStr {
        StyledStr("\x1b[32m", s, "\x1b[0m")
    }
    fn red(s: &str) -> StyledStr {
        StyledStr("\x1b[31m", s, "\x1b[0m")
    }
}

struct StyledStr<'a>(&'a str, &'a str, &'a str);

impl fmt::Display for StyledStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if atty_stdout() {
            write!(f, "{}{}{}", self.0, self.1, self.2)
        } else {
            write!(f, "{}", self.1)
        }
    }
}

fn atty_stdout() -> bool {
    unsafe { libc_isatty(1) != 0 }
}

extern "C" {
    #[link_name = "isatty"]
    fn libc_isatty(fd: i32) -> i32;
}

// ---------------------------------------------------------------------------
// Workspace discovery
// ---------------------------------------------------------------------------

fn find_workspace_root() -> PathBuf {
    let mut dir = env::current_dir().expect("cannot get cwd");
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("crates").is_dir() {
            return dir;
        }
        if !dir.pop() {
            eprintln!("error: cannot find workspace root (looked for Cargo.toml + crates/)");
            std::process::exit(1);
        }
    }
}
