//! Small-global inlining pass (ExprPass).
//!
//! Replaces references to small, non-recursive global definitions with their
//! body. "Small" means the body's AST has at most 5 nodes.
//!
//!   (define add (lambda (n m) (+ n m)))
//!   (define f (lambda (x) (add x 1)))
//!     =>
//!   (define f (lambda (x) ((lambda (n m) (+ n m)) x 1)))
//!
//! This trades code size for reduced call overhead. On memory-constrained
//! targets the code growth may outweigh the benefit when a small function
//! is used at many call sites.

use std::collections::HashMap;
use crate::desugar::{Define, Expr, MatchCase};
use super::ExprPass;

pub struct InlineSmallGlobals;

impl ExprPass for InlineSmallGlobals {
    fn name(&self) -> &'static str { "inline_small_globals" }

    fn run(&self, defs: Vec<Define>) -> Vec<Define> {
        let candidates = find_candidates(&defs);
        if candidates.is_empty() {
            return defs;
        }
        defs.into_iter()
            .map(|d| Define {
                name: d.name,
                body: inline_expr(d.body, &candidates),
            })
            .collect()
    }
}

fn find_candidates(defs: &[Define]) -> HashMap<String, Expr> {
    let mut candidates = HashMap::new();
    for d in defs {
        if is_small(&d.body) && !references_self(&d.body, &d.name) {
            candidates.insert(d.name.clone(), d.body.clone());
        }
    }
    candidates
}

fn is_small(expr: &Expr) -> bool {
    expr_size(expr) <= 5
}

fn expr_size(expr: &Expr) -> usize {
    match expr {
        Expr::Var(_) | Expr::Int(_) | Expr::Error | Expr::Foreign(_) => 1,
        Expr::Bytes(_) => 1,
        Expr::Ctor(_, fields) => 1 + fields.iter().map(expr_size).sum::<usize>(),
        Expr::PrimOp(_, args) => 1 + args.iter().map(expr_size).sum::<usize>(),
        Expr::Lambda(_, body) => 1 + expr_size(body),
        Expr::Lambdas(params, body) => params.len() + expr_size(body),
        Expr::App(f, a) => 1 + expr_size(f) + expr_size(a),
        Expr::AppN(f, args) => 1 + expr_size(f) + args.iter().map(expr_size).sum::<usize>(),
        Expr::If(c, t, e) => 1 + expr_size(c) + expr_size(t) + expr_size(e),
        Expr::Let(_, val, body) => 1 + expr_size(val) + expr_size(body),
        Expr::Letrec(_, val, body) => 1 + expr_size(val) + expr_size(body),
        Expr::Match(scrut, cases) => {
            1 + expr_size(scrut) + cases.iter().map(|c| expr_size(&c.body)).sum::<usize>()
        }
        Expr::CaseNat(zc, sc, scrut) => 1 + expr_size(zc) + expr_size(sc) + expr_size(scrut),
    }
}

fn references_self(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Var(v) => v == name,
        Expr::Int(_) | Expr::Error | Expr::Bytes(_) | Expr::Foreign(_) => false,
        Expr::Ctor(_, fields) => fields.iter().any(|f| references_self(f, name)),
        Expr::PrimOp(_, args) => args.iter().any(|a| references_self(a, name)),
        Expr::Lambda(_, body) => references_self(body, name),
        Expr::Lambdas(_, body) => references_self(body, name),
        Expr::App(f, a) => references_self(f, name) || references_self(a, name),
        Expr::AppN(f, args) => references_self(f, name) || args.iter().any(|a| references_self(a, name)),
        Expr::If(c, t, e) => {
            references_self(c, name) || references_self(t, name) || references_self(e, name)
        }
        Expr::Let(_, val, body) => references_self(val, name) || references_self(body, name),
        Expr::Letrec(_, val, body) => references_self(val, name) || references_self(body, name),
        Expr::Match(scrut, cases) => {
            references_self(scrut, name)
                || cases.iter().any(|c| references_self(&c.body, name))
        }
        Expr::CaseNat(zc, sc, scrut) => {
            references_self(zc, name) || references_self(sc, name) || references_self(scrut, name)
        }
    }
}

fn inline_expr(expr: Expr, candidates: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Var(ref name) => {
            if let Some(body) = candidates.get(name) {
                body.clone()
            } else {
                expr
            }
        }
        Expr::App(f, a) => {
            Expr::App(Box::new(inline_expr(*f, candidates)), Box::new(inline_expr(*a, candidates)))
        }
        Expr::AppN(f, args) => {
            Expr::AppN(Box::new(inline_expr(*f, candidates)), args.into_iter().map(|a| inline_expr(a, candidates)).collect())
        }
        Expr::Lambda(p, body) => Expr::Lambda(p, Box::new(inline_expr(*body, candidates))),
        Expr::Lambdas(params, body) => Expr::Lambdas(params, Box::new(inline_expr(*body, candidates))),
        Expr::Let(name, val, body) => {
            Expr::Let(name, Box::new(inline_expr(*val, candidates)), Box::new(inline_expr(*body, candidates)))
        }
        Expr::Letrec(name, val, body) => {
            Expr::Letrec(name, Box::new(inline_expr(*val, candidates)), Box::new(inline_expr(*body, candidates)))
        }
        Expr::If(c, t, e) => {
            Expr::If(
                Box::new(inline_expr(*c, candidates)),
                Box::new(inline_expr(*t, candidates)),
                Box::new(inline_expr(*e, candidates)),
            )
        }
        Expr::Match(scrut, cases) => Expr::Match(
            Box::new(inline_expr(*scrut, candidates)),
            cases.into_iter().map(|c| MatchCase {
                tag: c.tag,
                bindings: c.bindings,
                body: inline_expr(c.body, candidates),
            }).collect(),
        ),
        Expr::Ctor(tag, fields) => {
            Expr::Ctor(tag, fields.into_iter().map(|f| inline_expr(f, candidates)).collect())
        }
        Expr::PrimOp(op, args) => {
            Expr::PrimOp(op, args.into_iter().map(|a| inline_expr(a, candidates)).collect())
        }
        Expr::CaseNat(zc, sc, scrut) => Expr::CaseNat(
            Box::new(inline_expr(*zc, candidates)),
            Box::new(inline_expr(*sc, candidates)),
            Box::new(inline_expr(*scrut, candidates)),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desugar::Expr;

    fn def(name: &str, body: Expr) -> Define {
        Define { name: name.to_string(), body }
    }

    #[test]
    fn inline_small_global() {
        // (define id (lambda (x) x))
        // (define f (lambda (y) (id y)))
        //   => f's body becomes (lambda (y) ((lambda (x) x) y))
        let defs = vec![
            def("id", Expr::Lambda("x".into(), Box::new(Expr::Var("x".into())))),
            def("f", Expr::Lambda("y".into(), Box::new(
                Expr::App(Box::new(Expr::Var("id".into())), Box::new(Expr::Var("y".into()))),
            ))),
        ];
        let result = InlineSmallGlobals.run(defs);
        let expected_f = Expr::Lambda("y".into(), Box::new(
            Expr::App(
                Box::new(Expr::Lambda("x".into(), Box::new(Expr::Var("x".into())))),
                Box::new(Expr::Var("y".into())),
            ),
        ));
        assert_eq!(result[1].body, expected_f);
    }

    #[test]
    fn skip_recursive_global() {
        let defs = vec![
            def("loop", Expr::Lambda("x".into(), Box::new(
                Expr::App(Box::new(Expr::Var("loop".into())), Box::new(Expr::Var("x".into()))),
            ))),
        ];
        let expected = defs.clone();
        let result = InlineSmallGlobals.run(defs);
        assert_eq!(result[0].body, expected[0].body);
    }

    #[test]
    fn skip_large_global() {
        let big_body = Expr::Lambda("a".into(), Box::new(
            Expr::Lambda("b".into(), Box::new(
                Expr::Lambda("c".into(), Box::new(
                    Expr::App(Box::new(Expr::Var("a".into())), Box::new(
                        Expr::App(Box::new(Expr::Var("b".into())), Box::new(Expr::Var("c".into()))),
                    )),
                )),
            )),
        ));
        let defs = vec![
            def("big", big_body.clone()),
            def("f", Expr::Var("big".into())),
        ];
        let result = InlineSmallGlobals.run(defs);
        assert_eq!(result[1].body, Expr::Var("big".into()));
    }
}
