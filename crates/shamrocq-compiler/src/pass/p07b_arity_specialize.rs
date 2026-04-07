//! Arity specialization pass (ResolvedPass).
//!
//! Rewrites calls to known globals when the argument count doesn't
//! match the callee's arity:
//!
//! **Over-application** — more args than the callee expects:
//!
//!   AppN(Global(f), [a, b, c, d])   where arity(f) = 2
//!     =>  AppN(AppN(Global(f), [a, b]), [c, d])
//!
//!   The inner call becomes a CALL (direct jump), avoiding
//!   intermediate closures that CALL_DYNAMIC chains would allocate.
//!
//! **Under-application** — fewer args than the callee expects:
//!
//!   AppN(Global(f), [a])            where arity(f) = 3
//!     =>  Lambdas(2, AppN(Global(f), [shift(a,2), Local(1), Local(0)]))
//!
//!   Eta-expands into a closure whose body uses CALL to the global's
//!   flat entry, eliminating the extend_closure chain at runtime.
//!
//! Also flattens nested App chains targeting a known global into AppN
//! so the above rewrites can fire.

use crate::ir::{Ctx, RDefines, RExpr};
use crate::pass::p08_anf::shift;
use super::ResolvedPass;

pub struct AritySpecialize;

impl ResolvedPass for AritySpecialize {
    fn name(&self) -> &'static str { "arity_specialize" }

    fn run(&self, defs: RDefines) -> RDefines {
        let arities: Vec<u8> = defs.iter().map(|d| d.body.lambda_arity()).collect();
        defs.map_bodies(|b| arity_spec(b, &arities))
    }
}

fn arity_spec(expr: RExpr, arities: &[u8]) -> RExpr {
    let expr = flatten_app_chain(expr);
    match expr {
        RExpr::AppN(func, args) => {
            let func = arity_spec(*func, arities);
            let args: Vec<RExpr> = args.into_iter().map(|a| arity_spec(a, arities)).collect();
            if let RExpr::Global(idx) = &func {
                let arity = arities.get(*idx as usize).copied().unwrap_or(0) as usize;
                if arity >= 2 && args.len() > arity {
                    return split_over(func, args, arity);
                }
                if arity >= 2 && args.len() >= 2 && args.len() < arity {
                    return eta_expand_under(func, args, arity);
                }
            }
            RExpr::AppN(Box::new(func), args)
        }
        other => other.map_children(Ctx::new(), |child, _| arity_spec(child, arities)),
    }
}

/// Flatten `App(App(App(Global(f), a), b), c)` into `AppN(Global(f), [a, b, c])`.
fn flatten_app_chain(expr: RExpr) -> RExpr {
    if !matches!(&expr, RExpr::App(_, _)) {
        return expr;
    }
    let mut args = Vec::new();
    let mut cur = expr;
    while let RExpr::App(func, arg) = cur {
        args.push(*arg);
        cur = *func;
    }
    if matches!(&cur, RExpr::Global(_)) && args.len() >= 2 {
        args.reverse();
        RExpr::AppN(Box::new(cur), args)
    } else {
        args.reverse();
        let mut result = cur;
        for a in args {
            result = RExpr::App(Box::new(result), Box::new(a));
        }
        result
    }
}

/// Over-application: split `AppN(Global(f), [a..arity, rest..])` into
/// `AppN(AppN(Global(f), exact_args), rest_args)`.
fn split_over(func: RExpr, args: Vec<RExpr>, arity: usize) -> RExpr {
    let mut args = args;
    let rest = args.split_off(arity);
    let exact_call = RExpr::AppN(Box::new(func), args);
    if rest.len() == 1 {
        RExpr::App(Box::new(exact_call), Box::new(rest.into_iter().next().unwrap()))
    } else {
        RExpr::AppN(Box::new(exact_call), rest)
    }
}

/// Under-application: eta-expand `AppN(Global(f), [a, b])` where arity(f) = 4 into
/// `Lambdas(2, AppN(Global(f), [shift(a,2), shift(b,2), Local(1), Local(0)]))`.
fn eta_expand_under(func: RExpr, args: Vec<RExpr>, arity: usize) -> RExpr {
    let remaining = arity - args.len();
    let mut full_args: Vec<RExpr> = args.iter()
        .map(|a| shift(a, 0, remaining))
        .collect();
    for i in (0..remaining).rev() {
        full_args.push(RExpr::Local(i as u8));
    }
    RExpr::Lambdas(
        remaining as u8,
        Box::new(RExpr::AppN(Box::new(func), full_args)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RExpr;

    fn specialize(expr: RExpr, arities: &[u8]) -> RExpr {
        arity_spec(expr, arities)
    }

    #[test]
    fn over_application_split() {
        // f has arity 2, called with 4 args
        // AppN(Global(0), [a, b, c, d]) => AppN(AppN(Global(0), [a, b]), [c, d])
        let arities = vec![2u8];
        let input = RExpr::AppN(
            Box::new(RExpr::Global(0)),
            vec![RExpr::Int(1), RExpr::Int(2), RExpr::Int(3), RExpr::Int(4)],
        );
        let result = specialize(input, &arities);
        match result {
            RExpr::AppN(inner_call, rest) => {
                assert_eq!(rest.len(), 2);
                assert_eq!(rest[0], RExpr::Int(3));
                assert_eq!(rest[1], RExpr::Int(4));
                match *inner_call {
                    RExpr::AppN(func, exact) => {
                        assert!(matches!(*func, RExpr::Global(0)));
                        assert_eq!(exact.len(), 2);
                        assert_eq!(exact[0], RExpr::Int(1));
                        assert_eq!(exact[1], RExpr::Int(2));
                    }
                    other => panic!("expected inner AppN, got {:?}", other),
                }
            }
            other => panic!("expected outer AppN, got {:?}", other),
        }
    }

    #[test]
    fn over_application_single_remainder() {
        // f has arity 2, called with 3 args
        // AppN(Global(0), [a, b, c]) => App(AppN(Global(0), [a, b]), c)
        let arities = vec![2u8];
        let input = RExpr::AppN(
            Box::new(RExpr::Global(0)),
            vec![RExpr::Int(1), RExpr::Int(2), RExpr::Int(3)],
        );
        let result = specialize(input, &arities);
        match result {
            RExpr::App(inner_call, rest_arg) => {
                assert_eq!(*rest_arg, RExpr::Int(3));
                assert!(matches!(*inner_call, RExpr::AppN(_, _)));
            }
            other => panic!("expected App, got {:?}", other),
        }
    }

    #[test]
    fn under_application_eta_expand() {
        // f has arity 4, called with 2 args
        // AppN(Global(0), [Int(10), Int(20)])
        //   => Lambdas(2, AppN(Global(0), [Int(10), Int(20), Local(1), Local(0)]))
        let arities = vec![4u8];
        let input = RExpr::AppN(
            Box::new(RExpr::Global(0)),
            vec![RExpr::Int(10), RExpr::Int(20)],
        );
        let result = specialize(input, &arities);
        match result {
            RExpr::Lambdas(n, body) => {
                assert_eq!(n, 2);
                match *body {
                    RExpr::AppN(func, args) => {
                        assert!(matches!(*func, RExpr::Global(0)));
                        assert_eq!(args.len(), 4);
                        assert_eq!(args[0], RExpr::Int(10));
                        assert_eq!(args[1], RExpr::Int(20));
                        assert_eq!(args[2], RExpr::Local(1));
                        assert_eq!(args[3], RExpr::Local(0));
                    }
                    other => panic!("expected AppN, got {:?}", other),
                }
            }
            other => panic!("expected Lambdas, got {:?}", other),
        }
    }

    #[test]
    fn under_application_shifts_local_args() {
        // f has arity 3, called with 2 args including a Local
        // AppN(Global(0), [Local(0), Int(5)])
        //   => Lambdas(1, AppN(Global(0), [Local(1), Int(5), Local(0)]))
        let arities = vec![3u8];
        let input = RExpr::AppN(
            Box::new(RExpr::Global(0)),
            vec![RExpr::Local(0), RExpr::Int(5)],
        );
        let result = specialize(input, &arities);
        match result {
            RExpr::Lambdas(1, body) => {
                match *body {
                    RExpr::AppN(_, ref args) => {
                        assert_eq!(args[0], RExpr::Local(1)); // shifted up by 1
                        assert_eq!(args[1], RExpr::Int(5));
                        assert_eq!(args[2], RExpr::Local(0));
                    }
                    other => panic!("expected AppN, got {:?}", other),
                }
            }
            other => panic!("expected Lambdas(1, ..), got {:?}", other),
        }
    }

    #[test]
    fn exact_application_unchanged() {
        let arities = vec![2u8];
        let input = RExpr::AppN(
            Box::new(RExpr::Global(0)),
            vec![RExpr::Int(1), RExpr::Int(2)],
        );
        let result = specialize(input.clone(), &arities);
        assert_eq!(result, input);
    }

    #[test]
    fn single_arg_under_application_unchanged() {
        // Only fire for >= 2 supplied args (single-arg under-app is just CALL_DYNAMIC)
        let arities = vec![3u8];
        let input = RExpr::App(
            Box::new(RExpr::Global(0)),
            Box::new(RExpr::Int(1)),
        );
        let result = specialize(input.clone(), &arities);
        assert_eq!(result, input);
    }

    #[test]
    fn flatten_app_chain_to_appn() {
        // App(App(Global(0), a), b) => AppN(Global(0), [a, b])
        // then exact match with arity 2 => unchanged
        let arities = vec![2u8];
        let input = RExpr::App(
            Box::new(RExpr::App(
                Box::new(RExpr::Global(0)),
                Box::new(RExpr::Int(1)),
            )),
            Box::new(RExpr::Int(2)),
        );
        let result = specialize(input, &arities);
        match result {
            RExpr::AppN(func, args) => {
                assert!(matches!(*func, RExpr::Global(0)));
                assert_eq!(args, vec![RExpr::Int(1), RExpr::Int(2)]);
            }
            other => panic!("expected AppN, got {:?}", other),
        }
    }

    #[test]
    fn flatten_and_split_over() {
        // App(App(App(Global(0), a), b), c) where arity=2
        // => flatten to AppN(Global(0), [a, b, c])
        // => split to App(AppN(Global(0), [a, b]), c)
        let arities = vec![2u8];
        let input = RExpr::App(
            Box::new(RExpr::App(
                Box::new(RExpr::App(
                    Box::new(RExpr::Global(0)),
                    Box::new(RExpr::Int(1)),
                )),
                Box::new(RExpr::Int(2)),
            )),
            Box::new(RExpr::Int(3)),
        );
        let result = specialize(input, &arities);
        match result {
            RExpr::App(inner, rest) => {
                assert_eq!(*rest, RExpr::Int(3));
                match *inner {
                    RExpr::AppN(func, args) => {
                        assert!(matches!(*func, RExpr::Global(0)));
                        assert_eq!(args.len(), 2);
                    }
                    other => panic!("expected AppN, got {:?}", other),
                }
            }
            other => panic!("expected App, got {:?}", other),
        }
    }
}
