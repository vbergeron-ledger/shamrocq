//! Constant folding pass (ExprPass).
//!
//! Evaluates expressions that can be fully resolved at compile time:
//!
//! - Arithmetic on integer literals:
//!     (+ 1 2)  =>  3
//!     (- 5 3)  =>  2
//!     (= 1 1)  =>  True
//!
//! - Match/If on known nullary constructors:
//!     (if True a b)                      =>  a
//!     (match (True) ((True) a) ((False) b))  =>  a

use crate::desugar::{Define, Expr, MatchCase, PrimOp};
use super::ExprPass;

pub struct ConstantFold;

impl ExprPass for ConstantFold {
    fn name(&self) -> &'static str { "constant_fold" }

    fn run(&self, defs: Vec<Define>) -> Vec<Define> {
        defs.into_iter()
            .map(|d| Define {
                name: d.name,
                body: fold(d.body),
            })
            .collect()
    }
}

fn fold(expr: Expr) -> Expr {
    match expr {
        Expr::PrimOp(op, args) => {
            let args: Vec<Expr> = args.into_iter().map(fold).collect();
            fold_primop(op, args)
        }
        Expr::Match(scrut, cases) => {
            let scrut = fold(*scrut);
            let cases: Vec<MatchCase> = cases.into_iter().map(|c| MatchCase {
                tag: c.tag,
                bindings: c.bindings,
                body: fold(c.body),
            }).collect();
            if let Expr::Ctor(ref tag, ref fields) = scrut {
                if fields.is_empty() {
                    if let Some(case) = cases.iter().find(|c| c.tag == *tag) {
                        if case.bindings.is_empty() {
                            return case.body.clone();
                        }
                    }
                }
            }
            Expr::Match(Box::new(scrut), cases)
        }
        Expr::If(c, t, e) => {
            let c = fold(*c);
            let t = fold(*t);
            let e = fold(*e);
            match &c {
                Expr::Ctor(tag, fields) if fields.is_empty() => {
                    if tag == "True" { return t; }
                    if tag == "False" { return e; }
                }
                _ => {}
            }
            Expr::If(Box::new(c), Box::new(t), Box::new(e))
        }
        Expr::App(f, a) => Expr::App(Box::new(fold(*f)), Box::new(fold(*a))),
        Expr::Lambda(p, body) => Expr::Lambda(p, Box::new(fold(*body))),
        Expr::Let(name, val, body) => {
            Expr::Let(name, Box::new(fold(*val)), Box::new(fold(*body)))
        }
        Expr::Letrec(name, val, body) => {
            Expr::Letrec(name, Box::new(fold(*val)), Box::new(fold(*body)))
        }
        Expr::Ctor(tag, fields) => {
            Expr::Ctor(tag, fields.into_iter().map(fold).collect())
        }
        other => other,
    }
}

fn fold_primop(op: PrimOp, args: Vec<Expr>) -> Expr {
    match (&op, args.as_slice()) {
        (PrimOp::Add, [Expr::Int(a), Expr::Int(b)]) => Expr::Int(a.wrapping_add(*b)),
        (PrimOp::Sub, [Expr::Int(a), Expr::Int(b)]) => Expr::Int(a.wrapping_sub(*b)),
        (PrimOp::Mul, [Expr::Int(a), Expr::Int(b)]) => Expr::Int(a.wrapping_mul(*b)),
        (PrimOp::Div, [Expr::Int(a), Expr::Int(b)]) if *b != 0 => Expr::Int(a.wrapping_div(*b)),
        (PrimOp::Neg, [Expr::Int(a)]) => Expr::Int(a.wrapping_neg()),
        (PrimOp::Eq, [Expr::Int(a), Expr::Int(b)]) => {
            if a == b {
                Expr::Ctor("True".to_string(), Vec::new())
            } else {
                Expr::Ctor("False".to_string(), Vec::new())
            }
        }
        (PrimOp::Lt, [Expr::Int(a), Expr::Int(b)]) => {
            if a < b {
                Expr::Ctor("True".to_string(), Vec::new())
            } else {
                Expr::Ctor("False".to_string(), Vec::new())
            }
        }
        _ => Expr::PrimOp(op, args),
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
    fn fold_add() {
        let input = def("f", Expr::PrimOp(PrimOp::Add, vec![Expr::Int(1), Expr::Int(2)]));
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, Expr::Int(3));
    }

    #[test]
    fn fold_eq_true() {
        let input = def("f", Expr::PrimOp(PrimOp::Eq, vec![Expr::Int(5), Expr::Int(5)]));
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, Expr::Ctor("True".into(), vec![]));
    }

    #[test]
    fn fold_eq_false() {
        let input = def("f", Expr::PrimOp(PrimOp::Eq, vec![Expr::Int(1), Expr::Int(2)]));
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, Expr::Ctor("False".into(), vec![]));
    }

    #[test]
    fn fold_if_true() {
        // (if True 1 2)  =>  1
        let input = def("f", Expr::If(
            Box::new(Expr::Ctor("True".into(), vec![])),
            Box::new(Expr::Int(1)),
            Box::new(Expr::Int(2)),
        ));
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, Expr::Int(1));
    }

    #[test]
    fn fold_if_false() {
        // (if False 1 2)  =>  2
        let input = def("f", Expr::If(
            Box::new(Expr::Ctor("False".into(), vec![])),
            Box::new(Expr::Int(1)),
            Box::new(Expr::Int(2)),
        ));
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, Expr::Int(2));
    }

    #[test]
    fn fold_match_known_tag() {
        // (match (True) ((True) 1) ((False) 2))  =>  1
        let input = def("f", Expr::Match(
            Box::new(Expr::Ctor("True".into(), vec![])),
            vec![
                MatchCase { tag: "True".into(), bindings: vec![], body: Expr::Int(1) },
                MatchCase { tag: "False".into(), bindings: vec![], body: Expr::Int(2) },
            ],
        ));
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, Expr::Int(1));
    }

    #[test]
    fn no_fold_dynamic_add() {
        // (+ x 1) unchanged
        let input = def("f", Expr::PrimOp(PrimOp::Add, vec![Expr::Var("x".into()), Expr::Int(1)]));
        let expected = input.clone();
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, expected.body);
    }

    #[test]
    fn fold_nested_arithmetic() {
        // (+ (+ 1 2) (- 10 4))  =>  (+ 3 6)  =>  9
        let input = def("f", Expr::PrimOp(PrimOp::Add, vec![
            Expr::PrimOp(PrimOp::Add, vec![Expr::Int(1), Expr::Int(2)]),
            Expr::PrimOp(PrimOp::Sub, vec![Expr::Int(10), Expr::Int(4)]),
        ]));
        let result = ConstantFold.run(vec![input]);
        assert_eq!(result[0].body, Expr::Int(9));
    }
}
