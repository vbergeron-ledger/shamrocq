//! De Bruijn index helpers shared across resolved-IR passes.
//!
//! Provides shifting, substitution, and binding-usage analysis for
//! the `RExpr` representation where locals are de Bruijn indices.

use crate::resolve::{RExpr, RMatchCase};

// ---------------------------------------------------------------------------
// Atomicity
// ---------------------------------------------------------------------------

pub fn is_atomic(expr: &RExpr) -> bool {
    matches!(expr, RExpr::Local(_) | RExpr::Global(_) | RExpr::Int(_) | RExpr::Bytes(_) | RExpr::Foreign(_))
}

// ---------------------------------------------------------------------------
// Shifting
// ---------------------------------------------------------------------------

/// Add `amount` to every `Local` index >= `cutoff`.
pub fn shift(expr: &RExpr, cutoff: usize, amount: usize) -> RExpr {
    match expr {
        RExpr::Local(idx) => {
            if (*idx as usize) >= cutoff {
                RExpr::Local(*idx + amount as u8)
            } else {
                RExpr::Local(*idx)
            }
        }
        RExpr::Global(idx) => RExpr::Global(*idx),
        RExpr::Int(n) => RExpr::Int(*n),
        RExpr::Bytes(data) => RExpr::Bytes(data.clone()),
        RExpr::Foreign(idx) => RExpr::Foreign(*idx),
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(*tag, fields.iter().map(|f| shift(f, cutoff, amount)).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(*op, args.iter().map(|a| shift(a, cutoff, amount)).collect())
        }
        RExpr::Lambda(body) => RExpr::Lambda(Box::new(shift(body, cutoff + 1, amount))),
        RExpr::Lambdas(n, body) => RExpr::Lambdas(*n, Box::new(shift(body, cutoff + *n as usize, amount))),
        RExpr::App(func, arg) => RExpr::App(
            Box::new(shift(func, cutoff, amount)),
            Box::new(shift(arg, cutoff, amount)),
        ),
        RExpr::AppN(func, args) => RExpr::AppN(
            Box::new(shift(func, cutoff, amount)),
            args.iter().map(|a| shift(a, cutoff, amount)).collect(),
        ),
        RExpr::Let(val, body) => RExpr::Let(
            Box::new(shift(val, cutoff, amount)),
            Box::new(shift(body, cutoff + 1, amount)),
        ),
        RExpr::Letrec(val, body) => RExpr::Letrec(
            Box::new(shift(val, cutoff + 1, amount)),
            Box::new(shift(body, cutoff + 1, amount)),
        ),
        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(shift(scrut, cutoff, amount)),
            cases
                .iter()
                .map(|c| RMatchCase {
                    tag: c.tag,
                    arity: c.arity,
                    body: shift(&c.body, cutoff + c.arity as usize, amount),
                })
                .collect(),
        ),
        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
            Box::new(shift(zc, cutoff, amount)),
            Box::new(shift(sc, cutoff, amount)),
            Box::new(shift(scrut, cutoff, amount)),
        ),
        RExpr::Error => RExpr::Error,
    }
}

/// Subtract `amount` from every `Local` index >= `cutoff`.
pub fn shift_down(expr: &RExpr, cutoff: usize, amount: usize) -> RExpr {
    match expr {
        RExpr::Local(idx) => {
            if (*idx as usize) >= cutoff {
                RExpr::Local(idx.wrapping_sub(amount as u8))
            } else {
                RExpr::Local(*idx)
            }
        }
        RExpr::Global(idx) => RExpr::Global(*idx),
        RExpr::Int(n) => RExpr::Int(*n),
        RExpr::Bytes(data) => RExpr::Bytes(data.clone()),
        RExpr::Foreign(idx) => RExpr::Foreign(*idx),
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(*tag, fields.iter().map(|f| shift_down(f, cutoff, amount)).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(*op, args.iter().map(|a| shift_down(a, cutoff, amount)).collect())
        }
        RExpr::Lambda(body) => RExpr::Lambda(Box::new(shift_down(body, cutoff + 1, amount))),
        RExpr::Lambdas(n, body) => RExpr::Lambdas(*n, Box::new(shift_down(body, cutoff + *n as usize, amount))),
        RExpr::App(func, arg) => RExpr::App(
            Box::new(shift_down(func, cutoff, amount)),
            Box::new(shift_down(arg, cutoff, amount)),
        ),
        RExpr::AppN(func, args) => RExpr::AppN(
            Box::new(shift_down(func, cutoff, amount)),
            args.iter().map(|a| shift_down(a, cutoff, amount)).collect(),
        ),
        RExpr::Let(val, body) => RExpr::Let(
            Box::new(shift_down(val, cutoff, amount)),
            Box::new(shift_down(body, cutoff + 1, amount)),
        ),
        RExpr::Letrec(val, body) => RExpr::Letrec(
            Box::new(shift_down(val, cutoff + 1, amount)),
            Box::new(shift_down(body, cutoff + 1, amount)),
        ),
        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(shift_down(scrut, cutoff, amount)),
            cases
                .iter()
                .map(|c| RMatchCase {
                    tag: c.tag,
                    arity: c.arity,
                    body: shift_down(&c.body, cutoff + c.arity as usize, amount),
                })
                .collect(),
        ),
        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
            Box::new(shift_down(zc, cutoff, amount)),
            Box::new(shift_down(sc, cutoff, amount)),
            Box::new(shift_down(scrut, cutoff, amount)),
        ),
        RExpr::Error => RExpr::Error,
    }
}

// ---------------------------------------------------------------------------
// Substitution
// ---------------------------------------------------------------------------

/// Combined substitution and binding removal for `Let(val, body)`.
///
/// Replaces every occurrence of the let-bound variable (`Local(depth)` at
/// nesting depth `d`) with `val` (shifted into scope), and simultaneously
/// decrements all free variables above the removed binding slot.
///
/// This is the standard de Bruijn beta-substitution with binder removal in a
/// single pass, avoiding the need for a separate `shift_down` call.
pub fn let_subst(body: &RExpr, val: &RExpr, depth: usize) -> RExpr {
    match body {
        RExpr::Local(idx) => {
            let idx = *idx as usize;
            if idx == depth {
                shift(val, 0, depth)
            } else if idx > depth {
                RExpr::Local((idx - 1) as u8)
            } else {
                RExpr::Local(idx as u8)
            }
        }
        RExpr::Global(idx) => RExpr::Global(*idx),
        RExpr::Int(n) => RExpr::Int(*n),
        RExpr::Bytes(data) => RExpr::Bytes(data.clone()),
        RExpr::Foreign(idx) => RExpr::Foreign(*idx),
        RExpr::Error => RExpr::Error,
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(*tag, fields.iter().map(|f| let_subst(f, val, depth)).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(*op, args.iter().map(|a| let_subst(a, val, depth)).collect())
        }
        RExpr::Lambda(b) => {
            RExpr::Lambda(Box::new(let_subst(b, val, depth + 1)))
        }
        RExpr::Lambdas(n, b) => {
            RExpr::Lambdas(*n, Box::new(let_subst(b, val, depth + *n as usize)))
        }
        RExpr::App(f, a) => RExpr::App(
            Box::new(let_subst(f, val, depth)),
            Box::new(let_subst(a, val, depth)),
        ),
        RExpr::AppN(f, args) => RExpr::AppN(
            Box::new(let_subst(f, val, depth)),
            args.iter().map(|a| let_subst(a, val, depth)).collect(),
        ),
        RExpr::Let(v, b) => RExpr::Let(
            Box::new(let_subst(v, val, depth)),
            Box::new(let_subst(b, val, depth + 1)),
        ),
        RExpr::Letrec(v, b) => RExpr::Letrec(
            Box::new(let_subst(v, val, depth + 1)),
            Box::new(let_subst(b, val, depth + 1)),
        ),
        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(let_subst(scrut, val, depth)),
            cases.iter().map(|c| RMatchCase {
                tag: c.tag,
                arity: c.arity,
                body: let_subst(&c.body, val, depth + c.arity as usize),
            }).collect(),
        ),
        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
            Box::new(let_subst(zc, val, depth)),
            Box::new(let_subst(sc, val, depth)),
            Box::new(let_subst(scrut, val, depth)),
        ),
    }
}

// ---------------------------------------------------------------------------
// Reference analysis
// ---------------------------------------------------------------------------

/// Whether the binding at `target` (adjusted by `depth` under binders) is
/// referenced anywhere in `expr`.
pub fn references_local(expr: &RExpr, target: u8, depth: usize) -> bool {
    match expr {
        RExpr::Local(idx) => *idx as usize == target as usize + depth,
        RExpr::Global(_) | RExpr::Int(_) | RExpr::Bytes(_) | RExpr::Error | RExpr::Foreign(_) => false,
        RExpr::Ctor(_, fields) => fields.iter().any(|f| references_local(f, target, depth)),
        RExpr::PrimOp(_, args) => args.iter().any(|a| references_local(a, target, depth)),
        RExpr::Lambda(body) => references_local(body, target, depth + 1),
        RExpr::Lambdas(n, body) => references_local(body, target, depth + *n as usize),
        RExpr::App(f, a) => references_local(f, target, depth) || references_local(a, target, depth),
        RExpr::AppN(f, args) => references_local(f, target, depth) || args.iter().any(|a| references_local(a, target, depth)),
        RExpr::Let(val, body) => {
            references_local(val, target, depth) || references_local(body, target, depth + 1)
        }
        RExpr::Letrec(val, body) => {
            references_local(val, target, depth + 1) || references_local(body, target, depth + 1)
        }
        RExpr::Match(scrut, cases) => {
            references_local(scrut, target, depth)
                || cases.iter().any(|c| references_local(&c.body, target, depth + c.arity as usize))
        }
        RExpr::CaseNat(zc, sc, scrut) => {
            references_local(zc, target, depth)
                || references_local(sc, target, depth)
                || references_local(scrut, target, depth)
        }
    }
}

// ---------------------------------------------------------------------------
// Binding usage analysis
// ---------------------------------------------------------------------------

/// Summary of how a let-bound variable is used in the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingUsage {
    pub ref_count: usize,
    pub call_only: bool,
}

impl BindingUsage {
    pub fn unused() -> Self { Self { ref_count: 0, call_only: true } }
    fn one_call() -> Self { Self { ref_count: 1, call_only: true } }
    fn one_escape() -> Self { Self { ref_count: 1, call_only: false } }

    fn merge(self, other: Self) -> Self {
        Self {
            ref_count: self.ref_count + other.ref_count,
            call_only: self.call_only && other.call_only,
        }
    }
}

/// Single-pass analysis of how the binding at de Bruijn index `target`
/// (adjusted by `depth` under binders) is used in `expr`.
///
/// Returns the total reference count and whether every reference appears in
/// call-head position (`App`/`AppN` function slot). The `App`/`AppN` cases
/// intercept the target in function position before the generic `Local` arm
/// can classify it as an escape.
pub fn analyze_binding(expr: &RExpr, target: u8, depth: usize) -> BindingUsage {
    let t = target as usize + depth;
    match expr {
        RExpr::Local(idx) => {
            if *idx as usize == t { BindingUsage::one_escape() } else { BindingUsage::unused() }
        }
        RExpr::Global(_) | RExpr::Int(_) | RExpr::Bytes(_) | RExpr::Error | RExpr::Foreign(_) => {
            BindingUsage::unused()
        }
        RExpr::Ctor(_, fields) => fields.iter().fold(BindingUsage::unused(), |acc, f| acc.merge(analyze_binding(f, target, depth))),
        RExpr::PrimOp(_, args) => args.iter().fold(BindingUsage::unused(), |acc, a| acc.merge(analyze_binding(a, target, depth))),

        RExpr::App(f, a) => {
            let arg_usage = analyze_binding(a, target, depth);
            if matches!(f.as_ref(), RExpr::Local(idx) if *idx as usize == t) {
                BindingUsage::one_call().merge(arg_usage)
            } else {
                analyze_binding(f, target, depth).merge(arg_usage)
            }
        }
        RExpr::AppN(f, args) => {
            let args_usage = args.iter().fold(BindingUsage::unused(), |acc, a| acc.merge(analyze_binding(a, target, depth)));
            if matches!(f.as_ref(), RExpr::Local(idx) if *idx as usize == t) {
                BindingUsage::one_call().merge(args_usage)
            } else {
                analyze_binding(f, target, depth).merge(args_usage)
            }
        }

        RExpr::Lambda(body) => analyze_binding(body, target, depth + 1),
        RExpr::Lambdas(n, body) => analyze_binding(body, target, depth + *n as usize),
        RExpr::Let(val, body) => {
            analyze_binding(val, target, depth).merge(analyze_binding(body, target, depth + 1))
        }
        RExpr::Letrec(val, body) => {
            analyze_binding(val, target, depth + 1).merge(analyze_binding(body, target, depth + 1))
        }
        RExpr::Match(scrut, cases) => {
            cases.iter().fold(
                analyze_binding(scrut, target, depth),
                |acc, c| acc.merge(analyze_binding(&c.body, target, depth + c.arity as usize)),
            )
        }
        RExpr::CaseNat(zc, sc, scrut) => {
            analyze_binding(zc, target, depth)
                .merge(analyze_binding(sc, target, depth))
                .merge(analyze_binding(scrut, target, depth))
        }
    }
}
