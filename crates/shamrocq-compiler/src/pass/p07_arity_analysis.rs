//! Arity analysis pass (ResolvedPass).
//!
//! Computes the "true arity" of each global definition by counting
//! the depth of the outermost Lambda chain. This is a pure analysis
//! pass -- it does not transform the IR.
//!
//! The arity information is a prerequisite for future multi-argument
//! APPLY/GRAB instructions that can bypass partial application when
//! a function is called with exactly the right number of arguments.

use crate::resolve::{RDefine, RExpr};
use super::ResolvedPass;

pub struct ArityAnalysis;

impl ResolvedPass for ArityAnalysis {
    fn name(&self) -> &'static str { "arity_analysis" }

    fn run(&self, defs: Vec<RDefine>) -> Vec<RDefine> {
        for d in &defs {
            let _arity = lambda_arity(&d.body);
        }
        defs
    }
}

pub fn lambda_arity(expr: &RExpr) -> u8 {
    let mut depth: u8 = 0;
    let mut e = expr;
    while let RExpr::Lambda(body) = e {
        depth += 1;
        e = body;
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::RExpr;

    #[test]
    fn arity_zero() {
        assert_eq!(lambda_arity(&RExpr::Int(42)), 0);
    }

    #[test]
    fn arity_one() {
        // lambda x. x
        assert_eq!(lambda_arity(&RExpr::Lambda(Box::new(RExpr::Local(0)))), 1);
    }

    #[test]
    fn arity_three() {
        // lambda a. lambda b. lambda c. body
        let e = RExpr::Lambda(Box::new(
            RExpr::Lambda(Box::new(
                RExpr::Lambda(Box::new(RExpr::Local(2))),
            )),
        ));
        assert_eq!(lambda_arity(&e), 3);
    }

    #[test]
    fn arity_stops_at_non_lambda() {
        // lambda a. let _ = 0 in a  => arity 1, not 2
        let e = RExpr::Lambda(Box::new(
            RExpr::Let(Box::new(RExpr::Int(0)), Box::new(RExpr::Local(1))),
        ));
        assert_eq!(lambda_arity(&e), 1);
    }
}
