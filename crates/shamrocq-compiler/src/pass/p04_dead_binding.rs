//! Dead binding elimination pass (ResolvedPass).
//!
//! Removes `Let` bindings whose bound variable is never referenced:
//!
//!   let _ = val in body   =>   body   (when Local(0) unused in body)
//!
//! This is safe because the language has no side effects -- if the bound
//! value is never observed, computing it can be skipped entirely. Free
//! variables in `body` are shifted down to account for the removed binding.

use crate::resolve::{RDefine, RExpr, RMatchCase};
use super::SingleResolvedPass;
use super::debruijn::{references_local, shift_down};

pub struct DeadBindingElim;

impl SingleResolvedPass for DeadBindingElim {
    fn name(&self) -> &'static str { "dead_binding_elim" }

    fn run(&self, d: RDefine) -> RDefine {
        RDefine { name: d.name, global_idx: d.global_idx, body: elim(d.body) }
    }
}

fn elim(expr: RExpr) -> RExpr {
    match expr {
        RExpr::Let(val, body) => {
            let val = elim(*val);
            let body = elim(*body);
            if !references_local(&body, 0, 0) {
                shift_down(&body, 0, 1)
            } else {
                RExpr::Let(Box::new(val), Box::new(body))
            }
        }
        RExpr::Lambda(body) => RExpr::Lambda(Box::new(elim(*body))),
        RExpr::Lambdas(n, body) => RExpr::Lambdas(n, Box::new(elim(*body))),
        RExpr::App(f, a) => RExpr::App(Box::new(elim(*f)), Box::new(elim(*a))),
        RExpr::AppN(f, args) => RExpr::AppN(Box::new(elim(*f)), args.into_iter().map(elim).collect()),
        RExpr::Letrec(val, body) => {
            RExpr::Letrec(Box::new(elim(*val)), Box::new(elim(*body)))
        }
        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(elim(*scrut)),
            cases.into_iter().map(|c| RMatchCase {
                tag: c.tag,
                arity: c.arity,
                body: elim(c.body),
            }).collect(),
        ),
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(tag, fields.into_iter().map(elim).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(op, args.into_iter().map(elim).collect())
        }
        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
            Box::new(elim(*zc)),
            Box::new(elim(*sc)),
            Box::new(elim(*scrut)),
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
    fn dead_let_removed() {
        // let _ = 42 in Global(0)  =>  Global(0)
        let input = rdef("f", RExpr::Let(
            Box::new(RExpr::Int(42)),
            Box::new(RExpr::Global(0)),
        ));
        let result = DeadBindingElim.run(input);
        assert_eq!(result.body, RExpr::Global(0));
    }

    #[test]
    fn live_let_kept() {
        // let x = 42 in x   (Local(0) is used)
        let input = rdef("f", RExpr::Let(
            Box::new(RExpr::Int(42)),
            Box::new(RExpr::Local(0)),
        ));
        let result = DeadBindingElim.run(input);
        assert_eq!(result.body, RExpr::Let(
            Box::new(RExpr::Int(42)),
            Box::new(RExpr::Local(0)),
        ));
    }

    #[test]
    fn dead_let_shifts_free_vars() {
        // let _ = 0 in Local(1)  =>  Local(0)
        // Local(1) in the body refers to a variable one level above the dead
        // binding; after removal it becomes Local(0).
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::Let(
                Box::new(RExpr::Int(0)),
                Box::new(RExpr::Local(1)),
            ),
        )));
        let result = DeadBindingElim.run(input);
        assert_eq!(result.body, RExpr::Lambda(Box::new(RExpr::Local(0))));
    }
}
