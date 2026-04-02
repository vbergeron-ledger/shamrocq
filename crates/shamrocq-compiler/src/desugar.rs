use crate::parser::Sexp;

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

/// Desugar a list of top-level S-expressions into Defines.
/// Skips `load` forms.
pub fn desugar_program(sexps: &[Sexp]) -> Result<Vec<Define>, String> {
    let mut defs = Vec::new();
    let mut n_foreign: u16 = 0;
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
                        "define-foreign" => {
                            match items.len() {
                                2 => {
                                    // (define-foreign name) — 1-arg, host receives raw Value
                                    let name = items[1]
                                        .as_atom()
                                        .ok_or("define-foreign name must be an atom")?
                                        .to_string();
                                    let idx = n_foreign;
                                    n_foreign += 1;
                                    defs.push(Define { name, body: Expr::Foreign(idx) });
                                }
                                3 => {
                                    // (define-foreign name (p1 p2 ... pN)) — N-arg,
                                    // compiler generates curried wrapper that packs args
                                    // into a constructor; host unpacks with ctor_field().
                                    let name = items[1]
                                        .as_atom()
                                        .ok_or("define-foreign name must be an atom")?
                                        .to_string();
                                    let params = items[2]
                                        .as_list()
                                        .ok_or("define-foreign params must be a list")?;
                                    if params.is_empty() {
                                        return Err("define-foreign params list must not be empty".to_string());
                                    }
                                    let param_names: Vec<String> = params
                                        .iter()
                                        .map(|p| {
                                            p.as_atom()
                                                .ok_or("define-foreign param must be an atom".to_string())
                                                .map(|s| s.to_string())
                                        })
                                        .collect::<Result<_, _>>()?;
                                    let idx = n_foreign;
                                    n_foreign += 1;
                                    // Pack all params into a single constructor so the
                                    // host receives one Value and reads fields by index.
                                    let pack_tag = format!("__ffi{}", idx);
                                    let ctor_fields: Vec<Expr> = param_names
                                        .iter()
                                        .map(|p| Expr::Var(p.clone()))
                                        .collect();
                                    let body = Expr::App(
                                        Box::new(Expr::Foreign(idx)),
                                        Box::new(Expr::Ctor(pack_tag, ctor_fields)),
                                    );
                                    let body = Expr::Lambdas(param_names, Box::new(body));
                                    defs.push(Define { name, body });
                                }
                                _ => return Err(
                                    "define-foreign expects a name or a name and a params list".to_string()
                                ),
                            }
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

/// (lambdas (a b c) body) -> Lambdas(["a", "b", "c"], body)
fn desugar_lambdas(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() != 3 {
        return Err("lambdas expects params and body".to_string());
    }
    let params = items[1]
        .as_list()
        .ok_or("lambdas params must be a list")?;
    let param_names: Vec<String> = params
        .iter()
        .map(|p| {
            p.as_atom()
                .ok_or("lambdas param must be atom".to_string())
                .map(|s| s.to_string())
        })
        .collect::<Result<_, _>>()?;
    let body = desugar_expr(&items[2])?;
    Ok(Expr::Lambdas(param_names, Box::new(body)))
}

/// (@ f x y z) -> AppN(f, [x, y, z])
fn desugar_at(items: &[Sexp]) -> Result<Expr, String> {
    if items.len() < 2 {
        return Err("@ needs at least a function".to_string());
    }
    let func = desugar_expr(&items[1])?;
    let args: Vec<Expr> = items[2..]
        .iter()
        .map(desugar_expr)
        .collect::<Result<_, _>>()?;
    if args.len() == 1 {
        Ok(Expr::App(Box::new(func), Box::new(args.into_iter().next().unwrap())))
    } else if args.is_empty() {
        Ok(func)
    } else {
        Ok(Expr::AppN(Box::new(func), args))
    }
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

fn is_ctor_tag(s: &str) -> bool {
    s.parse::<i32>().is_err()
}

/// `(Tag ,expr1 ,expr2) -> Ctor("Tag", [expr1, expr2])
/// `(Tag) -> Ctor("Tag", [])
/// `(EXPR ,e1 ...) where head is not a ctor tag -> application with unquotes unwrapped
fn desugar_quasiquote(sexp: &Sexp) -> Result<Expr, String> {
    match sexp {
        Sexp::Atom(s) => Ok(Expr::Ctor(s.clone(), Vec::new())),
        Sexp::List(items) if items.is_empty() => Ok(Expr::Ctor("Nil".to_string(), Vec::new())),
        Sexp::List(items) => {
            match items[0].as_atom() {
                Some(tag) if is_ctor_tag(tag) => {
                    let tag = tag.to_string();
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
                _ => {
                    let parts: Vec<Sexp> = items.iter().map(|item| match item {
                        Sexp::List(unq) if unq.len() == 2 && unq[0].as_atom() == Some("unquote") => {
                            unq[1].clone()
                        }
                        other => other.clone(),
                    }).collect();
                    if parts.len() == 1 {
                        desugar_expr(&parts[0])
                    } else {
                        desugar_application(&parts)
                    }
                }
            }
        }
    }
}

/// Regular function application: (f x) or (f x y ...)
fn desugar_application(items: &[Sexp]) -> Result<Expr, String> {
    let func = desugar_expr(&items[0])?;
    let args: Vec<Expr> = items[1..]
        .iter()
        .map(desugar_expr)
        .collect::<Result<_, _>>()?;
    if args.len() == 1 {
        Ok(Expr::App(Box::new(func), Box::new(args.into_iter().next().unwrap())))
    } else if args.is_empty() {
        Ok(func)
    } else {
        Ok(Expr::AppN(Box::new(func), args))
    }
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
            Expr::Lambdas(params, body) => {
                assert_eq!(params, &["n", "m"]);
                match body.as_ref() {
                    Expr::AppN(_, args) => {
                        assert_eq!(args.len(), 2);
                    }
                    other => panic!("expected AppN, got {:?}", other),
                }
            }
            other => panic!("expected Lambdas, got {:?}", other),
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
    fn desugar_quasiquote_int_head() {
        // `(0) — Rocq extraction of nat O
        let defs = desugar("(define z (lambda (x) `(0)))");
        assert_eq!(defs.len(), 1);
        match &defs[0].body {
            Expr::Lambda(_, body) => {
                assert_eq!(body.as_ref(), &Expr::Int(0));
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn desugar_quasiquote_lambda_head() {
        // `((lambda (n) (+ n 1)) ,x) — Rocq extraction of nat S
        let defs = desugar("(define s (lambda (x) `((lambda (n) (+ n 1)) ,x)))");
        assert_eq!(defs.len(), 1);
        match &defs[0].body {
            Expr::Lambda(_, body) => match body.as_ref() {
                Expr::App(f, _arg) => match f.as_ref() {
                    Expr::Lambda(param, _) => assert_eq!(param, "n"),
                    other => panic!("expected Lambda, got {:?}", other),
                },
                other => panic!("expected App, got {:?}", other),
            },
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn desugar_quasiquote_nested_nat() {
        // `((lambda (n) (+ n 1)) ,`(0)) = S O = 1
        let defs = desugar("(define one (lambda (x) `((lambda (n) (+ n 1)) ,`(0))))");
        assert_eq!(defs.len(), 1);
        match &defs[0].body {
            Expr::Lambda(_, body) => match body.as_ref() {
                Expr::App(f, arg) => {
                    match f.as_ref() {
                        Expr::Lambda(param, _) => assert_eq!(param, "n"),
                        other => panic!("expected Lambda, got {:?}", other),
                    }
                    assert_eq!(arg.as_ref(), &Expr::Int(0));
                }
                other => panic!("expected App, got {:?}", other),
            },
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn desugar_full_hash_forest() {
        let src = std::fs::read_to_string("../../scheme/hash_forest.scm").unwrap();
        let sexps = parse(&src).unwrap();
        let defs = desugar_program(&sexps).unwrap();
        assert!(defs.len() > 20);
    }
}
