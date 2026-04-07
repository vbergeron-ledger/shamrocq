pub mod debruijn;
pub mod p00_inline;
pub mod p01_beta_reduce;
pub mod p01b_case_nat;
pub mod p02_constant_fold;
pub mod p03_if_to_match;
pub mod p04_dead_binding;
pub mod p04b_contify;
pub mod p05_case_known_ctor;
pub mod p06_eta_reduce;
pub mod p07_arity_analysis;
pub mod p07b_arity_specialize;
pub mod p08_anf;

use std::collections::HashMap;

use crate::desugar::Define;
use crate::resolve::RDefine;

pub trait ExprPass {
    fn name(&self) -> &'static str;
    fn run(&self, defs: Vec<Define>) -> Vec<Define>;
}

pub trait SingleExprPass {
    fn name(&self) -> &'static str;
    fn run(&self, def: Define) -> Define;
}

impl<T: SingleExprPass> ExprPass for T {
    fn name(&self) -> &'static str {
        SingleExprPass::name(self)
    }
    fn run(&self, defs: Vec<Define>) -> Vec<Define> {
        defs.into_iter().map(|d| SingleExprPass::run(self, d)).collect()
    }
}

pub trait ResolvedPass {
    fn name(&self) -> &'static str;
    fn run(&self, defs: Vec<RDefine>) -> Vec<RDefine>;
}

pub trait SingleResolvedPass {
    fn name(&self) -> &'static str;
    fn run(&self, def: RDefine) -> RDefine;
}

impl<T: SingleResolvedPass> ResolvedPass for T {
    fn name(&self) -> &'static str {
        SingleResolvedPass::name(self)
    }
    fn run(&self, defs: Vec<RDefine>) -> Vec<RDefine> {
        defs.into_iter().map(|d| SingleResolvedPass::run(self, d)).collect()
    }
}

/// Per-pass enable/disable overrides parsed from `--pass:name=yes/no`.
#[derive(Default, Clone)]
pub struct PassConfig {
    overrides: HashMap<String, bool>,
}

impl PassConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &str, enabled: bool) {
        self.overrides.insert(name.to_string(), enabled);
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.overrides.get(name).copied().unwrap_or(true)
    }

    /// Parse a `--pass:name=yes|no` flag. Returns an error message on bad syntax.
    pub fn parse_flag(flag: &str) -> Result<(String, bool), String> {
        let rest = flag.strip_prefix("--pass:")
            .ok_or_else(|| format!("expected --pass:NAME=yes/no, got {}", flag))?;
        let (name, val) = rest.split_once('=')
            .ok_or_else(|| format!("expected --pass:NAME=yes/no, got {}", flag))?;
        let enabled = match val {
            "on" => true,
            "off" => false,
            _ => return Err(format!("expected yes/no for --pass:{}, got {}", name, val)),
        };
        Ok((name.to_string(), enabled))
    }

    pub fn all_pass_names() -> Vec<&'static str> {
        let expr: Vec<&str> = expr_passes().iter().map(|p| p.name()).collect();
        let resolved: Vec<&str> = resolved_passes().iter().map(|p| p.name()).collect();
        expr.into_iter().chain(resolved).collect()
    }
}

pub fn expr_passes() -> Vec<Box<dyn ExprPass>> {
    vec![
        Box::new(p00_inline::InlineSmallGlobals),
        Box::new(p01_beta_reduce::BetaReduce),
        Box::new(p01b_case_nat::CaseNat),
        Box::new(p02_constant_fold::ConstantFold),
        Box::new(p03_if_to_match::IfToMatch),
    ]
}

pub fn resolved_passes() -> Vec<Box<dyn ResolvedPass>> {
    vec![
        Box::new(p04_dead_binding::DeadBindingElim),
        Box::new(p04b_contify::Contify),
        Box::new(p05_case_known_ctor::CaseOfKnownCtor),
        Box::new(p06_eta_reduce::EtaReduce),
        Box::new(p07_arity_analysis::ArityAnalysis),
        Box::new(p07b_arity_specialize::AritySpecialize),
        Box::new(p08_anf::AnfNormalize),
    ]
}
