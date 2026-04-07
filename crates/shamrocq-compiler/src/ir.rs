#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimOp {
    Add, Sub, Mul, Div, Neg, Eq, Lt,
    BytesLen, BytesGet, BytesEq, BytesCat,
}

/// High-level IR after desugaring, before variable resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Var(String),
    Int(i32),
    Bytes(Vec<u8>),
    /// Nullary or N-ary constructor application via quasiquote: `(Tag field...)
    Ctor(String, Vec<Expr>),
    PrimOp(PrimOp, Vec<Expr>),
    Lambda(String, Box<Expr>),
    Lambdas(Vec<String>, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    AppN(Box<Expr>, Vec<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    Letrec(String, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<MatchCase>),
    /// Nat eliminator: CaseNat(zero_case, succ_case, scrutinee)
    CaseNat(Box<Expr>, Box<Expr>, Box<Expr>),
    Error,
    /// A host-provided foreign function, identified by its registration index.
    Foreign(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchCase {
    pub tag: String,
    pub bindings: Vec<String>,
    pub body: Expr,
}

/// Top-level definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Define {
    pub name: String,
    pub body: Expr,
}

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

// ---------------------------------------------------------------------------
// Recursive structure: map_children
// ---------------------------------------------------------------------------

impl Expr {
    /// Apply `f` to each immediate child, rebuilding this node (owned).
    pub fn map_children(self, mut f: impl FnMut(Expr) -> Expr) -> Expr {
        match self {
            Expr::Ctor(tag, fields) => Expr::Ctor(tag, fields.into_iter().map(&mut f).collect()),
            Expr::PrimOp(op, args) => Expr::PrimOp(op, args.into_iter().map(&mut f).collect()),
            Expr::Lambda(p, body) => Expr::Lambda(p, Box::new(f(*body))),
            Expr::Lambdas(ps, body) => Expr::Lambdas(ps, Box::new(f(*body))),
            Expr::App(a, b) => Expr::App(Box::new(f(*a)), Box::new(f(*b))),
            Expr::AppN(a, bs) => Expr::AppN(Box::new(f(*a)), bs.into_iter().map(&mut f).collect()),
            Expr::If(c, t, e) => Expr::If(Box::new(f(*c)), Box::new(f(*t)), Box::new(f(*e))),
            Expr::Let(n, v, b) => Expr::Let(n, Box::new(f(*v)), Box::new(f(*b))),
            Expr::Letrec(n, v, b) => Expr::Letrec(n, Box::new(f(*v)), Box::new(f(*b))),
            Expr::Match(s, cases) => Expr::Match(
                Box::new(f(*s)),
                cases.into_iter().map(|c| MatchCase {
                    tag: c.tag, bindings: c.bindings, body: f(c.body),
                }).collect(),
            ),
            Expr::CaseNat(zc, sc, s) => Expr::CaseNat(Box::new(f(*zc)), Box::new(f(*sc)), Box::new(f(*s))),
            other => other,
        }
    }

    /// Iterate over child references (for read-only analysis).
    pub fn for_each_child(&self, mut f: impl FnMut(&Expr)) {
        match self {
            Expr::Var(_) | Expr::Int(_) | Expr::Bytes(_) | Expr::Error | Expr::Foreign(_) => {}
            Expr::Ctor(_, fields) => fields.iter().for_each(&mut f),
            Expr::PrimOp(_, args) => args.iter().for_each(&mut f),
            Expr::Lambda(_, body) | Expr::Lambdas(_, body) => f(body),
            Expr::App(a, b) => { f(a); f(b); }
            Expr::AppN(a, bs) => { f(a); bs.iter().for_each(&mut f); }
            Expr::If(c, t, e) => { f(c); f(t); f(e); }
            Expr::Let(_, v, b) | Expr::Letrec(_, v, b) => { f(v); f(b); }
            Expr::Match(s, cases) => { f(s); cases.iter().for_each(|c| f(&c.body)); }
            Expr::CaseNat(zc, sc, s) => { f(zc); f(sc); f(s); }
        }
    }

    /// Short-circuit OR over children.
    pub fn any_child(&self, mut f: impl FnMut(&Expr) -> bool) -> bool {
        match self {
            Expr::Var(_) | Expr::Int(_) | Expr::Bytes(_) | Expr::Error | Expr::Foreign(_) => false,
            Expr::Ctor(_, fields) => fields.iter().any(&mut f),
            Expr::PrimOp(_, args) => args.iter().any(&mut f),
            Expr::Lambda(_, body) | Expr::Lambdas(_, body) => f(body),
            Expr::App(a, b) => f(a) || f(b),
            Expr::AppN(a, bs) => f(a) || bs.iter().any(&mut f),
            Expr::If(c, t, e) => f(c) || f(t) || f(e),
            Expr::Let(_, v, b) | Expr::Letrec(_, v, b) => f(v) || f(b),
            Expr::Match(s, cases) => f(s) || cases.iter().any(|c| f(&c.body)),
            Expr::CaseNat(zc, sc, s) => f(zc) || f(sc) || f(s),
        }
    }
}

/// Binder-depth context for RExpr traversals.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub depth: usize,
}

impl Ctx {
    pub fn new() -> Self { Ctx { depth: 0 } }
    pub fn bind(self, n: usize) -> Self { Ctx { depth: self.depth + n } }
}

impl RExpr {
    /// Apply `f` to each immediate child with binder-aware context (owned).
    pub fn map_children(self, ctx: Ctx, mut f: impl FnMut(RExpr, Ctx) -> RExpr) -> RExpr {
        match self {
            RExpr::Ctor(tag, fields) => RExpr::Ctor(tag, fields.into_iter().map(|e| f(e, ctx)).collect()),
            RExpr::PrimOp(op, args) => RExpr::PrimOp(op, args.into_iter().map(|a| f(a, ctx)).collect()),
            RExpr::Lambda(body) => RExpr::Lambda(Box::new(f(*body, ctx.bind(1)))),
            RExpr::Lambdas(n, body) => RExpr::Lambdas(n, Box::new(f(*body, ctx.bind(n as usize)))),
            RExpr::App(a, b) => RExpr::App(Box::new(f(*a, ctx)), Box::new(f(*b, ctx))),
            RExpr::AppN(a, bs) => RExpr::AppN(Box::new(f(*a, ctx)), bs.into_iter().map(|b| f(b, ctx)).collect()),
            RExpr::Let(v, b) => RExpr::Let(Box::new(f(*v, ctx)), Box::new(f(*b, ctx.bind(1)))),
            RExpr::Letrec(v, b) => RExpr::Letrec(Box::new(f(*v, ctx.bind(1))), Box::new(f(*b, ctx.bind(1)))),
            RExpr::Match(s, cases) => RExpr::Match(
                Box::new(f(*s, ctx)),
                cases.into_iter().map(|c| RMatchCase {
                    tag: c.tag, arity: c.arity,
                    body: f(c.body, ctx.bind(c.arity as usize)),
                }).collect(),
            ),
            RExpr::CaseNat(zc, sc, s) => RExpr::CaseNat(Box::new(f(*zc, ctx)), Box::new(f(*sc, ctx)), Box::new(f(*s, ctx))),
            other => other,
        }
    }

    /// Apply `f` to each immediate child (by reference) with binder-aware context, rebuilding.
    pub fn map_children_ref(&self, ctx: Ctx, mut f: impl FnMut(&RExpr, Ctx) -> RExpr) -> RExpr {
        match self {
            RExpr::Local(idx) => RExpr::Local(*idx),
            RExpr::Global(idx) => RExpr::Global(*idx),
            RExpr::Int(n) => RExpr::Int(*n),
            RExpr::Bytes(b) => RExpr::Bytes(b.clone()),
            RExpr::Error => RExpr::Error,
            RExpr::Foreign(idx) => RExpr::Foreign(*idx),
            RExpr::Ctor(tag, fields) => RExpr::Ctor(*tag, fields.iter().map(|e| f(e, ctx)).collect()),
            RExpr::PrimOp(op, args) => RExpr::PrimOp(*op, args.iter().map(|a| f(a, ctx)).collect()),
            RExpr::Lambda(body) => RExpr::Lambda(Box::new(f(body, ctx.bind(1)))),
            RExpr::Lambdas(n, body) => RExpr::Lambdas(*n, Box::new(f(body, ctx.bind(*n as usize)))),
            RExpr::App(a, b) => RExpr::App(Box::new(f(a, ctx)), Box::new(f(b, ctx))),
            RExpr::AppN(a, bs) => RExpr::AppN(Box::new(f(a, ctx)), bs.iter().map(|b| f(b, ctx)).collect()),
            RExpr::Let(v, b) => RExpr::Let(Box::new(f(v, ctx)), Box::new(f(b, ctx.bind(1)))),
            RExpr::Letrec(v, b) => RExpr::Letrec(Box::new(f(v, ctx.bind(1))), Box::new(f(b, ctx.bind(1)))),
            RExpr::Match(s, cases) => RExpr::Match(
                Box::new(f(s, ctx)),
                cases.iter().map(|c| RMatchCase {
                    tag: c.tag, arity: c.arity,
                    body: f(&c.body, ctx.bind(c.arity as usize)),
                }).collect(),
            ),
            RExpr::CaseNat(zc, sc, s) => RExpr::CaseNat(Box::new(f(zc, ctx)), Box::new(f(sc, ctx)), Box::new(f(s, ctx))),
        }
    }

    /// Short-circuit OR over children with binder-aware context.
    pub fn any_child(&self, ctx: Ctx, mut f: impl FnMut(&RExpr, Ctx) -> bool) -> bool {
        match self {
            RExpr::Local(_) | RExpr::Global(_) | RExpr::Int(_) | RExpr::Bytes(_) | RExpr::Error | RExpr::Foreign(_) => false,
            RExpr::Ctor(_, fields) => fields.iter().any(|e| f(e, ctx)),
            RExpr::PrimOp(_, args) => args.iter().any(|a| f(a, ctx)),
            RExpr::Lambda(body) => f(body, ctx.bind(1)),
            RExpr::Lambdas(n, body) => f(body, ctx.bind(*n as usize)),
            RExpr::App(a, b) => f(a, ctx) || f(b, ctx),
            RExpr::AppN(a, bs) => f(a, ctx) || bs.iter().any(|b| f(b, ctx)),
            RExpr::Let(v, b) => f(v, ctx) || f(b, ctx.bind(1)),
            RExpr::Letrec(v, b) => f(v, ctx.bind(1)) || f(b, ctx.bind(1)),
            RExpr::Match(s, cases) => f(s, ctx) || cases.iter().any(|c| f(&c.body, ctx.bind(c.arity as usize))),
            RExpr::CaseNat(zc, sc, s) => f(zc, ctx) || f(sc, ctx) || f(s, ctx),
        }
    }
}

impl Expr {
    /// Bottom-up transform: recurse into children first, then apply `f`.
    pub fn bottom_up(self, f: &impl Fn(Expr) -> Expr) -> Expr {
        f(self.map_children(|child| child.bottom_up(f)))
    }
}

impl Define {
    pub fn bottom_up(self, f: &impl Fn(Expr) -> Expr) -> Define {
        Define { name: self.name, body: self.body.bottom_up(f) }
    }

    pub fn map_body(self, f: impl FnOnce(Expr) -> Expr) -> Define {
        Define { name: self.name, body: f(self.body) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Defines(pub Vec<Define>);

impl std::ops::Deref for Defines {
    type Target = [Define];
    fn deref(&self) -> &[Define] { &self.0 }
}

impl IntoIterator for Defines {
    type Item = Define;
    type IntoIter = std::vec::IntoIter<Define>;
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl FromIterator<Define> for Defines {
    fn from_iter<I: IntoIterator<Item = Define>>(iter: I) -> Self {
        Defines(iter.into_iter().collect())
    }
}

impl From<Vec<Define>> for Defines {
    fn from(v: Vec<Define>) -> Self { Defines(v) }
}

impl Defines {
    pub fn bottom_up(self, f: &impl Fn(Expr) -> Expr) -> Defines {
        self.into_iter().map(|d| d.bottom_up(f)).collect()
    }

    pub fn map_bodies(self, mut f: impl FnMut(Expr) -> Expr) -> Defines {
        self.into_iter().map(|d| d.map_body(&mut f)).collect()
    }
}

impl RExpr {
    /// Bottom-up transform (ignores binder depth): recurse into children first, then apply `f`.
    pub fn bottom_up(self, f: &impl Fn(RExpr) -> RExpr) -> RExpr {
        f(self.map_children(Ctx::new(), |child, _| child.bottom_up(f)))
    }

    /// Count the depth of the outermost Lambda/Lambdas chain.
    pub fn lambda_arity(&self) -> u8 {
        let mut depth: u8 = 0;
        let mut e = self;
        loop {
            match e {
                RExpr::Lambda(body) => { depth += 1; e = body; }
                RExpr::Lambdas(n, body) => { depth += n; e = body; }
                _ => break,
            }
        }
        depth
    }
}

impl RDefine {
    pub fn bottom_up(self, f: &impl Fn(RExpr) -> RExpr) -> RDefine {
        RDefine { name: self.name, global_idx: self.global_idx, body: self.body.bottom_up(f) }
    }

    pub fn map_body(self, f: impl FnOnce(RExpr) -> RExpr) -> RDefine {
        RDefine { name: self.name, global_idx: self.global_idx, body: f(self.body) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RDefines(pub Vec<RDefine>);

impl std::ops::Deref for RDefines {
    type Target = [RDefine];
    fn deref(&self) -> &[RDefine] { &self.0 }
}

impl IntoIterator for RDefines {
    type Item = RDefine;
    type IntoIter = std::vec::IntoIter<RDefine>;
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl FromIterator<RDefine> for RDefines {
    fn from_iter<I: IntoIterator<Item = RDefine>>(iter: I) -> Self {
        RDefines(iter.into_iter().collect())
    }
}

impl From<Vec<RDefine>> for RDefines {
    fn from(v: Vec<RDefine>) -> Self { RDefines(v) }
}

impl RDefines {
    pub fn bottom_up(self, f: &impl Fn(RExpr) -> RExpr) -> RDefines {
        self.into_iter().map(|d| d.bottom_up(f)).collect()
    }

    pub fn map_bodies(self, mut f: impl FnMut(RExpr) -> RExpr) -> RDefines {
        self.into_iter().map(|d| d.map_body(&mut f)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lambda_arity_zero() {
        assert_eq!(RExpr::Int(42).lambda_arity(), 0);
    }

    #[test]
    fn lambda_arity_one() {
        assert_eq!(RExpr::Lambda(Box::new(RExpr::Local(0))).lambda_arity(), 1);
    }

    #[test]
    fn lambda_arity_three() {
        let e = RExpr::Lambda(Box::new(
            RExpr::Lambda(Box::new(
                RExpr::Lambda(Box::new(RExpr::Local(2))),
            )),
        ));
        assert_eq!(e.lambda_arity(), 3);
    }

    #[test]
    fn lambda_arity_stops_at_non_lambda() {
        let e = RExpr::Lambda(Box::new(
            RExpr::Let(Box::new(RExpr::Int(0)), Box::new(RExpr::Local(1))),
        ));
        assert_eq!(e.lambda_arity(), 1);
    }
}
