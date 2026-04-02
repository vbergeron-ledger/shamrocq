use std::collections::HashMap;

use crate::desugar::{Define, Expr, PrimOp};

/// Resolved IR: variables are indices, constructors are tag IDs.
#[derive(Debug, Clone, PartialEq)]
pub enum RExpr {
    /// Local variable, de Bruijn index (0 = innermost)
    Local(u8),
    /// Global variable by slot index
    Global(u16),
    Int(i32),
    Bytes(Vec<u8>),
    /// Constructor: tag id + resolved field exprs
    Ctor(u8, Vec<RExpr>),
    PrimOp(PrimOp, Vec<RExpr>),
    Lambda(Box<RExpr>),
    Lambdas(u8, Box<RExpr>),
    App(Box<RExpr>, Box<RExpr>),
    AppN(Box<RExpr>, Vec<RExpr>),
    Let(Box<RExpr>, Box<RExpr>),
    Letrec(Box<RExpr>, Box<RExpr>),
    Match(Box<RExpr>, Vec<RMatchCase>),
    /// Nat eliminator: CaseNat(zero_case, succ_case, scrutinee)
    CaseNat(Box<RExpr>, Box<RExpr>, Box<RExpr>),
    Error,
    /// A host-provided foreign function, identified by its registration index.
    Foreign(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RMatchCase {
    pub tag: u8,
    pub arity: u8,
    pub body: RExpr,
}

#[derive(Debug, Clone, PartialEq)]
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
        t.intern("True");
        t.intern("False");
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
            body,
        });
    }
    Ok(resolved)
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
        Expr::Bytes(data) => Ok(RExpr::Bytes(data.clone())),

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

        Expr::Lambdas(params, body) => {
            let mut inner = locals.to_vec();
            for p in params {
                inner.push(p.clone());
            }
            let rbody = resolve_expr(body, &inner, tags, globals)?;
            Ok(RExpr::Lambdas(params.len() as u8, Box::new(rbody)))
        }

        Expr::App(f, a) => {
            let rf = resolve_expr(f, locals, tags, globals)?;
            let ra = resolve_expr(a, locals, tags, globals)?;
            Ok(RExpr::App(Box::new(rf), Box::new(ra)))
        }

        Expr::AppN(f, args) => {
            let rf = resolve_expr(f, locals, tags, globals)?;
            let rargs = args
                .iter()
                .map(|a| resolve_expr(a, locals, tags, globals))
                .collect::<Result<_, _>>()?;
            Ok(RExpr::AppN(Box::new(rf), rargs))
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

        Expr::CaseNat(zc, sc, scrut) => Ok(RExpr::CaseNat(
            Box::new(resolve_expr(zc, locals, tags, globals)?),
            Box::new(resolve_expr(sc, locals, tags, globals)?),
            Box::new(resolve_expr(scrut, locals, tags, globals)?),
        )),

        Expr::Error => Ok(RExpr::Error),

        Expr::Foreign(idx) => Ok(RExpr::Foreign(*idx)),
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
    fn resolve_full_hash_forest() {
        let src = std::fs::read_to_string("../../scheme/hash_forest.scm").unwrap();
        let sexps = parse(&src).unwrap();
        let defs = desugar_program(&sexps).unwrap();
        let mut tags = TagTable::new();
        let mut globals = GlobalTable::new();
        let rdefs = resolve_program(&defs, &mut tags, &mut globals).unwrap();
        assert!(rdefs.len() > 20);
        assert_eq!(tags.get("True"), Some(0));
        assert_eq!(tags.get("False"), Some(1));
        assert!(tags.get("Build_hforest").is_some());
    }
}
