//! Church-encoded nat eliminator recognition (ExprPass).
//!
//! Rocq's `Extract Inductive nat` produces eliminators of the form:
//!
//!   ((lambdas (fO fS n) (if (= n 0) (fO 0) (fS (- n 1)))) zc sc scrut)
//!
//! This pass recognizes that exact pattern (name-invariant, structure-matched)
//! and rewrites it to:
//!
//!   CaseNat(zc, sc, scrut)
//!
//! No new variable bindings are introduced, avoiding the scoping issues
//! that plagued the Let-based rewriting approach.

use crate::desugar::{Define, Expr, MatchCase, PrimOp};
use super::ExprPass;

pub struct CaseNat;

impl ExprPass for CaseNat {
    fn name(&self) -> &'static str { "case_nat" }

    fn run(&self, defs: Vec<Define>) -> Vec<Define> {
        defs.into_iter()
            .map(|d| Define {
                name: d.name,
                body: reduce(d.body),
            })
            .collect()
    }
}

/// Try to match the Church-encoded nat eliminator pattern.
/// Returns `(zc, sc, scrut)` on success.
fn try_match_nat_elim(func: &Expr, args: &[Expr]) -> Option<(Expr, Expr, Expr)> {
    if args.len() != 3 {
        return None;
    }
    let (params, body) = match func {
        Expr::Lambdas(params, body) if params.len() == 3 => (params, body.as_ref()),
        _ => return None,
    };
    let (p0, p1, p2) = (&params[0], &params[1], &params[2]);

    let (cond, then_br, else_br) = match body {
        Expr::If(c, t, e) => (c.as_ref(), t.as_ref(), e.as_ref()),
        _ => return None,
    };

    match cond {
        Expr::PrimOp(PrimOp::Eq, eq_args)
            if eq_args.len() == 2
                && matches!(&eq_args[0], Expr::Var(v) if v == p2)
                && matches!(&eq_args[1], Expr::Int(0)) => {}
        _ => return None,
    }

    match then_br {
        Expr::App(f, a)
            if matches!(f.as_ref(), Expr::Var(v) if v == p0)
                && matches!(a.as_ref(), Expr::Int(0)) => {}
        _ => return None,
    }

    match else_br {
        Expr::App(f, a) if matches!(f.as_ref(), Expr::Var(v) if v == p1) => {
            match a.as_ref() {
                Expr::PrimOp(PrimOp::Sub, sub_args)
                    if sub_args.len() == 2
                        && matches!(&sub_args[0], Expr::Var(v) if v == p2)
                        && matches!(&sub_args[1], Expr::Int(1)) => {}
                _ => return None,
            }
        }
        _ => return None,
    }

    Some((args[0].clone(), args[1].clone(), args[2].clone()))
}

fn reduce(expr: Expr) -> Expr {
    match expr {
        Expr::AppN(f, args) => {
            let f = reduce(*f);
            let args: Vec<Expr> = args.into_iter().map(reduce).collect();
            if let Some((zc, sc, scrut)) = try_match_nat_elim(&f, &args) {
                Expr::CaseNat(Box::new(zc), Box::new(sc), Box::new(scrut))
            } else {
                Expr::AppN(Box::new(f), args)
            }
        }
        Expr::App(f, a) => Expr::App(Box::new(reduce(*f)), Box::new(reduce(*a))),
        Expr::Lambda(p, body) => Expr::Lambda(p, Box::new(reduce(*body))),
        Expr::Lambdas(params, body) => Expr::Lambdas(params, Box::new(reduce(*body))),
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
        Expr::CaseNat(zc, sc, scrut) => Expr::CaseNat(
            Box::new(reduce(*zc)),
            Box::new(reduce(*sc)),
            Box::new(reduce(*scrut)),
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

    fn nat_elim_appn(zc: Expr, sc: Expr, scrut: Expr) -> Expr {
        Expr::AppN(
            Box::new(Expr::Lambdas(
                vec!["fO".into(), "fS".into(), "n".into()],
                Box::new(Expr::If(
                    Box::new(Expr::PrimOp(PrimOp::Eq, vec![
                        Expr::Var("n".into()),
                        Expr::Int(0),
                    ])),
                    Box::new(Expr::App(
                        Box::new(Expr::Var("fO".into())),
                        Box::new(Expr::Int(0)),
                    )),
                    Box::new(Expr::App(
                        Box::new(Expr::Var("fS".into())),
                        Box::new(Expr::PrimOp(PrimOp::Sub, vec![
                            Expr::Var("n".into()),
                            Expr::Int(1),
                        ])),
                    )),
                )),
            )),
            vec![zc, sc, scrut],
        )
    }

    #[test]
    fn rewrites_to_case_nat() {
        let zc = Expr::Lambda("_".into(), Box::new(Expr::Var("l".into())));
        let sc = Expr::Lambda("fuel~".into(), Box::new(Expr::Var("l".into())));
        let scrut = Expr::Var("fuel".into());
        let input = def("f", nat_elim_appn(zc.clone(), sc.clone(), scrut.clone()));
        let result = CaseNat.run(vec![input]);
        assert_eq!(
            result[0].body,
            Expr::CaseNat(Box::new(zc), Box::new(sc), Box::new(scrut)),
        );
    }

    #[test]
    fn different_param_names() {
        let input = def("g", Expr::AppN(
            Box::new(Expr::Lambdas(
                vec!["a".into(), "b".into(), "x".into()],
                Box::new(Expr::If(
                    Box::new(Expr::PrimOp(PrimOp::Eq, vec![
                        Expr::Var("x".into()),
                        Expr::Int(0),
                    ])),
                    Box::new(Expr::App(
                        Box::new(Expr::Var("a".into())),
                        Box::new(Expr::Int(0)),
                    )),
                    Box::new(Expr::App(
                        Box::new(Expr::Var("b".into())),
                        Box::new(Expr::PrimOp(PrimOp::Sub, vec![
                            Expr::Var("x".into()),
                            Expr::Int(1),
                        ])),
                    )),
                )),
            )),
            vec![Expr::Int(10), Expr::Int(20), Expr::Var("k".into())],
        ));
        let result = CaseNat.run(vec![input]);
        match &result[0].body {
            Expr::CaseNat(zc, sc, scrut) => {
                assert_eq!(**zc, Expr::Int(10));
                assert_eq!(**sc, Expr::Int(20));
                assert_eq!(**scrut, Expr::Var("k".into()));
            }
            other => panic!("expected CaseNat, got {:?}", other),
        }
    }

    #[test]
    fn non_matching_unchanged() {
        let input = def("h", Expr::AppN(
            Box::new(Expr::Var("f".into())),
            vec![Expr::Int(1), Expr::Int(2)],
        ));
        let expected = input.clone();
        let result = CaseNat.run(vec![input]);
        assert_eq!(result[0].body, expected.body);
    }

    #[test]
    fn nested_nat_elim() {
        let inner = nat_elim_appn(
            Expr::Lambda("_".into(), Box::new(Expr::Var("l".into()))),
            Expr::Lambda("m".into(), Box::new(Expr::Var("l".into()))),
            Expr::Var("n".into()),
        );
        let input = def("f", Expr::Lambdas(
            vec!["n".into(), "l".into()],
            Box::new(inner),
        ));
        let result = CaseNat.run(vec![input]);
        match &result[0].body {
            Expr::Lambdas(_, body) => {
                assert!(matches!(body.as_ref(), Expr::CaseNat(..)));
            }
            other => panic!("expected Lambdas, got {:?}", other),
        }
    }
}
