//! If-to-Match lowering pass (ExprPass).
//!
//! Eliminates the `If` node from the IR by rewriting it as a `Match`
//! on the boolean constructors `True` and `False`:
//!
//!   (if cond then else)
//!     =>  (match cond ((True) then) ((False) else))
//!
//! After this pass, the resolver no longer needs a special case for `If`
//! and the rest of the pipeline only deals with `Match`.

use crate::desugar::{Define, Expr, MatchCase};
use super::SingleExprPass;

pub struct IfToMatch;

impl SingleExprPass for IfToMatch {
    fn name(&self) -> &'static str { "if_to_match" }

    fn run(&self, d: Define) -> Define {
        Define { name: d.name, body: lower(d.body) }
    }
}

fn lower(expr: Expr) -> Expr {
    match expr {
        Expr::If(c, t, e) => {
            let c = lower(*c);
            let t = lower(*t);
            let e = lower(*e);
            Expr::Match(
                Box::new(c),
                vec![
                    MatchCase { tag: "True".to_string(), bindings: Vec::new(), body: t },
                    MatchCase { tag: "False".to_string(), bindings: Vec::new(), body: e },
                ],
            )
        }
        Expr::App(f, a) => Expr::App(Box::new(lower(*f)), Box::new(lower(*a))),
        Expr::AppN(f, args) => Expr::AppN(Box::new(lower(*f)), args.into_iter().map(lower).collect()),
        Expr::Lambda(p, body) => Expr::Lambda(p, Box::new(lower(*body))),
        Expr::Lambdas(params, body) => Expr::Lambdas(params, Box::new(lower(*body))),
        Expr::Let(name, val, body) => {
            Expr::Let(name, Box::new(lower(*val)), Box::new(lower(*body)))
        }
        Expr::Letrec(name, val, body) => {
            Expr::Letrec(name, Box::new(lower(*val)), Box::new(lower(*body)))
        }
        Expr::Match(scrut, cases) => Expr::Match(
            Box::new(lower(*scrut)),
            cases.into_iter().map(|c| MatchCase {
                tag: c.tag,
                bindings: c.bindings,
                body: lower(c.body),
            }).collect(),
        ),
        Expr::Ctor(tag, fields) => {
            Expr::Ctor(tag, fields.into_iter().map(lower).collect())
        }
        Expr::PrimOp(op, args) => {
            Expr::PrimOp(op, args.into_iter().map(lower).collect())
        }
        Expr::CaseNat(zc, sc, scrut) => Expr::CaseNat(
            Box::new(lower(*zc)),
            Box::new(lower(*sc)),
            Box::new(lower(*scrut)),
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
    fn if_becomes_match() {
        // (if x 1 2)  =>  (match x ((True) 1) ((False) 2))
        let input = def("f", Expr::If(
            Box::new(Expr::Var("x".into())),
            Box::new(Expr::Int(1)),
            Box::new(Expr::Int(2)),
        ));
        let result = IfToMatch.run(input);
        assert_eq!(result.body, Expr::Match(
            Box::new(Expr::Var("x".into())),
            vec![
                MatchCase { tag: "True".into(), bindings: vec![], body: Expr::Int(1) },
                MatchCase { tag: "False".into(), bindings: vec![], body: Expr::Int(2) },
            ],
        ));
    }

    #[test]
    fn nested_if_lowered() {
        // (if (if a b c) d e)  =>  (match (match a ...) ...)
        let input = def("f", Expr::If(
            Box::new(Expr::If(
                Box::new(Expr::Var("a".into())),
                Box::new(Expr::Var("b".into())),
                Box::new(Expr::Var("c".into())),
            )),
            Box::new(Expr::Var("d".into())),
            Box::new(Expr::Var("e".into())),
        ));
        let result = IfToMatch.run(input);
        // Both levels should be Match
        if let Expr::Match(scrut, _) = &result.body {
            assert!(matches!(**scrut, Expr::Match(_, _)));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn non_if_unchanged() {
        let input = def("f", Expr::Int(42));
        let result = IfToMatch.run(input);
        assert_eq!(result.body, Expr::Int(42));
    }
}
