//! Eta reduction pass (ResolvedPass).
//!
//! Eliminates redundant wrapper closures. When a lambda simply applies
//! another function to its own argument:
//!
//!   lambda x. f(x)   =>   f     (when x not free in f)
//!
//! In de Bruijn notation: `Lambda(App(f, Local(0)))` where `Local(0)` does
//! not appear free in `f`. This saves one closure allocation and one call
//! at runtime.

use crate::resolve::{RDefine, RExpr, RMatchCase};
use super::SingleResolvedPass;
use super::debruijn::{references_local, shift_down};

pub struct EtaReduce;

impl SingleResolvedPass for EtaReduce {
    fn name(&self) -> &'static str { "eta_reduce" }

    fn run(&self, d: RDefine) -> RDefine {
        RDefine { name: d.name, global_idx: d.global_idx, body: reduce(d.body) }
    }
}

fn reduce(expr: RExpr) -> RExpr {
    match expr {
        RExpr::Lambda(body) => {
            let body = reduce(*body);
            if let RExpr::App(ref f, ref arg) = body {
                if let RExpr::Local(0) = **arg {
                    if !references_local(f, 0, 0) {
                        return shift_down(f, 0, 1);
                    }
                }
            }
            RExpr::Lambda(Box::new(body))
        }
        RExpr::Lambdas(n, body) => RExpr::Lambdas(n, Box::new(reduce(*body))),
        RExpr::App(f, a) => RExpr::App(Box::new(reduce(*f)), Box::new(reduce(*a))),
        RExpr::AppN(f, args) => RExpr::AppN(Box::new(reduce(*f)), args.into_iter().map(reduce).collect()),
        RExpr::Let(val, body) => {
            RExpr::Let(Box::new(reduce(*val)), Box::new(reduce(*body)))
        }
        RExpr::Letrec(val, body) => {
            RExpr::Letrec(Box::new(reduce(*val)), Box::new(reduce(*body)))
        }
        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(reduce(*scrut)),
            cases.into_iter().map(|c| RMatchCase {
                tag: c.tag,
                arity: c.arity,
                body: reduce(c.body),
            }).collect(),
        ),
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(tag, fields.into_iter().map(reduce).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(op, args.into_iter().map(reduce).collect())
        }
        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
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
    use crate::resolve::RExpr;

    fn rdef(name: &str, body: RExpr) -> RDefine {
        RDefine { name: name.to_string(), global_idx: 0, body }
    }

    #[test]
    fn eta_reduce_global() {
        // lambda x. Global(0)(x)  =>  Global(0)
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::App(Box::new(RExpr::Global(0)), Box::new(RExpr::Local(0))),
        )));
        let result = EtaReduce.run(input);
        assert_eq!(result.body, RExpr::Global(0));
    }

    #[test]
    fn no_eta_when_var_captured() {
        // lambda x. x(x) -- Local(0) appears in the function part, cannot reduce
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::App(Box::new(RExpr::Local(0)), Box::new(RExpr::Local(0))),
        )));
        let expected = input.clone();
        let result = EtaReduce.run(input);
        assert_eq!(result.body, expected.body);
    }

    #[test]
    fn eta_reduce_nested() {
        // lambda x. (lambda y. Global(0)(y))(x)
        //   inner reduces to Global(0), then outer: lambda x. Global(0)(x) => Global(0)
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::App(
                Box::new(RExpr::Lambda(Box::new(
                    RExpr::App(Box::new(RExpr::Global(0)), Box::new(RExpr::Local(0))),
                ))),
                Box::new(RExpr::Local(0)),
            ),
        )));
        let result = EtaReduce.run(input);
        assert_eq!(result.body, RExpr::Global(0));
    }

    #[test]
    fn no_eta_when_arg_not_local0() {
        // lambda x. Global(0)(Global(1)) -- arg is not Local(0)
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::App(Box::new(RExpr::Global(0)), Box::new(RExpr::Global(1))),
        )));
        let expected = input.clone();
        let result = EtaReduce.run(input);
        assert_eq!(result.body, expected.body);
    }
}
