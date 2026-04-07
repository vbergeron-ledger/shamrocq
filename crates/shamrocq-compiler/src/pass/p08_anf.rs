use crate::desugar::PrimOp;
use crate::resolve::{RDefine, RExpr, RMatchCase};
use super::SingleResolvedPass;
use super::debruijn::{is_atomic, shift};

pub struct AnfNormalize;

impl SingleResolvedPass for AnfNormalize {
    fn name(&self) -> &'static str { "anf_normalize" }

    fn run(&self, d: RDefine) -> RDefine {
        RDefine { name: d.name, global_idx: d.global_idx, body: anf_normalize(d.body) }
    }
}

fn anf_normalize(expr: RExpr) -> RExpr {
    match expr {
        RExpr::Local(_) | RExpr::Global(_) | RExpr::Int(_) | RExpr::Bytes(_) | RExpr::Error | RExpr::Foreign(_) => expr,

        RExpr::Ctor(tag, fields) => {
            let fields: Vec<RExpr> = fields.into_iter().map(anf_normalize).collect();
            lift_ctor_fields(tag, fields)
        }

        RExpr::PrimOp(op, args) => {
            let args: Vec<RExpr> = args.into_iter().map(anf_normalize).collect();
            lift_primop_args(op, args)
        }

        RExpr::Lambda(body) => RExpr::Lambda(Box::new(anf_normalize(*body))),
        RExpr::Lambdas(n, body) => RExpr::Lambdas(n, Box::new(anf_normalize(*body))),

        RExpr::App(func, arg) => {
            let func = anf_normalize(*func);
            let arg = anf_normalize(*arg);
            if is_atomic(&arg) {
                RExpr::App(Box::new(func), Box::new(arg))
            } else {
                RExpr::Let(
                    Box::new(arg),
                    Box::new(RExpr::App(
                        Box::new(shift(&func, 0, 1)),
                        Box::new(RExpr::Local(0)),
                    )),
                )
            }
        }

        RExpr::AppN(func, args) => {
            let func = anf_normalize(*func);
            let args: Vec<RExpr> = args.into_iter().map(anf_normalize).collect();
            let non_atomic: Vec<usize> = (0..args.len())
                .filter(|i| !is_atomic(&args[*i]))
                .collect();
            if non_atomic.is_empty() {
                return RExpr::AppN(Box::new(func), args);
            }
            let k = non_atomic.len();
            let mut new_args = Vec::with_capacity(args.len());
            for (i, arg) in args.iter().enumerate() {
                if let Some(j) = non_atomic.iter().position(|&idx| idx == i) {
                    new_args.push(RExpr::Local((k - 1 - j) as u8));
                } else {
                    new_args.push(shift(arg, 0, k));
                }
            }
            let mut result = RExpr::AppN(Box::new(shift(&func, 0, k)), new_args);
            for j in (0..k).rev() {
                let val = shift(&args[non_atomic[j]], 0, j);
                result = RExpr::Let(Box::new(val), Box::new(result));
            }
            result
        }

        RExpr::Let(val, body) => RExpr::Let(
            Box::new(anf_normalize(*val)),
            Box::new(anf_normalize(*body)),
        ),

        RExpr::Letrec(val, body) => RExpr::Letrec(
            Box::new(anf_normalize(*val)),
            Box::new(anf_normalize(*body)),
        ),

        RExpr::Match(scrut, cases) => RExpr::Match(
            Box::new(anf_normalize(*scrut)),
            cases
                .into_iter()
                .map(|c| RMatchCase {
                    tag: c.tag,
                    arity: c.arity,
                    body: anf_normalize(c.body),
                })
                .collect(),
        ),

        RExpr::CaseNat(zc, sc, scrut) => RExpr::CaseNat(
            Box::new(anf_normalize(*zc)),
            Box::new(anf_normalize(*sc)),
            Box::new(anf_normalize(*scrut)),
        ),
    }
}

fn lift_ctor_fields(tag: u8, fields: Vec<RExpr>) -> RExpr {
    let non_atomic: Vec<usize> = (0..fields.len())
        .filter(|i| !is_atomic(&fields[*i]))
        .collect();

    if non_atomic.is_empty() {
        return RExpr::Ctor(tag, fields);
    }

    let k = non_atomic.len();

    let mut ctor_fields = Vec::with_capacity(fields.len());
    for (i, field) in fields.iter().enumerate() {
        if let Some(j) = non_atomic.iter().position(|&idx| idx == i) {
            ctor_fields.push(RExpr::Local((k - 1 - j) as u8));
        } else {
            ctor_fields.push(shift(field, 0, k));
        }
    }

    let mut result = RExpr::Ctor(tag, ctor_fields);

    for j in (0..k).rev() {
        let val = shift(&fields[non_atomic[j]], 0, j);
        result = RExpr::Let(Box::new(val), Box::new(result));
    }

    result
}

fn lift_primop_args(op: PrimOp, args: Vec<RExpr>) -> RExpr {
    let non_atomic: Vec<usize> = (0..args.len())
        .filter(|i| !is_atomic(&args[*i]))
        .collect();

    if non_atomic.is_empty() {
        return RExpr::PrimOp(op, args);
    }

    let k = non_atomic.len();

    let mut primop_args = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        if let Some(j) = non_atomic.iter().position(|&idx| idx == i) {
            primop_args.push(RExpr::Local((k - 1 - j) as u8));
        } else {
            primop_args.push(shift(arg, 0, k));
        }
    }

    let mut result = RExpr::PrimOp(op, primop_args);

    for j in (0..k).rev() {
        let val = shift(&args[non_atomic[j]], 0, j);
        result = RExpr::Let(Box::new(val), Box::new(result));
    }

    result
}
