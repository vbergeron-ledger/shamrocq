use crate::bytecode::{Emitter, ProgramHeader};
use crate::resolve::{RDefine, RExpr, RMatchCase};

pub struct CompiledProgram {
    pub header: ProgramHeader,
    pub code: Vec<u8>,
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
}

struct DeferredLambda {
    body: RExpr,
    captures: Vec<u8>,
    closure_code_addr_pos: usize,
}

struct Compiler {
    emitter: Emitter,
    deferred: Vec<DeferredLambda>,
    last_closure_captures: Option<Vec<u8>>,
}

/// Compile-time context tracking how de Bruijn indices map to LOAD slots.
///
/// Frame layout (bottom to top):
///   [capture_0, ..., capture_{n-1}, param, bind_0, bind_1, ...]
///
/// LOAD(idx) indexes from the bottom (idx=0 = capture_0).
/// De Bruijn index 0 = most recently introduced binding (top of frame).
///
/// Mapping at frame_depth D, with n_captures N:
///   - let d = D - N  (locally introduced bindings: param + let/match)
///   - de Bruijn idx < d  → LOAD(D - 1 - idx)   (local)
///   - de Bruijn idx >= d → LOAD(capture_slot)   (captured from parent)
///     where parent_de_bruijn = idx - d, and capture_slot = captures.position(parent_de_bruijn)
#[derive(Clone)]
struct Ctx {
    n_captures: usize,
    captures: Vec<u8>,
    frame_depth: usize,
}

impl Ctx {
    fn toplevel() -> Self {
        Ctx {
            n_captures: 0,
            captures: Vec::new(),
            frame_depth: 0,
        }
    }

    fn for_closure(captures: Vec<u8>) -> Self {
        let n = captures.len();
        Ctx {
            n_captures: n,
            captures,
            frame_depth: n + 1, // captures + param
        }
    }

    fn local_depth(&self) -> usize {
        self.frame_depth - self.n_captures
    }

    fn load_slot(&self, de_bruijn: u8) -> u8 {
        let idx = de_bruijn as usize;
        let d = self.local_depth();
        if idx < d {
            (self.frame_depth - 1 - idx) as u8
        } else {
            let parent_idx = (idx - d) as u8;
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
    let mut c = Compiler {
        emitter: Emitter::new(),
        deferred: Vec::new(),
        last_closure_captures: None,
    };

    let mut global_offsets: Vec<(String, u16)> = Vec::new();

    for def in defs {
        let offset = c.emitter.pos() as u16;
        global_offsets.push((def.name.clone(), offset));
        let mut ctx = Ctx::toplevel();
        c.compile_expr(&def.body, &mut ctx, true);
    }

    c.emit_deferred();

    CompiledProgram {
        header: ProgramHeader {
            n_globals: global_offsets.len() as u16,
            globals: global_offsets,
        },
        code: c.emitter.code,
    }
}

impl Compiler {
    fn compile_expr(&mut self, expr: &RExpr, ctx: &mut Ctx, tail: bool) {
        match expr {
            RExpr::Local(idx) => {
                self.emitter.emit_load(ctx.load_slot(*idx));
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
                if fields.is_empty() {
                    self.emitter.emit_imm(*tag);
                } else {
                    for f in fields {
                        self.compile_expr(f, ctx, false);
                    }
                    self.emitter.emit_tuple(*tag, fields.len() as u8);
                }
                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::Lambda(body) => {
                let mut free = Vec::new();
                collect_free(body, 1, &mut free);
                free.sort();
                free.dedup();

                self.emit_captures(&free, ctx);

                let n_captures = free.len() as u8;
                self.emitter.emit_closure(0, n_captures);
                let code_addr_pos = self.emitter.pos() - 3;

                self.last_closure_captures = Some(free.clone());

                self.deferred.push(DeferredLambda {
                    body: (**body).clone(),
                    captures: free,
                    closure_code_addr_pos: code_addr_pos,
                });

                if tail {
                    self.emitter.emit_ret();
                }
            }

            RExpr::App(func, arg) => {
                self.compile_expr(func, ctx, false);
                self.compile_expr(arg, ctx, false);
                if tail {
                    self.emitter.emit_tail_apply();
                } else {
                    self.emitter.emit_apply();
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
                self.emitter.emit_imm(0);
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

            RExpr::Error => {
                self.emitter.emit_error();
            }
        }
    }

    fn emit_captures(&mut self, free: &[u8], ctx: &Ctx) {
        for &parent_idx in free {
            let d = ctx.local_depth();
            let slot = if (parent_idx as usize) < d {
                (ctx.frame_depth - 1 - parent_idx as usize) as u8
            } else {
                let grandparent_idx = parent_idx - d as u8;
                ctx.captures
                    .iter()
                    .position(|&c| c == grandparent_idx)
                    .unwrap_or_else(|| {
                        panic!(
                            "BUG: capture parent_idx {} (grandparent {}) not in ctx.captures {:?}",
                            parent_idx, grandparent_idx, ctx.captures
                        )
                    }) as u8
            };
            self.emitter.emit_load(slot);
        }
    }

    fn compile_match(&mut self, cases: &[RMatchCase], ctx: &mut Ctx, tail: bool) {
        let n = cases.len() as u8;
        let table_start = self.emitter.emit_match_header(n);

        let mut jmp_patches = Vec::new();

        for (i, case) in cases.iter().enumerate() {
            let case_offset = self.emitter.pos() as u16;
            self.emitter
                .patch_match_case(table_start, i, case.tag, case.arity, case_offset);

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
                let mut ctx = Ctx::for_closure(dl.captures);
                self.compile_expr(&dl.body, &mut ctx, true);
            }
        }
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
        RExpr::Global(_) | RExpr::Error => {}
        RExpr::Ctor(_, fields) => {
            for f in fields {
                collect_free(f, bound, free);
            }
        }
        RExpr::Lambda(body) => {
            collect_free(body, bound + 1, free);
        }
        RExpr::App(f, a) => {
            collect_free(f, bound, free);
            collect_free(a, bound, free);
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
    fn compile_full_fourchette() {
        let src = std::fs::read_to_string("../../scheme/fourchette.scm").unwrap();
        let prog = compile(&src);
        assert!(prog.header.n_globals > 20);
        assert!(prog.code.len() > 100);
        let blob = prog.serialize();
        assert!(blob.len() > prog.code.len());
    }
}
