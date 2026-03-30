pub mod p00_inline;
pub mod p01_beta_reduce;
pub mod p02_constant_fold;
pub mod p03_if_to_match;
pub mod p04_dead_binding;
pub mod p05_case_known_ctor;
pub mod p06_eta_reduce;
pub mod p07_arity_analysis;
pub mod p08_anf;

use crate::desugar::Define;
use crate::resolve::RDefine;

pub trait ExprPass {
    fn name(&self) -> &'static str;
    fn run(&self, defs: Vec<Define>) -> Vec<Define>;
}

pub trait ResolvedPass {
    fn name(&self) -> &'static str;
    fn run(&self, defs: Vec<RDefine>) -> Vec<RDefine>;
}

pub fn expr_passes() -> Vec<Box<dyn ExprPass>> {
    vec![
        Box::new(p00_inline::InlineSmallGlobals),
        Box::new(p01_beta_reduce::BetaReduce),
        Box::new(p02_constant_fold::ConstantFold),
        Box::new(p03_if_to_match::IfToMatch),
    ]
}

pub fn resolved_passes() -> Vec<Box<dyn ResolvedPass>> {
    vec![
        Box::new(p04_dead_binding::DeadBindingElim),
        Box::new(p05_case_known_ctor::CaseOfKnownCtor),
        Box::new(p06_eta_reduce::EtaReduce),
        Box::new(p07_arity_analysis::ArityAnalysis),
        Box::new(p08_anf::AnfNormalize),
    ]
}
