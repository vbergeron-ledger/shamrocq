use crate::parser::Sexp;

#[derive(Debug, Clone, Copy)]
pub enum PrimOp {
    Add, Sub, Mul, Div, Neg, Eq, Lt,
    BytesLen, BytesGet, BytesEq, BytesCat,
}

/// High-level IR after desugaring, before variable resolution.
#[derive(Debug, Clone)]
pub enum Expr {
    Var(String),
    Int(i32),
    Bytes(Vec<u8>),
    /// Nullary or N-ary constructor application via quasiquote: `(Tag field...)
    Ctor(String, Vec<Expr>),
    PrimOp(PrimOp, Vec<Expr>),
    Lambda(String, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    Letrec(String, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<MatchCase>),
    Error,
}

#[derive(Debug, Clone)]
pub struct MatchCase {
    pub tag: String,
    pub bindings: Vec<String>,
    pub body: Expr,
}

/// Top-level definition.
#[derive(Debug, Clone)]
pub struct Define {
    pub name: String,
    pub body: Expr,
}

/// Desugar a list of top-level S-expressions into Defines.
/// Skips `load` forms.
pub fn desugar_program(sexps: &[Sexp]) -> Result<Vec<Define>, String> {
    let mut defs = Vec::new();
    for sexp in sexps {
        match sexp {
            Sexp::List(items) => {
                if let Some(Sexp::Atom(head)) = items.first() {
                    match head.as_str() {
                        "load" => continue,
                        "define" => {
                            let def = desugar_define(items)?;
                            defs.push(def);
                        }
                        _ => return Err(format!("unexpected top-level form: {}", head)),
                    }
                }
            }
            _ => return Err("unexpected top-level atom".to_string()),
        }
    }
    Ok(defs)
}

fn desugar_define(items: &[Sexp]) -> Result<Define, String> {
    if items.len() != 3 {
        return Err("define expects 2 arguments".to_string());
    }
    let name = items[1]
        .as_atom()
        .ok_or("define name must be atom")?
        .to_string();
    let body = desugar_expr(&items[2])?;
    Ok(Define { name, body })
}

fn desugar_expr(sexp: &Sexp) -> Result<Expr, String> {
    match sexp {
        Sexp::Atom(s) => {
            if let Ok(n) = s.parse::<i32>() {
                Ok(Expr::Int(n))
            } else if s.starts_with('"') && s.ends_with('"') {
                Ok(Expr::Bytes(s[1..s.len() - 1].as_bytes().to_vec()))
            } else {
                Ok(Expr::Var(s.clone()))
            }
        }
        Sexp::List(items) if items.is_empty() => Err("empty application".to_string()),
        Sexp::List(items) => {
            let head = &items[0];
            if let Sexp::Atom(tag) = head {
                match tag.as_str() {
                    "lambda" => return desugar_lambda(items),
                    "lambdas" => return desugar_lambdas(items),
                    "@" => return desugar_at(items),
                    "if" => return desugar_if(items),
                    "let" => return desugar_let(items),
                    "letrec" => return desugar_letrec(items),
                    "match" => return desugar_match(items),
                    "quote" => return desugar_quote(&items[1]),
                    "quasiquote" => return desugar_quasiquote(&items[1]),
                    "error" => return Ok(Expr::Error),
                    "+" => return desugar_binop(PrimOp::Add, items),
                    "-" => return desugar_binop(PrimOp::Sub, items),
                    "*" => return desugar_binop(PrimOp::Mul, items),
                    "/" => return desugar_binop(PrimOp::Div, items),
                    "neg" => return desugar_unop(PrimOp::Neg, items),
                    "=" => return desugar_binop(PrimOp::Eq, items),
                    "<" => return desugar_binop(PrimOp::Lt, items),
                    "bytes-len" => return desugar_unop(PrimOp::BytesLen, items),
                    "bytes-get" => return desugar_binop(PrimOp::BytesGet, items),
                    "bytes-eq" => return desugar_binop(PrimOp::BytesEq, items),
                    "bytes-cat" => return desugar_binop(PrimOp::BytesCat, items),
                    _ => {}
                }
            }
            desugar_application(items)
        }
    }
}

fn desugar_binop(op: PrimOp, items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 3 {
        return Err(format!("{:?} expects 2 arguments", op));
    }
    let a = desugar_expr(&items[1])?;
    let b = desugar_expr(&items[2])?;
    Ok(Expr::PrimOp(op, vec![a, b]))
}

fn desugar_unop(op: PrimOp, items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 2 {
        return Err(format!("{:?} expects 1 argument", op));
    }
    let a = desugar_expr(&items[1])?;
    Ok(Expr::PrimOp(op, vec![a]))
}

/// (lambda (x) body) -> Lambda("x", body)
fn desugar_lambda(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 3 {
        return Err("lambda expects params and body".to_string());
    }
    let params = items[1]
        .as_list()
        .ok_or("lambda params must be a list")?;
    if params.len() != 1 {
        return Err(format!(
            "lambda must have exactly 1 param (use lambdas for multi), got {}",
            params.len()
        ));
    }
    let param = params[0]
        .as_atom()
        .ok_or("lambda param must be atom")?
        .to_string();
    let body = desugar_expr(&items[2])?;
    Ok(Expr::Lambda(param, Box::new(body)))
}

/// (lambdas (a b c) body) -> Lambda("a", Lambda("b", Lambda("c", body)))
fn desugar_lambdas(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 3 {
        return Err("lambdas expects params and body".to_string());
    }
    let params = items[1]
        .as_list()
        .ok_or("lambdas params must be a list")?;
    let mut body = desugar_expr(&items[2])?;
    for p in params.iter().rev() {
        let name = p.as_atom().ok_or("lambdas param must be atom")?.to_string();
        body = Expr::Lambda(name, Box::new(body));
    }
    Ok(body)
}

/// (@ f x y z) -> (((f x) y) z)
fn desugar_at(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() < 2 {
        return Err("@ needs at least a function".to_string());
    }
    let mut result = desugar_expr(&items[1])?;
    for arg in &items[2..] {
        let a = desugar_expr(arg)?;
        result = Expr::App(Box::new(result), Box::new(a));
    }
    Ok(result)
}

fn desugar_if(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 4 {
        return Err("if expects 3 arguments".to_string());
    }
    Ok(Expr::If(
        Box::new(desugar_expr(&items[1])?),
        Box::new(desugar_expr(&items[2])?),
        Box::new(desugar_expr(&items[3])?),
    ))
}

/// (let ((x expr)) body)
fn desugar_let(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 3 {
        return Err("let expects bindings and body".to_string());
    }
    let bindings = items[1].as_list().ok_or("let bindings must be a list")?;
    let mut body = desugar_expr(&items[2])?;
    for binding in bindings.iter().rev() {
        let pair = binding.as_list().ok_or("let binding must be a list")?;
        if pair.len() != 2 {
            return Err("let binding must have 2 elements".to_string());
        }
        let name = pair[0]
            .as_atom()
            .ok_or("let binding name must be atom")?
            .to_string();
        let val = desugar_expr(&pair[1])?;
        body = Expr::Let(name, Box::new(val), Box::new(body));
    }
    Ok(body)
}

/// (letrec ((name expr)) body) — only single binding supported
fn desugar_letrec(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 3 {
        return Err("letrec expects bindings and body".to_string());
    }
    let bindings = items[1].as_list().ok_or("letrec bindings must be a list")?;
    if bindings.len() != 1 {
        return Err("letrec supports only a single binding".to_string());
    }
    let pair = bindings[0]
        .as_list()
        .ok_or("letrec binding must be a list")?;
    if pair.len() != 2 {
        return Err("letrec binding must have 2 elements".to_string());
    }
    let name = pair[0]
        .as_atom()
        .ok_or("letrec binding name must be atom")?
        .to_string();
    let val = desugar_expr(&pair[1])?;
    let body = desugar_expr(&items[2])?;
    Ok(Expr::Letrec(name, Box::new(val), Box::new(body)))
}

/// (match scrutinee ((Ctor args...) body) ...)
fn desugar_match(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() < 3 {
        return Err("match needs scrutinee and at least one case".to_string());
    }
    let scrutinee = desugar_expr(&items[1])?;
    let mut cases = Vec::new();
    for case_sexp in &items[2..] {
        let case_list = case_sexp.as_list().ok_or("match case must be a list")?;
        if case_list.len() != 2 {
            return Err("match case must have pattern and body".to_string());
        }
        let pattern = case_list[0]
            .as_list()
            .ok_or("match pattern must be a list")?;
        if pattern.is_empty() {
            return Err("match pattern must have a constructor".to_string());
        }
        let tag = pattern[0]
            .as_atom()
            .ok_or("match constructor must be atom")?
            .to_string();
        let bindings: Vec<String> = pattern[1..]
            .iter()
            .map(|s| {
                s.as_atom()
                    .ok_or("match binding must be atom".to_string())
                    .map(|a| a.to_string())
            })
            .collect::<Result<_, _>>()?;
        let body = desugar_expr(&case_list[1])?;
        cases.push(MatchCase {
            tag,
            bindings,
            body,
        });
    }
    Ok(Expr::Match(Box::new(scrutinee), cases))
}

/// 'foo -> Ctor("foo", [])  (a quoted symbol is a 0-ary constructor tag)
fn desugar_quote(sexp: &Sexp) -> Result<Expr, String> {
    match sexp {
        Sexp::Atom(s) => Ok(Expr::Ctor(s.clone(), Vec::new())),
        Sexp::List(items) if items.is_empty() => Ok(Expr::Ctor("Nil".to_string(), Vec::new())),
        Sexp::List(items) => {
            let tag = items[0]
                .as_atom()
                .ok_or("quoted list head must be atom")?
                .to_string();
            Ok(Expr::Ctor(tag, Vec::new()))
        }
    }
}

/// `(Tag ,expr1 ,expr2) -> Ctor("Tag", [expr1, expr2])
/// `(Tag) -> Ctor("Tag", [])
fn desugar_quasiquote(sexp: &Sexp) -> Result<Expr, String> {
    match sexp {
        Sexp::Atom(s) => Ok(Expr::Ctor(s.clone(), Vec::new())),
        Sexp::List(items) if items.is_empty() => Ok(Expr::Ctor("Nil".to_string(), Vec::new())),
        Sexp::List(items) => {
            let tag = items[0]
                .as_atom()
                .ok_or("quasiquote list head must be atom")?
                .to_string();
            let mut fields = Vec::new();
            for item in &items[1..] {
                match item {
                    Sexp::List(unq) if unq.len() == 2 && unq[0].as_atom() == Some("unquote") => {
                        fields.push(desugar_expr(&unq[1])?);
                    }
                    other => {
                        fields.push(desugar_quasiquote(other)?);
                    }
                }
            }
            Ok(Expr::Ctor(tag, fields))
        }
    }
}

/// Regular function application: (f x) or (f x y ...)
fn desugar_application(items: &[Sexp]) -> Result<Expr, String> {
    let mut result = desugar_expr(&items[0])?;
    for arg in &items[1..] {
        let a = desugar_expr(arg)?;
        result = Expr::App(Box::new(result), Box::new(a));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn desugar(src: &str) -> Vec<Define> {
        let sexps = parse(src).unwrap();
        desugar_program(&sexps).unwrap()
    }

    #[test]
    fn desugar_negb() {
        let defs = desugar(
            r#"(define negb (lambda (b) (match b
                            ((True) `(False))
                            ((False) `(True)))))"#,
        );
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "negb");
        match &defs[0].body {
            Expr::Lambda(param, body) => {
                assert_eq!(param, "b");
                match body.as_ref() {
                    Expr::Match(_, cases) => {
                        assert_eq!(cases.len(), 2);
                        assert_eq!(cases[0].tag, "True");
                        assert_eq!(cases[0].bindings.len(), 0);
                        assert_eq!(cases[1].tag, "False");
                    }
                    other => panic!("expected Match, got {:?}", other),
                }
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn desugar_lambdas_and_at() {
        let defs = desugar("(define leb (lambdas (n m) (@ f n m)))");
        assert_eq!(defs.len(), 1);
        match &defs[0].body {
            Expr::Lambda(p1, inner) => {
                assert_eq!(p1, "n");
                match inner.as_ref() {
                    Expr::Lambda(p2, body) => {
                        assert_eq!(p2, "m");
                        match body.as_ref() {
                            Expr::App(_, _) => {}
                            other => panic!("expected App, got {:?}", other),
                        }
                    }
                    other => panic!("expected inner Lambda, got {:?}", other),
                }
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn desugar_quasiquote_nested() {
        let defs = desugar("(define f (lambda (x) `(Cons ,x ,`(Nil))))");
        assert_eq!(defs.len(), 1);
        match &defs[0].body {
            Expr::Lambda(_, body) => match body.as_ref() {
                Expr::Ctor(tag, fields) => {
                    assert_eq!(tag, "Cons");
                    assert_eq!(fields.len(), 2);
                    match &fields[1] {
                        Expr::Ctor(inner_tag, inner_fields) => {
                            assert_eq!(inner_tag, "Nil");
                            assert!(inner_fields.is_empty());
                        }
                        other => panic!("expected Ctor(Nil), got {:?}", other),
                    }
                }
                other => panic!("expected Ctor, got {:?}", other),
            },
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn desugar_load_skipped() {
        let defs = desugar(r#"(load "macros_extr.scm") (define x (lambda (a) a))"#);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "x");
    }

    #[test]
    fn desugar_full_fourchette() {
        let src = std::fs::read_to_string("../../scheme/fourchette.scm").unwrap();
        let sexps = parse(&src).unwrap();
        let defs = desugar_program(&sexps).unwrap();
        assert!(defs.len() > 20);
    }
}
