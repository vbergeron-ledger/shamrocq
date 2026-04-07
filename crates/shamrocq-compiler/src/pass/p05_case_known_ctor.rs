//! Case-of-known-constructor pass (ResolvedPass).
//!
//! When a `Match` scrutinee is a statically known constructor, the entire
//! match can be replaced by the matching branch body with the constructor
//! fields substituted for the case bindings:
//!
//!   match Ctor(tag, [a, b]) with
//!     | tag(x, y) -> body
//!     =>  body[x := a, y := b]
//!
//! For nullary constructors the substitution is trivial (just pick the body).
//! This often fires after constant folding or inlining produces known ctors.

use crate::ir::{Ctx, RDefines, RExpr, RMatchCase};
use super::ResolvedPass;
use super::p08_anf::{shift, shift_down};

pub struct CaseOfKnownCtor;

impl ResolvedPass for CaseOfKnownCtor {
    fn name(&self) -> &'static str { "case_of_known_ctor" }

    fn run(&self, defs: RDefines) -> RDefines {
        defs.map_bodies(case_known)
    }
}

fn case_known(expr: RExpr) -> RExpr {
    match expr {
        RExpr::Match(scrut, cases) => {
            let scrut = case_known(*scrut);
            let cases: Vec<RMatchCase> = cases.into_iter().map(|c| RMatchCase {
                tag: c.tag,
                arity: c.arity,
                body: case_known(c.body),
            }).collect();

            if let RExpr::Ctor(tag, ref fields) = scrut {
                if let Some(case) = cases.iter().find(|c| c.tag == tag) {
                    let arity = case.arity as usize;
                    if arity == 0 {
                        return case.body.clone();
                    }
                    if arity == fields.len() {
                        return subst_fields(&case.body, fields, arity);
                    }
                }
            }
            RExpr::Match(Box::new(scrut), cases)
        }
        other => other.map_children(Ctx::new(), |child, _| case_known(child)),
    }
}

/// Substitute the match bindings with the constructor fields.
///
/// In de Bruijn convention, within a match case of arity N:
///   Local(0) = field N-1, Local(1) = field N-2, ..., Local(N-1) = field 0
///
/// We replace each Local(i) for i < arity with field[arity - 1 - i] (shifted up
/// to account for the arity bindings being removed), then shift the whole
/// result down by arity.
fn subst_fields(body: &RExpr, fields: &[RExpr], arity: usize) -> RExpr {
    let substituted = subst_rec(body, fields, arity, 0);
    shift_down(&substituted, 0, arity)
}

fn subst_rec(expr: &RExpr, fields: &[RExpr], arity: usize, depth: usize) -> RExpr {
    match expr {
        RExpr::Local(idx) => {
            let idx = *idx as usize;
            if idx >= depth && idx < depth + arity {
                shift(&fields[arity - 1 - (idx - depth)], 0, depth)
            } else if idx >= depth + arity {
                RExpr::Local((idx + arity) as u8)
            } else {
                RExpr::Local(idx as u8)
            }
        }
        _ => expr.map_children_ref(Ctx { depth }, |child: &RExpr, ctx: Ctx| {
            subst_rec(child, fields, arity, ctx.depth)
        }),
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
    fn nullary_ctor_selects_branch() {
        // match True with True -> 1 | False -> 2   =>   1
        let input = rdef("f", RExpr::Match(
            Box::new(RExpr::Ctor(0, vec![])),
            vec![
                RMatchCase { tag: 0, arity: 0, body: RExpr::Int(1) },
                RMatchCase { tag: 1, arity: 0, body: RExpr::Int(2) },
            ],
        ));
        let result = CaseOfKnownCtor.run(vec![input].into());
        assert_eq!(result[0].body, RExpr::Int(1));
    }

    #[test]
    fn unary_ctor_substitutes_field() {
        // match Some(42) with Some(x) -> x   =>   42
        let input = rdef("f", RExpr::Match(
            Box::new(RExpr::Ctor(0, vec![RExpr::Int(42)])),
            vec![
                RMatchCase { tag: 0, arity: 1, body: RExpr::Local(0) },
            ],
        ));
        let result = CaseOfKnownCtor.run(vec![input].into());
        assert_eq!(result[0].body, RExpr::Int(42));
    }

    #[test]
    fn binary_ctor_substitutes_both() {
        // match Pair(10, 20) with Pair(a, b) -> (+ a b)
        //   =>  (+ 10 20)
        // In de Bruijn: Local(1) = field 0, Local(0) = field 1
        use crate::ir::PrimOp;
        let input = rdef("f", RExpr::Match(
            Box::new(RExpr::Ctor(0, vec![RExpr::Int(10), RExpr::Int(20)])),
            vec![
                RMatchCase {
                    tag: 0, arity: 2,
                    body: RExpr::PrimOp(PrimOp::Add, vec![RExpr::Local(1), RExpr::Local(0)]),
                },
            ],
        ));
        let result = CaseOfKnownCtor.run(vec![input].into());
        assert_eq!(result[0].body, RExpr::PrimOp(
            PrimOp::Add,
            vec![RExpr::Int(10), RExpr::Int(20)],
        ));
    }

    #[test]
    fn dynamic_scrutinee_unchanged() {
        let input = rdef("f", RExpr::Match(
            Box::new(RExpr::Local(0)),
            vec![
                RMatchCase { tag: 0, arity: 0, body: RExpr::Int(1) },
                RMatchCase { tag: 1, arity: 0, body: RExpr::Int(2) },
            ],
        ));
        let expected = input.clone();
        let result = CaseOfKnownCtor.run(vec![input].into());
        assert_eq!(result[0].body, expected.body);
    }
}
