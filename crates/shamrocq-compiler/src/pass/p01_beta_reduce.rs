//! Beta reduction pass (ExprPass).
//!
//! Rewrites immediately-applied lambda expressions into let bindings:
//!
//!   (lambda (x) body)(arg)  =>  let x = arg in body
//!
//! This eliminates a closure allocation and a CALL instruction, replacing
//! them with a simple BIND. Only fires when the lambda is syntactically in
//! function position (not a variable reference), so it never duplicates code.

use crate::ir::{Defines, Expr};
use super::ExprPass;

pub struct BetaReduce;

impl ExprPass for BetaReduce {
    fn name(&self) -> &'static str { "beta_reduce" }

    fn run(&self, defs: Defines) -> Defines {
        defs.bottom_up(&|e| match e {
            Expr::App(func, arg) => {
                if let Expr::Lambda(param, body) = *func {
                    Expr::Let(param, arg, body)
                } else {
                    Expr::App(func, arg)
                }
            }
            other => other,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Define, Expr};

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
        let result = BetaReduce.run(vec![input].into());
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
        let result = BetaReduce.run(vec![input].into());
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
        let result = BetaReduce.run(vec![input].into());
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
