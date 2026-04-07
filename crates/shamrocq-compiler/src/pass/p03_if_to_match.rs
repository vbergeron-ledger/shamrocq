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

use crate::ir::{Defines, Expr, MatchCase};
use super::ExprPass;

pub struct IfToMatch;

impl ExprPass for IfToMatch {
    fn name(&self) -> &'static str { "if_to_match" }

    fn run(&self, defs: Defines) -> Defines {
        defs.bottom_up(&|e| match e {
            Expr::If(c, t, e) => {
                Expr::Match(
                    c,
                    vec![
                        MatchCase { tag: "True".to_string(), bindings: Vec::new(), body: *t },
                        MatchCase { tag: "False".to_string(), bindings: Vec::new(), body: *e },
                    ],
                )
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
    fn if_becomes_match() {
        // (if x 1 2)  =>  (match x ((True) 1) ((False) 2))
        let input = def("f", Expr::If(
            Box::new(Expr::Var("x".into())),
            Box::new(Expr::Int(1)),
            Box::new(Expr::Int(2)),
        ));
        let result = IfToMatch.run(vec![input].into());
        assert_eq!(result[0].body, Expr::Match(
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
        let result = IfToMatch.run(vec![input].into());
        // Both levels should be Match
        if let Expr::Match(scrut, _) = &result[0].body {
            assert!(matches!(**scrut, Expr::Match(_, _)));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn non_if_unchanged() {
        let input = def("f", Expr::Int(42));
        let result = IfToMatch.run(vec![input].into());
        assert_eq!(result[0].body, Expr::Int(42));
    }
}
