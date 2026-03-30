//! Beta reduction pass (ExprPass).
//!
//! Rewrites immediately-applied lambda expressions into let bindings:
//!
//!   (lambda (x) body)(arg)  =>  let x = arg in body
//!
//! This eliminates a closure allocation and a CALL instruction, replacing
//! them with a simple BIND. Only fires when the lambda is syntactically in
//! function position (not a variable reference), so it never duplicates code.

use crate::desugar::{Define, Expr, MatchCase};
use super::ExprPass;

pub struct BetaReduce;

impl ExprPass for BetaReduce {
    fn name(&self) -> &'static str { "beta_reduce" }

    fn run(&self, defs: Vec<Define>) -> Vec<Define> {
        defs.into_iter()
            .map(|d| Define {
                name: d.name,
                body: reduce(d.body),
            })
            .collect()
    }
}

fn reduce(expr: Expr) -> Expr {
    match expr {
        Expr::App(func, arg) => {
            let func = reduce(*func);
            let arg = reduce(*arg);
            if let Expr::Lambda(param, body) = func {
                Expr::Let(param, Box::new(arg), body)
            } else {
                Expr::App(Box::new(func), Box::new(arg))
            }
        }
        Expr::Lambda(p, body) => Expr::Lambda(p, Box::new(reduce(*body))),
        Expr::Let(name, val, body) => {
            Expr::Let(name, Box::new(reduce(*val)), Box::new(reduce(*body)))
        }
        Expr::Letrec(name, val, body) => {
            Expr::Letrec(name, Box::new(reduce(*val)), Box::new(reduce(*body)))
        }
        Expr::If(c, t, e) => {
            Expr::If(Box::new(reduce(*c)), Box::new(reduce(*t)), Box::new(reduce(*e)))
        }
        Expr::Match(scrut, cases) => Expr::Match(
            Box::new(reduce(*scrut)),
            cases.into_iter().map(|c| MatchCase {
                tag: c.tag,
                bindings: c.bindings,
                body: reduce(c.body),
            }).collect(),
        ),
        Expr::Ctor(tag, fields) => {
            Expr::Ctor(tag, fields.into_iter().map(reduce).collect())
        }
        Expr::PrimOp(op, args) => {
            Expr::PrimOp(op, args.into_iter().map(reduce).collect())
        }
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
    fn immediate_app_becomes_let() {
        // ((lambda (x) x) 42)  =>  let x = 42 in x
        let input = def("f", Expr::App(
            Box::new(Expr::Lambda("x".into(), Box::new(Expr::Var("x".into())))),
            Box::new(Expr::Int(42)),
        ));
        let result = BetaReduce.run(vec![input]);
        assert_eq!(result[0].body, Expr::Let(
            "x".into(),
            Box::new(Expr::Int(42)),
            Box::new(Expr::Var("x".into())),
        ));
    }

    #[test]
    fn non_lambda_app_unchanged() {
        // (f 42) stays (f 42)
        let input = def("g", Expr::App(
            Box::new(Expr::Var("f".into())),
            Box::new(Expr::Int(42)),
        ));
        let expected = input.clone();
        let result = BetaReduce.run(vec![input]);
        assert_eq!(result[0].body, expected.body);
    }

    #[test]
    fn nested_beta() {
        // ((lambda (x) ((lambda (y) y) x)) 1)
        //   =>  let x = 1 in (let y = x in y)
        let input = def("f", Expr::App(
            Box::new(Expr::Lambda("x".into(), Box::new(Expr::App(
                Box::new(Expr::Lambda("y".into(), Box::new(Expr::Var("y".into())))),
                Box::new(Expr::Var("x".into())),
            )))),
            Box::new(Expr::Int(1)),
        ));
        let result = BetaReduce.run(vec![input]);
        assert_eq!(result[0].body, Expr::Let(
            "x".into(),
            Box::new(Expr::Int(1)),
            Box::new(Expr::Let(
                "y".into(),
                Box::new(Expr::Var("x".into())),
                Box::new(Expr::Var("y".into())),
            )),
        ));
    }
}
