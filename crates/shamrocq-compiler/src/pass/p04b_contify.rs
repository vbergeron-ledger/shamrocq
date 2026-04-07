//! Contification pass — Level 1 (ResolvedPass).
//!
//! Inlines `Let`-bound lambdas that are used exactly once in call-head
//! position, and immediately beta-reduces the resulting redex:
//!
//!   let f = λ.body_f in ... f(arg) ...     (f used once, in call position)
//!     =>  ... body_f[0 := arg] ...
//!
//! This eliminates a closure allocation and a CALL_DYNAMIC for every
//! single-use local helper — a pattern very common in Coq-extracted code.

use crate::resolve::{RDefine, RExpr, RMatchCase};
use super::SingleResolvedPass;
use super::debruijn::{analyze_binding, let_subst};

pub struct Contify;

impl SingleResolvedPass for Contify {
    fn name(&self) -> &'static str { "contify" }

    fn run(&self, d: RDefine) -> RDefine {
        RDefine { name: d.name, global_idx: d.global_idx, body: simplify(d.body) }
    }
}

fn simplify(expr: RExpr) -> RExpr {
    match expr {
        RExpr::Let(val, body) => {
            let val = simplify(*val);
            let body = simplify(*body);

            if is_lambda(&val) {
                let usage = analyze_binding(&body, 0, 0);
                if usage.ref_count == 1 && usage.call_only {
                    let inlined = let_subst(&body, &val, 0);
                    return beta_at_call_sites(inlined);
                }
            }

            RExpr::Let(Box::new(val), Box::new(body))
        }
        RExpr::Lambda(body) => RExpr::Lambda(Box::new(simplify(*body))),
        RExpr::Lambdas(n, body) => RExpr::Lambdas(n, Box::new(simplify(*body))),
        RExpr::App(f, a) => RExpr::App(Box::new(simplify(*f)), Box::new(simplify(*a))),
        RExpr::AppN(f, args) => RExpr::AppN(Box::new(simplify(*f)), args.into_iter().map(simplify).collect()),
        RExpr::Letrec(val, body) => {
            RExpr::Letrec(Box::new(simplify(*val)), Box::new(simplify(*body)))
        }
        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(simplify(*scrut)),
            cases.into_iter().map(|c| RMatchCase {
                tag: c.tag,
                arity: c.arity,
                body: simplify(c.body),
            }).collect(),
        ),
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(tag, fields.into_iter().map(simplify).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(op, args.into_iter().map(simplify).collect())
        }
        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
            Box::new(simplify(*zc)),
            Box::new(simplify(*sc)),
            Box::new(simplify(*scrut)),
        ),
        other => other,
    }
}

fn is_lambda(expr: &RExpr) -> bool {
    matches!(expr, RExpr::Lambda(_) | RExpr::Lambdas(_, _))
}

/// Walk the expression and beta-reduce any `App(Lambda(body), arg)` redexes
/// that were exposed by the substitution.
fn beta_at_call_sites(expr: RExpr) -> RExpr {
    match expr {
        RExpr::App(f, a) => {
            let f = beta_at_call_sites(*f);
            let a = beta_at_call_sites(*a);
            if let RExpr::Lambda(body) = f {
                let_subst(&body, &a, 0)
            } else {
                RExpr::App(Box::new(f), Box::new(a))
            }
        }
        RExpr::AppN(f, args) => {
            let f = beta_at_call_sites(*f);
            let args: Vec<RExpr> = args.into_iter().map(beta_at_call_sites).collect();
            beta_appn(f, args)
        }
        RExpr::Lambda(body) => RExpr::Lambda(Box::new(beta_at_call_sites(*body))),
        RExpr::Lambdas(n, body) => RExpr::Lambdas(n, Box::new(beta_at_call_sites(*body))),
        RExpr::Let(val, body) => RExpr::Let(
            Box::new(beta_at_call_sites(*val)),
            Box::new(beta_at_call_sites(*body)),
        ),
        RExpr::Letrec(val, body) => RExpr::Letrec(
            Box::new(beta_at_call_sites(*val)),
            Box::new(beta_at_call_sites(*body)),
        ),
        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(beta_at_call_sites(*scrut)),
            cases.into_iter().map(|c| RMatchCase {
                tag: c.tag,
                arity: c.arity,
                body: beta_at_call_sites(c.body),
            }).collect(),
        ),
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(tag, fields.into_iter().map(beta_at_call_sites).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(op, args.into_iter().map(beta_at_call_sites).collect())
        }
        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
            Box::new(beta_at_call_sites(*zc)),
            Box::new(beta_at_call_sites(*sc)),
            Box::new(beta_at_call_sites(*scrut)),
        ),
        other => other,
    }
}

/// Beta-reduce `AppN(f, args)` when `f` is a multi-arg lambda.
///
/// Peels `Lambda`/`Lambdas` layers off `f` one argument at a time, substituting
/// each arg. If the lambda is fully consumed, returns the reduced body.
/// If there are leftover args (over-application), wraps the result in `AppN`.
fn beta_appn(f: RExpr, args: Vec<RExpr>) -> RExpr {
    if args.is_empty() {
        return f;
    }
    if !is_lambda(&f) {
        return RExpr::AppN(Box::new(f), args);
    }

    let mut current = f;
    let mut remaining = args.into_iter();

    while let Some(arg) = remaining.next() {
        match current {
            RExpr::Lambda(body) => {
                current = let_subst(&body, &arg, 0);
            }
            RExpr::Lambdas(n, body) if n > 1 => {
                current = let_subst(
                    &RExpr::Lambdas(n - 1, body),
                    &arg,
                    0,
                );
            }
            RExpr::Lambdas(1, body) => {
                current = let_subst(&body, &arg, 0);
            }
            _ => {
                let rest: Vec<RExpr> = std::iter::once(arg).chain(remaining).collect();
                return RExpr::AppN(Box::new(current), rest);
            }
        }
    }

    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::RExpr;

    fn rdef(name: &str, body: RExpr) -> RDefine {
        RDefine { name: name.to_string(), global_idx: 0, body }
    }

    #[test]
    fn single_use_lambda_inlined() {
        // let f = λ.Local(0) in f(Int(42))
        //   = Let(Lambda(Local(0)), App(Local(0), Int(42)))
        //   => Int(42)
        let input = rdef("g", RExpr::Let(
            Box::new(RExpr::Lambda(Box::new(RExpr::Local(0)))),
            Box::new(RExpr::App(
                Box::new(RExpr::Local(0)),
                Box::new(RExpr::Int(42)),
            )),
        ));
        let result = Contify.run(input);
        assert_eq!(result.body, RExpr::Int(42));
    }

    #[test]
    fn single_use_lambda_with_outer_var() {
        // λa. let f = λx. a in f(Int(0))
        //   => λa. a
        //
        // de Bruijn: Lambda(Let(Lambda(Local(1)), App(Local(0), Int(0))))
        //   f body: Local(1) refers to `a` (past the λx binder)
        //   After subst f into body: App(Lambda(Local(1)), Int(0))
        //     but Local(1) here still refers to `a` at depth 1 (the outer Lambda)
        //   After beta: let_subst(Local(1), Int(0), 0)
        //     Local(1) > depth=0, so it becomes Local(0) — which is `a` under the outer Lambda
        //   Result: Lambda(Local(0)) = λa. a
        let input = rdef("g", RExpr::Lambda(Box::new(
            RExpr::Let(
                Box::new(RExpr::Lambda(Box::new(RExpr::Local(1)))),
                Box::new(RExpr::App(
                    Box::new(RExpr::Local(0)),
                    Box::new(RExpr::Int(0)),
                )),
            ),
        )));
        let result = Contify.run(input);
        assert_eq!(result.body, RExpr::Lambda(Box::new(RExpr::Local(0))));
    }

    #[test]
    fn multi_use_lambda_not_inlined() {
        // let f = λ.Local(0) in App(f, App(f, Int(1)))
        //   f is used twice in call position — keep it
        let input = rdef("g", RExpr::Let(
            Box::new(RExpr::Lambda(Box::new(RExpr::Local(0)))),
            Box::new(RExpr::App(
                Box::new(RExpr::Local(0)),
                Box::new(RExpr::App(
                    Box::new(RExpr::Local(0)),
                    Box::new(RExpr::Int(1)),
                )),
            )),
        ));
        let expected = input.clone();
        let result = Contify.run(input);
        assert_eq!(result.body, expected.body);
    }

    #[test]
    fn escaping_lambda_not_inlined() {
        // let f = λ.Local(0) in Ctor(0, [f])
        //   f escapes (used as a value, not in call position) — keep it
        let input = rdef("g", RExpr::Let(
            Box::new(RExpr::Lambda(Box::new(RExpr::Local(0)))),
            Box::new(RExpr::Ctor(0, vec![RExpr::Local(0)])),
        ));
        let expected = input.clone();
        let result = Contify.run(input);
        assert_eq!(result.body, expected.body);
    }

    #[test]
    fn non_lambda_let_untouched() {
        // let x = Int(42) in Local(0)  — val is not a lambda, skip
        let input = rdef("g", RExpr::Let(
            Box::new(RExpr::Int(42)),
            Box::new(RExpr::Local(0)),
        ));
        let expected = input.clone();
        let result = Contify.run(input);
        assert_eq!(result.body, expected.body);
    }

    #[test]
    fn single_use_lambdas_multi_arg() {
        // let f = Lambdas(2, PrimOp(Add, [Local(1), Local(0)])) in AppN(f, [Int(3), Int(4)])
        //   => PrimOp(Add, [Int(3), Int(4)])
        use crate::desugar::PrimOp;
        let input = rdef("g", RExpr::Let(
            Box::new(RExpr::Lambdas(2, Box::new(
                RExpr::PrimOp(PrimOp::Add, vec![RExpr::Local(1), RExpr::Local(0)]),
            ))),
            Box::new(RExpr::AppN(
                Box::new(RExpr::Local(0)),
                vec![RExpr::Int(3), RExpr::Int(4)],
            )),
        ));
        let result = Contify.run(input);
        assert_eq!(result.body, RExpr::PrimOp(
            PrimOp::Add,
            vec![RExpr::Int(3), RExpr::Int(4)],
        ));
    }

    #[test]
    fn nested_contify() {
        // let f = λ.Local(0) in let g = λ.App(Global(0), Local(0)) in App(f, App(g, Int(5)))
        //   Both f and g are single-use call-only → both inlined
        //   => App(Global(0), Int(5))
        let input = rdef("h", RExpr::Let(
            Box::new(RExpr::Lambda(Box::new(RExpr::Local(0)))),
            Box::new(RExpr::Let(
                Box::new(RExpr::Lambda(Box::new(
                    RExpr::App(Box::new(RExpr::Global(0)), Box::new(RExpr::Local(0))),
                ))),
                Box::new(RExpr::App(
                    Box::new(RExpr::Local(1)),
                    Box::new(RExpr::App(
                        Box::new(RExpr::Local(0)),
                        Box::new(RExpr::Int(5)),
                    )),
                )),
            )),
        ));
        let result = Contify.run(input);
        assert_eq!(result.body, RExpr::App(
            Box::new(RExpr::Global(0)),
            Box::new(RExpr::Int(5)),
        ));
    }
}
