use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;

use clap::Parser;

/// Read and disassemble a shamrocq bytecode blob.
#[derive(Parser)]
#[command(name = "shamrocq-reader", version)]
struct Cli {
    /// Bytecode file to disassemble (e.g. bytecode.bin)
    file: PathBuf,

    /// Color output mode
    #[arg(long, value_enum, default_value = "auto")]
    color: ColorMode,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

struct C {
    bld: &'static str,
    dim: &'static str,
    cyn: &'static str,
    ylw: &'static str,
    grn: &'static str,
    rst: &'static str,
}

impl C {
    fn on() -> Self {
        C {
            bld: "\x1b[1m",
            dim: "\x1b[2m",
            cyn: "\x1b[36m",
            ylw: "\x1b[33m",
            grn: "\x1b[32m",
            rst: "\x1b[0m",
        }
    }
    fn off() -> Self {
        C { bld: "", dim: "", cyn: "", ylw: "", grn: "", rst: "" }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let use_color = match cli.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stdout().is_terminal(),
    };
    let c = if use_color { C::on() } else { C::off() };
    let blob = std::fs::read(&cli.file)
        .map_err(|e| format!("cannot read {}: {}", cli.file.display(), e))?;
    disassemble(&blob, &c).map_err(|e| format!("disassembly error: {}", e))?;
    Ok(())
}

use shamrocq_bytecode::op;

// ── data types ───────────────────────────────────────────────────────────────

struct Global {
    name: String,
    offset: u16,
}

struct ClosureRef {
    pc: usize,
    target: u16,
    arity: u8,
    n_captures: u8,
}

struct FrameInfo {
    n_captures: usize,
    n_params: usize,
}

struct ScanResult {
    closures: Vec<ClosureRef>,
}

// ── header parsing ────────────────────────────────────────────────────────────

fn parse_header(blob: &[u8]) -> Result<(u16, Vec<Global>, Vec<String>, usize), String> {
    if blob.len() < 8 {
        return Err("blob too short for header".to_string());
    }
    if blob[0..4] != shamrocq_bytecode::MAGIC {
        return Err("bad magic: expected SMRQ header".to_string());
    }
    let version = read_u16le(blob, 4)?;
    let mut cursor = 6usize;

    let n_globals = read_u16le(blob, cursor)? as usize;
    cursor += 2;

    let mut globals = Vec::with_capacity(n_globals);
    for _ in 0..n_globals {
        let name_len = read_u8(blob, cursor)? as usize;
        cursor += 1;
        if cursor + name_len > blob.len() {
            return Err(format!(
                "header truncated: need {} name bytes at offset {}",
                name_len, cursor
            ));
        }
        let name = std::str::from_utf8(&blob[cursor..cursor + name_len])
            .map_err(|_| format!("non-UTF-8 name at offset {}", cursor))?
            .to_owned();
        cursor += name_len;
        let offset = read_u16le(blob, cursor)?;
        cursor += 2;
        globals.push(Global { name, offset });
    }

    // Try parsing the tag table; fall back to no tags for old-format blobs.
    let (tag_names, tag_end) = try_parse_tags(blob, cursor);
    cursor = tag_end;

    Ok((version, globals, tag_names, cursor))
}

/// Attempt to parse an embedded tag table at `start`. Returns the parsed names
/// and the cursor position after the table, or `(Vec::new(), start)` if the
/// bytes don't look like a valid tag section (old-format blob).
fn try_parse_tags(blob: &[u8], start: usize) -> (Vec<String>, usize) {
    let parse = || -> Option<(Vec<String>, usize)> {
        if start + 2 > blob.len() { return None; }
        let n_tags = u16::from_le_bytes([blob[start], blob[start + 1]]) as usize;
        if n_tags > 256 { return None; }
        let mut cur = start + 2;
        let mut names = Vec::with_capacity(n_tags);
        for _ in 0..n_tags {
            if cur >= blob.len() { return None; }
            let len = blob[cur] as usize;
            cur += 1;
            if len == 0 || cur + len > blob.len() { return None; }
            let bytes = &blob[cur..cur + len];
            let name = std::str::from_utf8(bytes).ok()?;
            if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return None;
            }
            names.push(name.to_owned());
            cur += len;
        }
        Some((names, cur))
    };
    parse().unwrap_or_else(|| (Vec::new(), start))
}

// ── code scanning (pass 1) ───────────────────────────────────────────────────

fn scan_code(code: &[u8]) -> Result<ScanResult, String> {
    let mut closures = Vec::new();

    let mut pc = 0usize;
    while pc < code.len() {
        let instr_pc = pc;
        let opcode = code[pc];
        pc += 1;

        match opcode {
            op::LOAD => { pc += 1; }
            op::LOAD2 => { pc += 2; }
            op::LOAD3 => { pc += 3; }
            op::LOAD_CAPTURE => { pc += 1; }
            op::GLOBAL => { pc += 2; }
            op::DROP => { pc += 1; }
            op::SLIDE => { pc += 1; }
            op::PACK => { pc += 2; }
            op::UNPACK => { pc += 1; }
            op::BIND => { pc += 1; }
            op::FUNCTION => { pc += 3; }
            op::CLOSURE => {
                let target = u16::from_le_bytes([code[pc], code[pc + 1]]);
                let arity = code[pc + 2];
                let n_captures = code[pc + 3];
                pc += 4;
                closures.push(ClosureRef { pc: instr_pc, target, arity, n_captures });
            }
            op::FIXPOINT => { pc += 1; }
            op::CALL => {}
            op::TAIL_CALL => {}
            op::RET => {}
            op::MATCH => {
                let n_cases = code[pc] as usize;
                pc += 1;
                pc += n_cases * 4;
            }
            op::JMP => { pc += 2; }
            op::ERROR => {}
            op::INT => { pc += 4; }
            op::ADD | op::SUB | op::MUL | op::DIV | op::NEG | op::EQ | op::LT => {}
            op::BYTES => {
                let len = code[pc] as usize;
                pc += 1 + len;
            }
            op::BYTES_LEN | op::BYTES_GET | op::BYTES_EQ | op::BYTES_CONCAT => {}
            other => {
                return Err(format!(
                    "unknown opcode 0x{:02X} at code+0x{:04X}",
                    other, instr_pc
                ));
            }
        }
    }

    Ok(ScanResult { closures })
}

// ── label building ───────────────────────────────────────────────────────────

fn build_labels(
    globals: &[Global],
    scan: &ScanResult,
) -> (HashMap<u16, String>, HashMap<u16, FrameInfo>) {
    let mut labels: HashMap<u16, String> = HashMap::new();
    let mut frames: HashMap<u16, FrameInfo> = HashMap::new();

    for g in globals {
        labels.insert(g.offset, g.name.clone());
    }

    // Process closures in pc order so each parent is labeled before its children.
    let mut sorted_closures: Vec<&ClosureRef> = scan.closures.iter().collect();
    sorted_closures.sort_by_key(|c| c.pc);

    let mut child_counts: HashMap<u16, usize> = HashMap::new();

    for cl in &sorted_closures {
        if labels.contains_key(&cl.target) {
            continue;
        }
        let parent = labels
            .iter()
            .filter(|(&addr, _)| (addr as usize) <= cl.pc)
            .max_by_key(|(&addr, _)| addr);

        if let Some((&parent_addr, parent_label)) = parent {
            let n = child_counts.entry(parent_addr).or_insert(0);
            let child_label = format!("{}/{}", parent_label, n);
            *n += 1;
            frames.insert(
                cl.target,
                FrameInfo { n_captures: cl.n_captures as usize, n_params: 1 },
            );
            labels.insert(cl.target, child_label);
        }
    }

    (labels, frames)
}

// ── disassembly (pass 2) ─────────────────────────────────────────────────────

fn disassemble(blob: &[u8], c: &C) -> Result<(), String> {
    let (version, globals, tag_names, header_len) = parse_header(blob)?;
    let code = &blob[header_len..];

    println!("{}=== shamrocq bytecode ==={}", c.bld, c.rst);
    println!(
        "blob: {} bytes  header: {} bytes  code: {} bytes  version: {}  tags: {}",
        blob.len(),
        header_len,
        code.len(),
        version,
        if tag_names.is_empty() { "none".to_string() } else { format!("{} embedded", tag_names.len()) },
    );
    println!();

    println!("{}Global table{} ({} entries):", c.bld, c.rst, globals.len());
    for (i, g) in globals.iter().enumerate() {
        println!("  [{:>3}]  {:.<40}  code+0x{:04X}", i, g.name, g.offset);
    }
    println!();

    if !tag_names.is_empty() {
        println!("{}Tag table{} ({} entries):", c.bld, c.rst, tag_names.len());
        for (i, name) in tag_names.iter().enumerate() {
            println!("  [{:>3}]  {}", i, name);
        }
        println!();
    }

    let scan = scan_code(code)?;
    let (labels, frames) = build_labels(&globals, &scan);

    println!("{}=== Code ==={}", c.bld, c.rst);

    let mut sorted_labels: Vec<(u16, &str)> =
        labels.iter().map(|(k, v)| (*k, v.as_str())).collect();
    sorted_labels.sort_by_key(|(addr, _)| *addr);
    let mut next_label = 0usize;

    // Frame-tracking state for LOAD annotations
    let mut n_captures: usize = 0;
    let mut n_params: usize = 0;
    let mut bind_depth: usize = 0;
    let mut bind_restore: HashMap<u16, usize> = HashMap::new();
    // Match branch labels: target_addr -> label string
    let mut branch_labels: HashMap<u16, String> = HashMap::new();

    let mut pc = 0usize;
    while pc < code.len() {
        // Restore bind_depth at match arm boundaries
        if let Some(&saved) = bind_restore.get(&(pc as u16)) {
            bind_depth = saved;
        }

        // Print match branch label if we're at a case target
        if let Some(bl) = branch_labels.remove(&(pc as u16)) {
            println!("  {}{:04X}{}  {}{}:{}", c.dim, pc, c.rst, c.ylw, bl, c.rst);
        }

        // Print label if we're at a labeled entry point
        while next_label < sorted_labels.len()
            && sorted_labels[next_label].0 as usize == pc
        {
            let (addr, name) = sorted_labels[next_label];
            println!();
            let comment = match frames.get(&addr) {
                Some(fi) if fi.n_captures == 0 && fi.n_params == 1 => "function".to_string(),
                Some(fi) if fi.n_captures > 0 && fi.n_params == 1 => {
                    format!("{} capture{}", fi.n_captures, if fi.n_captures > 1 { "s" } else { "" })
                }
                Some(fi) if fi.n_params > 1 => format!("direct, {} args", fi.n_params),
                _ => String::new(),
            };
            if comment.is_empty() {
                println!("{}{:04X}{}  {}<{}>:{}", c.dim, pc, c.rst, c.cyn, name, c.rst);
            } else {
                println!(
                    "{}{:04X}{}  {}<{}>:{} {}; {}{}",
                    c.dim, pc, c.rst, c.cyn, name, c.rst, c.dim, comment, c.rst
                );
            }

            // Reset frame state for new body
            if let Some(fi) = frames.get(&addr) {
                n_captures = fi.n_captures;
                n_params = fi.n_params;
            } else {
                n_captures = 0;
                n_params = 0;
            }
            bind_depth = 0;
            bind_restore.clear();
            next_label += 1;
        }

        let instr_pc = pc;
        let opcode = read_u8(code, pc)?;
        pc += 1;

        // Macro for the common "  ADDR  OPCODE  rest" pattern
        macro_rules! instr {
            ($pc:expr, $op:expr) => {
                println!("  {}{:04X}{}  {}{}{}", c.dim, $pc, c.rst, c.bld, $op, c.rst)
            };
            ($pc:expr, $op:expr, $($arg:tt)+) => {
                println!("  {}{:04X}{}  {}{:<13}{}{}", c.dim, $pc, c.rst, c.bld, $op, c.rst, format_args!($($arg)+))
            };
        }

        match opcode {
            op::LOAD => {
                let idx = read_u8(code, pc)?;
                pc += 1;
                let annot = annotate_load(idx as usize, n_captures, n_params, bind_depth);
                if annot.is_empty() {
                    instr!(instr_pc, "LOAD", "{}", idx);
                } else {
                    println!(
                        "  {}{:04X}{}  {}{:<13}{}{:<17}{}; {}{}",
                        c.dim, instr_pc, c.rst,
                        c.bld, "LOAD", c.rst,
                        idx, c.dim, annot, c.rst
                    );
                }
            }
            op::LOAD2 => {
                let idx_a = read_u8(code, pc)?;
                let idx_b = read_u8(code, pc + 1)?;
                pc += 2;
                let annot_a = annotate_load(idx_a as usize, n_captures, n_params, bind_depth);
                let annot_b = annotate_load(idx_b as usize, n_captures, n_params, bind_depth);
                let annot = match (annot_a.is_empty(), annot_b.is_empty()) {
                    (true, true) => String::new(),
                    _ => format!("; {}, {}", annot_a, annot_b),
                };
                if annot.is_empty() {
                    instr!(instr_pc, "LOAD2", "{} {}", idx_a, idx_b);
                } else {
                    println!(
                        "  {}{:04X}{}  {}{:<13}{}{} {:<14}{}{}{}",
                        c.dim, instr_pc, c.rst,
                        c.bld, "LOAD2", c.rst,
                        idx_a, idx_b, c.dim, annot, c.rst
                    );
                }
            }
            op::LOAD3 => {
                let idx_a = read_u8(code, pc)?;
                let idx_b = read_u8(code, pc + 1)?;
                let idx_c = read_u8(code, pc + 2)?;
                pc += 3;
                let annot_a = annotate_load(idx_a as usize, n_captures, n_params, bind_depth);
                let annot_b = annotate_load(idx_b as usize, n_captures, n_params, bind_depth);
                let annot_c = annotate_load(idx_c as usize, n_captures, n_params, bind_depth);
                let annot = match (annot_a.is_empty(), annot_b.is_empty(), annot_c.is_empty()) {
                    (true, true, true) => String::new(),
                    _ => format!("; {}, {}, {}", annot_a, annot_b, annot_c),
                };
                if annot.is_empty() {
                    instr!(instr_pc, "LOAD3", "{} {} {}", idx_a, idx_b, idx_c);
                } else {
                    println!(
                        "  {}{:04X}{}  {}{:<13}{}{} {} {:<12}{}{}{}",
                        c.dim, instr_pc, c.rst,
                        c.bld, "LOAD3", c.rst,
                        idx_a, idx_b, idx_c, c.dim, annot, c.rst
                    );
                }
            }
            op::LOAD_CAPTURE => {
                let idx = read_u8(code, pc)?;
                pc += 1;
                println!(
                    "  {}{:04X}{}  {}{:<13}{}{:<17}{}; cap.{}{}",
                    c.dim, instr_pc, c.rst,
                    c.bld, "LOAD_CAPTURE", c.rst,
                    idx, c.dim, idx, c.rst
                );
            }
            op::GLOBAL => {
                let idx = read_u16le(code, pc)?;
                pc += 2;
                let name = globals
                    .get(idx as usize)
                    .map(|g| g.name.as_str())
                    .unwrap_or("?");
                instr!(instr_pc, "GLOBAL", "{} {}({}){}", idx, c.cyn, name, c.rst);
            }
            op::DROP => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth = bind_depth.saturating_sub(n as usize);
                instr!(instr_pc, "DROP", "{}", n);
            }
            op::SLIDE => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth = bind_depth.saturating_sub(n as usize);
                instr!(instr_pc, "SLIDE", "{}", n);
            }
            op::PACK => {
                let tag = read_u8(code, pc)?;
                let arity = read_u8(code, pc + 1)?;
                pc += 2;
                instr!(instr_pc, "PACK", "{} arity={}", fmt_tag(tag, &tag_names, c), arity);
            }
            op::UNPACK => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth += n as usize;
                instr!(instr_pc, "UNPACK", "{}", n);
            }
            op::BIND => {
                let n = read_u8(code, pc)?;
                pc += 1;
                bind_depth += n as usize;
                instr!(instr_pc, "BIND", "{}", n);
            }
            op::FUNCTION => {
                let idx = read_u16le(code, pc)?;
                let arity = read_u8(code, pc + 2)?;
                pc += 3;
                instr!(instr_pc, "FUNCTION", "idx={} arity={}", idx, arity);
            }
            op::CLOSURE => {
                let code_addr = read_u16le(code, pc)?;
                let arity = read_u8(code, pc + 2)?;
                let n_cap = read_u8(code, pc + 3)?;
                pc += 4;
                let lbl = fmt_label(code_addr, &labels, c);
                if n_cap == 0 {
                    instr!(instr_pc, "CLOSURE", "fn code+0x{:04X}{} arity={}", code_addr, lbl, arity);
                } else {
                    instr!(instr_pc, "CLOSURE", "code+0x{:04X}{} arity={} captures={}", code_addr, lbl, arity, n_cap);
                }
            }
            op::FIXPOINT => {
                let cap_idx = read_u8(code, pc)?;
                pc += 1;
                if cap_idx == 0xFF {
                    instr!(instr_pc, "FIXPOINT", "(no self-capture)");
                } else {
                    instr!(instr_pc, "FIXPOINT", "cap_idx={}", cap_idx);
                }
            }
            op::CALL => {
                instr!(instr_pc, "CALL");
            }
            op::TAIL_CALL => {
                instr!(instr_pc, "TAIL_CALL");
            }
            op::RET => {
                instr!(instr_pc, "RET");
            }
            op::MATCH => {
                let n_cases = read_u8(code, pc)? as usize;
                pc += 1;
                let saved = bind_depth;
                instr!(instr_pc, "MATCH", "{} cases:", n_cases);
                for _i in 0..n_cases {
                    let tag = read_u8(code, pc)?;
                    let arity = read_u8(code, pc + 1)?;
                    let offset = read_u16le(code, pc + 2)?;
                    pc += 4;
                    let tag_str = fmt_tag(tag, &tag_names, c);
                    let bl = branch_name(tag, &tag_names);
                    println!(
                        "        {}|{} {} arity={} -> {:04X}",
                        c.ylw, c.rst, tag_str, arity, offset
                    );
                    branch_labels.insert(offset, bl);
                    bind_restore.insert(offset, saved);
                }
            }
            op::JMP => {
                let offset = read_u16le(code, pc)?;
                pc += 2;
                let lbl = fmt_label(offset, &labels, c);
                instr!(instr_pc, "JMP", "code+0x{:04X}{}", offset, lbl);
            }
            op::ERROR => {
                instr!(instr_pc, "ERROR");
            }
            op::INT => {
                let value = read_i32le(code, pc)?;
                pc += 4;
                instr!(instr_pc, "INT", "{}", value);
            }
            op::ADD => instr!(instr_pc, "ADD"),
            op::SUB => instr!(instr_pc, "SUB"),
            op::MUL => instr!(instr_pc, "MUL"),
            op::DIV => instr!(instr_pc, "DIV"),
            op::NEG => instr!(instr_pc, "NEG"),
            op::EQ  => instr!(instr_pc, "EQ"),
            op::LT  => instr!(instr_pc, "LT"),
            op::BYTES => {
                let len = read_u8(code, pc)? as usize;
                pc += 1;
                if pc + len > code.len() {
                    return Err(format!(
                        "BYTES at {:04X}: data truncated (need {} bytes)",
                        instr_pc, len
                    ));
                }
                let data = &code[pc..pc + len];
                pc += len;
                let display = escape_bytes(data);
                instr!(instr_pc, "BYTES", "len={} {:?}", len, display);
            }
            op::BYTES_LEN    => instr!(instr_pc, "BYTES_LEN"),
            op::BYTES_GET    => instr!(instr_pc, "BYTES_GET"),
            op::BYTES_EQ     => instr!(instr_pc, "BYTES_EQ"),
            op::BYTES_CONCAT => instr!(instr_pc, "BYTES_CONCAT"),
            other => {
                return Err(format!(
                    "unknown opcode 0x{:02X} at code+0x{:04X}",
                    other, instr_pc
                ));
            }
        }
    }

    println!();
    Ok(())
}

// ── LOAD annotation ──────────────────────────────────────────────────────────

fn annotate_load(idx: usize, _n_captures: usize, n_params: usize, bind_depth: usize) -> String {
    if n_params == 0 {
        return String::new();
    }
    if idx < n_params {
        if n_params == 1 {
            return "arg".to_string();
        }
        return format!("arg.{}", idx);
    }
    let let_idx = idx - n_params;
    if let_idx < bind_depth {
        return format!("let.{}", let_idx);
    }
    String::new()
}

// ── formatting helpers ───────────────────────────────────────────────────────

fn branch_name(tag: u8, tags: &[String]) -> String {
    match tags.get(tag as usize) {
        Some(name) if !name.is_empty() => format!(".{}", name),
        _ => format!(".tag{}", tag),
    }
}

fn fmt_tag(tag: u8, tags: &[String], c: &C) -> String {
    match tags.get(tag as usize) {
        Some(name) if !name.is_empty() => format!("tag={} {}({}){}", tag, c.grn, name, c.rst),
        _ => format!("tag={}", tag),
    }
}

fn fmt_label(addr: u16, labels: &HashMap<u16, String>, c: &C) -> String {
    match labels.get(&addr) {
        Some(name) => format!(" {}<{}>{}", c.cyn, name, c.rst),
        None => String::new(),
    }
}

// ── low-level readers ────────────────────────────────────────────────────────

fn read_u8(buf: &[u8], pos: usize) -> Result<u8, String> {
    buf.get(pos)
        .copied()
        .ok_or_else(|| format!("unexpected end of blob at byte {}", pos))
}

fn read_u16le(buf: &[u8], pos: usize) -> Result<u16, String> {
    if pos + 2 > buf.len() {
        return Err(format!("unexpected end of blob reading u16 at byte {}", pos));
    }
    Ok(u16::from_le_bytes([buf[pos], buf[pos + 1]]))
}

fn read_i32le(buf: &[u8], pos: usize) -> Result<i32, String> {
    if pos + 4 > buf.len() {
        return Err(format!("unexpected end of blob reading i32 at byte {}", pos));
    }
    Ok(i32::from_le_bytes([
        buf[pos],
        buf[pos + 1],
        buf[pos + 2],
        buf[pos + 3],
    ]))
}

fn escape_bytes(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() + 2);
    for &b in data {
        if b.is_ascii_graphic() || b == b' ' {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\x{:02X}", b));
        }
    }
    out
}
