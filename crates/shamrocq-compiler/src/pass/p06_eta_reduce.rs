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

use crate::ir::{Ctx, RDefines, RExpr};
use super::ResolvedPass;
use super::p08_anf::{references_local, shift_down};

pub struct EtaReduce;

impl ResolvedPass for EtaReduce {
    fn name(&self) -> &'static str { "eta_reduce" }

    fn run(&self, defs: RDefines) -> RDefines {
        defs.map_bodies(eta)
    }
}

fn eta(expr: RExpr) -> RExpr {
    match expr {
        RExpr::Lambda(body) => {
            let body = eta(*body);
            if let RExpr::App(ref f, ref arg) = body {
                if let RExpr::Local(0) = **arg {
                    if !references_local(f, 0, 0) {
                        return shift_down(f, 0, 1);
                    }
                }
            }
            RExpr::Lambda(Box::new(body))
        }
        other => other.map_children(Ctx::new(), |child, _| eta(child)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{RDefine, RExpr};

    fn rdef(name: &str, body: RExpr) -> RDefine {
        RDefine { name: name.to_string(), global_idx: 0, body }
    }

    #[test]
    fn eta_reduce_global() {
        // lambda x. Global(0)(x)  =>  Global(0)
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::App(Box::new(RExpr::Global(0)), Box::new(RExpr::Local(0))),
        )));
        let result = EtaReduce.run(vec![input].into());
        assert_eq!(result[0].body, RExpr::Global(0));
    }

    #[test]
    fn no_eta_when_var_captured() {
        // lambda x. x(x) -- Local(0) appears in the function part, cannot reduce
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::App(Box::new(RExpr::Local(0)), Box::new(RExpr::Local(0))),
        )));
        let expected = input.clone();
        let result = EtaReduce.run(vec![input].into());
        assert_eq!(result[0].body, expected.body);
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
        let result = EtaReduce.run(vec![input].into());
        assert_eq!(result[0].body, RExpr::Global(0));
    }

    #[test]
    fn no_eta_when_arg_not_local0() {
        // lambda x. Global(0)(Global(1)) -- arg is not Local(0)
        let input = rdef("f", RExpr::Lambda(Box::new(
            RExpr::App(Box::new(RExpr::Global(0)), Box::new(RExpr::Global(1))),
        )));
        let expected = input.clone();
        let result = EtaReduce.run(vec![input].into());
        assert_eq!(result[0].body, expected.body);
    }
}
