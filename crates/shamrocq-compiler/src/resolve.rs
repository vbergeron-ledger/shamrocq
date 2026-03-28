use std::collections::HashMap;

use crate::desugar::{Define, Expr, PrimOp};

/// Resolved IR: variables are indices, constructors are tag IDs.
#[derive(Debug, Clone)]
pub enum RExpr {
    /// Local variable, de Bruijn index (0 = innermost)
    Local(u8),
    /// Global variable by slot index
    Global(u16),
    Int(i32),
    /// Constructor: tag id + resolved field exprs
    Ctor(u8, Vec<RExpr>),
    PrimOp(PrimOp, Vec<RExpr>),
    Lambda(Box<RExpr>),
    App(Box<RExpr>, Box<RExpr>),
    Let(Box<RExpr>, Box<RExpr>),
    Letrec(Box<RExpr>, Box<RExpr>),
    Match(Box<RExpr>, Vec<RMatchCase>),
    Error,
}

#[derive(Debug, Clone)]
pub struct RMatchCase {
    pub tag: u8,
    pub arity: u8,
    pub body: RExpr,
}

#[derive(Debug, Clone)]
pub struct RDefine {
    pub name: String,
    pub global_idx: u16,
    pub body: RExpr,
}

/// Interns constructor tag names to u8 IDs.
pub struct TagTable {
    map: HashMap<String, u8>,
    next: u8,
}

impl TagTable {
    pub fn new() -> Self {
        let mut t = TagTable {
            map: HashMap::new(),
            next: 0,
        };
        for name in [
            "True",
            "False",
            "Nil",
            "Cons",
            "O",
            "S",
            "Left",
            "Right",
            "Pair",
            "Build_root",
            "Build_edge",
            "Build_hforest",
        ] {
            t.intern(name);
        }
        t
    }

    pub fn intern(&mut self, name: &str) -> u8 {
        if let Some(&id) = self.map.get(name) {
            return id;
        }
        let id = self.next;
        self.next += 1;
        self.map.insert(name.to_string(), id);
        id
    }

    pub fn get(&self, name: &str) -> Option<u8> {
        self.map.get(name).copied()
    }

    pub fn entries(&self) -> Vec<(String, u8)> {
        let mut v: Vec<_> = self.map.iter().map(|(k, &v)| (k.clone(), v)).collect();
        v.sort_by_key(|(_, id)| *id);
        v
    }
}

/// Maps global names to slot indices.
pub struct GlobalTable {
    map: HashMap<String, u16>,
    next: u16,
}

impl GlobalTable {
    pub fn new() -> Self {
        GlobalTable {
            map: HashMap::new(),
            next: 0,
        }
    }

    pub fn register(&mut self, name: &str) -> u16 {
        let id = self.next;
        self.map.insert(name.to_string(), id);
        self.next += 1;
        id
    }

    pub fn get(&self, name: &str) -> Option<u16> {
        self.map.get(name).copied()
    }

    pub fn count(&self) -> u16 {
        self.next
    }

    pub fn entries(&self) -> Vec<(String, u16)> {
        let mut v: Vec<_> = self.map.iter().map(|(k, &v)| (k.clone(), v)).collect();
        v.sort_by_key(|(_, id)| *id);
        v
    }
}

/// Resolve a full program.
pub fn resolve_program(
    defs: &[Define],
    tags: &mut TagTable,
    globals: &mut GlobalTable,
) -> Result<Vec<RDefine>, String> {
    // First pass: register all global names so mutual references work.
    for def in defs {
        globals.register(&def.name);
    }

    let mut resolved = Vec::new();
    for def in defs {
        let idx = globals.get(&def.name).unwrap();
        let env = Vec::new();
        let body = resolve_expr(&def.body, &env, tags, globals)?;
        resolved.push(RDefine {
            name: def.name.clone(),
            global_idx: idx,
            body: anf_normalize(body),
        });
    }
    Ok(resolved)
}

fn is_atomic(expr: &RExpr) -> bool {
    matches!(expr, RExpr::Local(_) | RExpr::Global(_) | RExpr::Int(_))
}

/// Shift all free de Bruijn indices >= cutoff by `amount`.
fn shift(expr: &RExpr, cutoff: usize, amount: usize) -> RExpr {
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
        RExpr::Ctor(tag, fields) => {
            RExpr::Ctor(*tag, fields.iter().map(|f| shift(f, cutoff, amount)).collect())
        }
        RExpr::PrimOp(op, args) => {
            RExpr::PrimOp(*op, args.iter().map(|a| shift(a, cutoff, amount)).collect())
        }
        RExpr::Lambda(body) => RExpr::Lambda(Box::new(shift(body, cutoff + 1, amount))),
        RExpr::App(func, arg) => RExpr::App(
            Box::new(shift(func, cutoff, amount)),
            Box::new(shift(arg, cutoff, amount)),
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
        RExpr::Error => RExpr::Error,
    }
}

/// Normalize to A-Normal Form: lift non-atomic App arguments and Ctor
/// fields into Let bindings so the codegen never has intermediate
/// temporaries on the stack when BIND pushes match fields.
fn anf_normalize(expr: RExpr) -> RExpr {
    match expr {
        RExpr::Local(_) | RExpr::Global(_) | RExpr::Int(_) | RExpr::Error => expr,

        RExpr::Ctor(tag, fields) => {
            let fields: Vec<RExpr> = fields.into_iter().map(anf_normalize).collect();
            lift_ctor_fields(tag, fields)
        }

        RExpr::PrimOp(op, args) => {
            let args: Vec<RExpr> = args.into_iter().map(anf_normalize).collect();
            lift_primop_args(op, args)
        }

        RExpr::Lambda(body) => RExpr::Lambda(Box::new(anf_normalize(*body))),

        RExpr::App(func, arg) => {
            let func = anf_normalize(*func);
            let arg = anf_normalize(*arg);
            if is_atomic(&arg) {
                RExpr::App(Box::new(func), Box::new(arg))
            } else {
                // Let(arg, App(shift(func,0,1), Local(0)))
                RExpr::Let(
                    Box::new(arg),
                    Box::new(RExpr::App(
                        Box::new(shift(&func, 0, 1)),
                        Box::new(RExpr::Local(0)),
                    )),
                )
            }
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
    }
}

/// Lift non-atomic Ctor fields into Let bindings so the Ctor only
/// references atomic values.
///
/// Non-atomic fields at indices i_0..i_{k-1} become k nested Lets
/// (i_0 outermost). In the innermost scope the Ctor uses Local(k-1-j)
/// for field i_j, and shifts every atomic field by k.
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

fn resolve_expr(
    expr: &Expr,
    locals: &[String],
    tags: &mut TagTable,
    globals: &GlobalTable,
) -> Result<RExpr, String> {
    match expr {
        Expr::Var(name) => {
            for (i, local) in locals.iter().rev().enumerate() {
                if local == name {
                    return Ok(RExpr::Local(i as u8));
                }
            }
            if let Some(idx) = globals.get(name) {
                return Ok(RExpr::Global(idx));
            }
            Err(format!("unresolved variable: {}", name))
        }

        Expr::Int(n) => Ok(RExpr::Int(*n)),

        Expr::PrimOp(op, args) => {
            let rargs = args
                .iter()
                .map(|a| resolve_expr(a, locals, tags, globals))
                .collect::<Result<_, _>>()?;
            Ok(RExpr::PrimOp(*op, rargs))
        }

        Expr::Ctor(name, fields) => {
            let tag = tags.intern(name);
            let rfields: Vec<RExpr> = fields
                .iter()
                .map(|f| resolve_expr(f, locals, tags, globals))
                .collect::<Result<_, _>>()?;
            Ok(RExpr::Ctor(tag, rfields))
        }

        Expr::Lambda(param, body) => {
            let mut inner = locals.to_vec();
            inner.push(param.clone());
            let rbody = resolve_expr(body, &inner, tags, globals)?;
            Ok(RExpr::Lambda(Box::new(rbody)))
        }

        Expr::App(f, a) => {
            let rf = resolve_expr(f, locals, tags, globals)?;
            let ra = resolve_expr(a, locals, tags, globals)?;
            Ok(RExpr::App(Box::new(rf), Box::new(ra)))
        }

        Expr::If(c, t, e) => {
            let rc = resolve_expr(c, locals, tags, globals)?;
            let rt = resolve_expr(t, locals, tags, globals)?;
            let re = resolve_expr(e, locals, tags, globals)?;
            // Desugar if into match on bool tags
            Ok(RExpr::Match(
                Box::new(rc),
                vec![
                    RMatchCase {
                        tag: tags.intern("True"),
                        arity: 0,
                        body: rt,
                    },
                    RMatchCase {
                        tag: tags.intern("False"),
                        arity: 0,
                        body: re,
                    },
                ],
            ))
        }

        Expr::Let(name, val, body) => {
            let rval = resolve_expr(val, locals, tags, globals)?;
            let mut inner = locals.to_vec();
            inner.push(name.clone());
            let rbody = resolve_expr(body, &inner, tags, globals)?;
            Ok(RExpr::Let(Box::new(rval), Box::new(rbody)))
        }

        Expr::Letrec(name, val, body) => {
            // The binding is visible in both val and body.
            let mut inner = locals.to_vec();
            inner.push(name.clone());
            let rval = resolve_expr(val, &inner, tags, globals)?;
            let rbody = resolve_expr(body, &inner, tags, globals)?;
            Ok(RExpr::Letrec(Box::new(rval), Box::new(rbody)))
        }

        Expr::Match(scrutinee, cases) => {
            let rscrutinee = resolve_expr(scrutinee, locals, tags, globals)?;
            let mut rcases = Vec::new();
            for case in cases {
                let tag = tags.intern(&case.tag);
                let arity = case.bindings.len() as u8;
                let mut inner = locals.to_vec();
                for b in &case.bindings {
                    inner.push(b.clone());
                }
                let rbody = resolve_expr(&case.body, &inner, tags, globals)?;
                rcases.push(RMatchCase {
                    tag,
                    arity,
                    body: rbody,
                });
            }
            Ok(RExpr::Match(Box::new(rscrutinee), rcases))
        }

        Expr::Error => Ok(RExpr::Error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desugar::desugar_program;
    use crate::parser::parse;

    #[test]
    fn resolve_negb() {
        let src = r#"(define negb (lambda (b) (match b
                            ((True) `(False))
                            ((False) `(True)))))"#;
        let sexps = parse(src).unwrap();
        let defs = desugar_program(&sexps).unwrap();
        let mut tags = TagTable::new();
        let mut globals = GlobalTable::new();
        let rdefs = resolve_program(&defs, &mut tags, &mut globals).unwrap();
        assert_eq!(rdefs.len(), 1);
        assert_eq!(rdefs[0].global_idx, 0);
    }

    #[test]
    fn resolve_full_fourchette() {
        let src = std::fs::read_to_string("../../scheme/fourchette.scm").unwrap();
        let sexps = parse(&src).unwrap();
        let defs = desugar_program(&sexps).unwrap();
        let mut tags = TagTable::new();
        let mut globals = GlobalTable::new();
        let rdefs = resolve_program(&defs, &mut tags, &mut globals).unwrap();
        assert!(rdefs.len() > 20);
        // All builtin tags should still be present
        assert_eq!(tags.get("True"), Some(0));
        assert_eq!(tags.get("Build_hforest"), Some(11));
    }
}
