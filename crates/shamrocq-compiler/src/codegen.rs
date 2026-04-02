use crate::bytecode::{Emitter, ProgramHeader};
use crate::desugar::PrimOp;
use crate::pass::p07_arity_analysis::lambda_arity;
use crate::resolve::{RDefine, RExpr, RMatchCase};

pub struct CompiledProgram {
    pub header: ProgramHeader,
    pub code: Vec<u8>,
    /// Foreign function declarations: (name, registration index).
    /// The host must call `vm.register_foreign(idx, fn)` for each entry
    /// before loading the program.
    pub foreign_fns: Vec<(String, u16)>,
}

impl CompiledProgram {
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.header.serialize(&mut out);
        out.extend_from_slice(&self.code);
        out
    }

    pub fn header_len(&self) -> usize {
        self.header.serialized_len()
    }

    pub fn emit_artifacts(
        &self,
        tags: &crate::resolve::TagTable,
        dir: &std::path::Path,
    ) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("bytecode.bin"), self.serialize())?;

        let mut s = String::new();

        s.push_str("pub mod funcs {\n");
        for (i, (name, _)) in self.header.globals.iter().enumerate() {
            s.push_str(&format!(
                "    #[allow(dead_code)]\n    pub const {}: u16 = {};\n",
                rust_const_name(name), i,
            ));
        }
        s.push_str("}\n\n");

        s.push_str("pub mod ctors {\n");
        for (name, id) in tags.entries() {
            s.push_str(&format!(
                "    #[allow(dead_code)]\n    pub const {}: u8 = {};\n",
                rust_const_name(&name), id,
            ));
        }
        s.push_str("}\n\n");

        s.push_str("pub mod foreign {\n");
        for (name, idx) in &self.foreign_fns {
            s.push_str(&format!(
                "    #[allow(dead_code)]\n    pub const {}: u16 = {};\n",
                rust_const_name(name), idx,
            ));
        }
        s.push_str("}\n");

        std::fs::write(dir.join("bindings.rs"), &s)?;

        Ok(())
    }
}

fn rust_const_name(name: &str) -> String {
    name.to_uppercase().replace('-', "_")
}

struct DeferredLambda {
    body: RExpr,
    captures: Vec<u8>,
    closure_code_addr_pos: usize,
    arity: u8,
}

struct Compiler {
    emitter: Emitter,
    deferred: Vec<DeferredLambda>,
    last_closure_captures: Option<Vec<u8>>,
    global_arities: Vec<u8>,
    flat_patches: Vec<(usize, u16)>,
}

/// Compile-time context tracking how de Bruijn indices map to LOAD slots.
///
/// Frame layout (bottom to top, on the operand stack):
///   [cap_0, ..., cap_{N-1}, param_0, ..., param_{M-1}, let_0, ...]
///
/// Captures occupy the first N slots, parameters follow, then let-bindings.
/// Everything is accessed via LOAD — no separate LOAD_CAPTURE instruction.
#[derive(Clone)]
struct Ctx {
    captures: Vec<u8>,
    frame_depth: usize,
}

impl Ctx {
    fn toplevel() -> Self {
        Ctx {
            captures: Vec::new(),
            frame_depth: 0,
        }
    }

    fn for_closure(captures: Vec<u8>, arity: usize) -> Self {
        Ctx {
            captures,
            frame_depth: arity,
        }
    }

    fn load_target(&self, de_bruijn: u8) -> u8 {
        let idx = de_bruijn as usize;
        if idx < self.frame_depth {
            (self.captures.len() + self.frame_depth - 1 - idx) as u8
        } else {
            let parent_idx = (idx - self.frame_depth) as u8;
            self.captures
                .iter()
                .position(|&c| c == parent_idx)
                .unwrap_or_else(|| {
                    panic!(
                        "BUG: free var (parent de Bruijn {}) not in captures {:?}",
                        parent_idx, self.captures
                    )
                }) as u8
        }
    }

    fn push_bindings(&mut self, n: usize) {
        self.frame_depth += n;
    }

    fn pop_bindings(&mut self, n: usize) {
        self.frame_depth -= n;
    }
}

pub fn compile_program(defs: &[RDefine]) -> CompiledProgram {
    let global_arities: Vec<u8> = defs.iter().map(|d| lambda_arity(&d.body)).collect();

    let mut c = Compiler {
        emitter: Emitter::new(),
        deferred: Vec::new(),
        last_closure_captures: None,
        global_arities,
        flat_patches: Vec::new(),
    };

    let mut global_offsets: Vec<(String, u16)> = Vec::new();

    for def in defs.iter() {
        let offset = c.emitter.pos() as u16;
        global_offsets.push((def.name.clone(), offset));
        let mut ctx = Ctx::toplevel();
        c.compile_expr(&def.body, &mut ctx, true);
    }

    c.emit_deferred();

    let flat_addrs = c.emit_flat_bodies(defs);
    c.emit_deferred();

    for &(pos, global_idx) in &c.flat_patches {
        if let Some(addr) = flat_addrs[global_idx as usize] {
            c.emitter.patch_u16(pos, addr);
        }
    }

    let foreign_fns: Vec<(String, u16)> = defs
        .iter()
        .filter_map(|d| {
            if let RExpr::Foreign(idx) = d.body {
                Some((d.name.clone(), idx))
            } else {
                None
            }
        })
        .collect();

    CompiledProgram {
        header: ProgramHeader {
            n_globals: global_offsets.len() as u16,
            globals: global_offsets,
            tags: Vec::new(),
        },
        code: c.emitter.code,
        foreign_fns,
    }
}

impl Compiler {
    fn compile_expr(&mut self, expr: &RExpr, ctx: &mut Ctx, tail: bool) {
        match expr {
            RExpr::Local(idx) => {
                self.emitter.emit_load(ctx.load_target(*idx));
                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::Global(idx) => {
                self.emitter.emit_global(*idx);
                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::Ctor(tag, fields) => {
                for f in fields {
                    self.compile_expr(f, ctx, false);
                }
                self.emitter.emit_pack(*tag, fields.len() as u8);
                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::Lambda(_) | RExpr::Lambdas(_, _) => {
                let (mut arity, mut inner) = match expr {
                    RExpr::Lambdas(n, body) => (*n, body.as_ref()),
                    RExpr::Lambda(body) => (1u8, body.as_ref()),
                    _ => unreachable!(),
                };
                loop {
                    match inner {
                        RExpr::Lambda(next) => { arity += 1; inner = next; }
                        RExpr::Lambdas(n, next) => { arity += n; inner = next; }
                        _ => break,
                    }
                }

                let mut free = Vec::new();
                collect_free(inner, arity as usize, &mut free);
                free.sort();
                free.dedup();

                self.emit_captures(&free, ctx);

                let n_captures = free.len() as u8;
                let total_arity = arity + n_captures;
                self.emitter.emit_closure(0, total_arity, n_captures);
                let code_addr_pos = self.emitter.pos() - if n_captures == 0 { 3 } else { 4 };

                self.last_closure_captures = Some(free.clone());

                self.deferred.push(DeferredLambda {
                    body: inner.clone(),
                    captures: free,
                    closure_code_addr_pos: code_addr_pos,
                    arity,
                });

                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::App(func, arg) => {
                if !self.try_compile_flat_call(expr, ctx, tail) {
                    self.compile_expr(func, ctx, false);
                    self.compile_expr(arg, ctx, false);
                    if tail {
                        self.emitter.emit_tail_call1();
                    } else {
                        self.emitter.emit_call1();
                    }
                }
            }

            RExpr::AppN(func, args) => {
                if !self.try_compile_flat_call(expr, ctx, tail) {
                    self.compile_expr(func, ctx, false);
                    for (i, arg) in args.iter().enumerate() {
                        self.compile_expr(arg, ctx, false);
                        if i == args.len() - 1 && tail {
                            self.emitter.emit_tail_call1();
                        } else {
                            self.emitter.emit_call1();
                        }
                    }
                }
            }

            RExpr::Let(val, body) => {
                self.compile_expr(val, ctx, false);
                ctx.push_bindings(1);
                self.compile_expr(body, ctx, tail);
                ctx.pop_bindings(1);
                if !tail {
                    self.emitter.emit_slide(1);
                }
            }

            RExpr::Letrec(val, body) => {
                // Push a dummy value for the letrec binding slot.
                self.emitter.emit_pack(0, 0);
                ctx.push_bindings(1);

                // Compile val (expected to be a Lambda that captures itself).
                self.last_closure_captures = None;
                self.compile_expr(val, ctx, false);

                // The closure is now on the stack above the dummy.
                // FIXPOINT patches the self-reference and replaces the dummy.
                let fix_slot = self
                    .last_closure_captures
                    .as_ref()
                    .and_then(|caps| caps.iter().position(|&x| x == 0))
                    .map(|s| s as u8)
                    .unwrap_or(0xFF);
                self.emitter.emit_fixpoint(fix_slot);

                self.compile_expr(body, ctx, tail);
                ctx.pop_bindings(1);
                if !tail {
                    self.emitter.emit_slide(1);
                }
            }

            RExpr::Match(scrutinee, cases) => {
                self.compile_expr(scrutinee, ctx, false);
                self.compile_match(cases, ctx, tail);
            }

            RExpr::Int(n) => {
                self.emitter.emit_int(*n);
                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::Bytes(data) => {
                self.emitter.emit_bytes(data);
                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::PrimOp(op, args) => {
                for arg in args {
                    self.compile_expr(arg, ctx, false);
                }
                match op {
                    PrimOp::Add => self.emitter.emit_add(),
                    PrimOp::Sub => self.emitter.emit_sub(),
                    PrimOp::Mul => self.emitter.emit_mul(),
                    PrimOp::Div => self.emitter.emit_div(),
                    PrimOp::Neg => self.emitter.emit_neg(),
                    PrimOp::Eq  => self.emitter.emit_eq(),
                    PrimOp::Lt  => self.emitter.emit_lt(),
                    PrimOp::BytesLen  => self.emitter.emit_bytes_len(),
                    PrimOp::BytesGet  => self.emitter.emit_bytes_get(),
                    PrimOp::BytesEq   => self.emitter.emit_bytes_eq(),
                    PrimOp::BytesCat  => self.emitter.emit_bytes_concat(),
                }
                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::Error => {
                self.emitter.emit_error();
            }

            RExpr::Foreign(idx) => {
                self.emitter.emit_foreign(*idx, 1);
                if tail {
                    self.emitter.emit_ret();
                }
            }
        }
    }

    fn emit_captures(&mut self, free: &[u8], ctx: &Ctx) {
        for &parent_idx in free {
            self.emitter.emit_load(ctx.load_target(parent_idx));
        }
    }

    fn compile_match(&mut self, cases: &[RMatchCase], ctx: &mut Ctx, tail: bool) {
        if cases.len() == 1 {
            let case = &cases[0];
            if case.arity > 0 {
                self.emitter.emit_unpack(case.arity);
                ctx.push_bindings(case.arity as usize);
                self.compile_expr(&case.body, ctx, tail);
                ctx.pop_bindings(case.arity as usize);
                if !tail {
                    self.emitter.emit_slide(case.arity);
                }
            } else {
                self.emitter.emit_drop(1);
                self.compile_expr(&case.body, ctx, tail);
            }
            return;
        }

        let base_tag = cases.iter().map(|c| c.tag).min().unwrap();
        let max_tag = cases.iter().map(|c| c.tag).max().unwrap();
        let n_entries = max_tag - base_tag + 1;
        let has_gaps = (n_entries as usize) > cases.len();

        let table_start = self.emitter.emit_match_header(base_tag, n_entries);

        let mut jmp_patches = Vec::new();

        for (i, case) in cases.iter().enumerate() {
            let case_offset = self.emitter.pos() as u16;
            let slot = (case.tag - base_tag) as usize;
            self.emitter
                .patch_match_entry(table_start, slot, case.arity, case_offset);

            if case.arity > 0 {
                self.emitter.emit_bind(case.arity);
                ctx.push_bindings(case.arity as usize);
            }

            self.compile_expr(&case.body, ctx, tail);

            if case.arity > 0 {
                ctx.pop_bindings(case.arity as usize);
                if !tail {
                    self.emitter.emit_slide(case.arity);
                }
            }

            if !tail && i < cases.len() - 1 {
                let jmp_pos = self.emitter.emit_jmp_placeholder();
                jmp_patches.push(jmp_pos);
            }
        }

        if has_gaps {
            let error_pos = self.emitter.pos() as u16;
            self.emitter.emit_error();
            for slot in 0..n_entries as usize {
                if self.emitter.match_entry_is_sentinel(table_start, slot) {
                    self.emitter.patch_match_entry(table_start, slot, 0, error_pos);
                }
            }
        }

        let end_pos = self.emitter.pos() as u16;
        for jmp_pos in jmp_patches {
            self.emitter.patch_u16(jmp_pos, end_pos);
        }
    }

    fn emit_deferred(&mut self) {
        while !self.deferred.is_empty() {
            let batch = core::mem::take(&mut self.deferred);
            for dl in batch {
                let body_addr = self.emitter.pos() as u16;
                self.emitter.patch_u16(dl.closure_code_addr_pos, body_addr);

                let mut ctx = Ctx::for_closure(dl.captures, dl.arity as usize);
                self.compile_expr(&dl.body, &mut ctx, true);
            }
        }
    }

    fn try_compile_flat_call(&mut self, expr: &RExpr, ctx: &mut Ctx, tail: bool) -> bool {
        let (global_idx, args) = match try_flatten_app(expr) {
            Some(x) => x,
            None => return false,
        };
        let arity = match self.global_arities.get(global_idx as usize) {
            Some(&a) if a >= 2 && args.len() == a as usize => a,
            _ => return false,
        };
        for a in &args {
            self.compile_expr(a, ctx, false);
        }
        if tail {
            let pos = self.emitter.emit_tail_call_n_placeholder(arity);
            self.flat_patches.push((pos, global_idx));
        } else {
            let pos = self.emitter.emit_call_n_placeholder(arity);
            self.flat_patches.push((pos, global_idx));
        }
        true
    }

    fn emit_flat_bodies(&mut self, defs: &[RDefine]) -> Vec<Option<u16>> {
        let mut flat_addrs: Vec<Option<u16>> = vec![None; defs.len()];
        for (i, def) in defs.iter().enumerate() {
            let arity = self.global_arities[i];
            if arity < 2 {
                continue;
            }
            let mut inner = &def.body;
            let mut peeled = 0u8;
            while peeled < arity {
                match inner {
                    RExpr::Lambda(body) => { peeled += 1; inner = body; }
                    RExpr::Lambdas(n, body) => { peeled += n; inner = body; }
                    _ => break,
                }
            }
            let addr = self.emitter.pos() as u16;
            flat_addrs[i] = Some(addr);
            let mut ctx = Ctx::for_closure(vec![], arity as usize);
            self.compile_expr(inner, &mut ctx, true);
        }
        flat_addrs
    }
}

fn try_flatten_app<'a>(expr: &'a RExpr) -> Option<(u16, Vec<&'a RExpr>)> {
    if let RExpr::AppN(func, args) = expr {
        if let RExpr::Global(idx) = func.as_ref() {
            return Some((*idx, args.iter().collect()));
        }
        return None;
    }
    let mut args = Vec::new();
    let mut cur = expr;
    while let RExpr::App(func, arg) = cur {
        args.push(arg.as_ref());
        cur = func;
    }
    if let RExpr::Global(idx) = cur {
        args.reverse();
        Some((*idx, args))
    } else {
        None
    }
}

/// Collect de Bruijn indices that are free in `expr` (>= `bound`),
/// normalized to the enclosing scope (shifted down by `bound`).
fn collect_free(expr: &RExpr, bound: usize, free: &mut Vec<u8>) {
    match expr {
        RExpr::Local(idx) => {
            let idx = *idx as usize;
            if idx >= bound {
                free.push((idx - bound) as u8);
            }
        }
        RExpr::Global(_) | RExpr::Int(_) | RExpr::Bytes(_) | RExpr::Error | RExpr::Foreign(_) => {}
        RExpr::Ctor(_, fields) => {
            for f in fields {
                collect_free(f, bound, free);
            }
        }
        RExpr::PrimOp(_, args) => {
            for a in args {
                collect_free(a, bound, free);
            }
        }
        RExpr::Lambda(body) => {
            collect_free(body, bound + 1, free);
        }
        RExpr::Lambdas(n, body) => {
            collect_free(body, bound + *n as usize, free);
        }
        RExpr::App(f, a) => {
            collect_free(f, bound, free);
            collect_free(a, bound, free);
        }
        RExpr::AppN(f, args) => {
            collect_free(f, bound, free);
            for a in args {
                collect_free(a, bound, free);
            }
        }
        RExpr::Let(val, body) => {
            collect_free(val, bound, free);
            collect_free(body, bound + 1, free);
        }
        RExpr::Letrec(val, body) => {
            collect_free(val, bound + 1, free);
            collect_free(body, bound + 1, free);
        }
        RExpr::Match(scrutinee, cases) => {
            collect_free(scrutinee, bound, free);
            for case in cases {
                collect_free(&case.body, bound + case.arity as usize, free);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desugar::desugar_program;
    use crate::parser::parse;
    use crate::resolve::{resolve_program, GlobalTable, TagTable};

    fn compile(src: &str) -> CompiledProgram {
        let sexps = parse(src).unwrap();
        let defs = desugar_program(&sexps).unwrap();
        let mut tags = TagTable::new();
        let mut globals = GlobalTable::new();
        let rdefs = resolve_program(&defs, &mut tags, &mut globals).unwrap();
        compile_program(&rdefs)
    }

    #[test]
    fn compile_negb() {
        let prog = compile(
            r#"(define negb (lambda (b) (match b
                            ((True) `(False))
                            ((False) `(True)))))"#,
        );
        assert_eq!(prog.header.n_globals, 1);
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn compile_with_ctor_fields() {
        let prog = compile("(define f (lambda (x) `(Cons ,x ,`(Nil))))");
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn compile_curried() {
        let prog = compile("(define f (lambdas (a b c) `(Cons ,a ,`(Cons ,b ,`(Cons ,c ,`(Nil))))))");
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn compile_let() {
        let prog = compile("(define f (lambda (x) (let ((y x)) y)))");
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn compile_match_bindings() {
        let prog = compile(
            r#"(define f (lambda (l) (match l
                ((Cons h t) h)
                ((Nil) `(Nil)))))"#,
        );
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn compile_full_hash_forest() {
        let src = std::fs::read_to_string("../../scheme/hash_forest.scm").unwrap();
        let prog = compile(&src);
        assert!(prog.header.n_globals > 20);
        assert!(prog.code.len() > 100);
        let blob = prog.serialize();
        assert!(blob.len() > prog.code.len());
    }

    #[test]
    fn compile_partial_app_uses_call1() {
        use crate::bytecode::op;
        let prog = compile(
            "(define f (lambdas (a b) (+ a b)))\n\
             (define g (lambda (x) (f x)))",
        );
        let has_call = prog.code.windows(1).any(|w| w[0] == op::CALL1 || w[0] == op::TAIL_CALL1);
        assert!(has_call, "partial application should use CALL1");
    }

    #[test]
    fn compile_exact_arity_uses_call_n() {
        use crate::bytecode::op;
        let prog = compile(
            "(define f (lambdas (a b) (+ a b)))\n\
             (define g (lambda (x) (f x x)))",
        );
        let has_call_n = prog.code.windows(1).any(|w| w[0] == op::CALL_N || w[0] == op::TAIL_CALL_N);
        assert!(has_call_n, "exact-arity call to known global should use CALL_N");
    }
}
