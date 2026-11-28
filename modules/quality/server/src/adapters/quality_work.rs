//! Injectable source gates for the owner-side Quality work projection.
//!
//! The projection itself is portable application policy in
//! `application::quality`. This module only supplies adapters callers can
//! inject: a closure over facts already read in their unit, and a mapped set
//! of per-hold facts for readers that prepared them together with the holds.
use crate::application::quality::{QualitySourceAccess, SourceAccess};
use erpx_platform_shared_server::public::BusinessError;
use std::collections::BTreeMap;

/// Closure-backed source gate. `None` keeps the work item unchanged, matching
/// the retained `qualitySourceAccess` disabled-enforcement branch.
pub struct FnSourceAccess<F> {
    access: F,
}

impl<F> FnSourceAccess<F>
where
    F: Fn(&str) -> Option<QualitySourceAccess> + Send + Sync,
{
    pub fn new(access: F) -> Self {
        Self { access }
    }
}

impl<F> SourceAccess for FnSourceAccess<F>
where
    F: Fn(&str) -> Option<QualitySourceAccess> + Send + Sync,
{
    fn source_access(&self, hold_id: &str) -> Result<Option<QualitySourceAccess>, BusinessError> {
        Ok((self.access)(hold_id))
    }
}

/// Per-hold source facts prepared in the caller's unit. A hold without an
/// entry has no source gate, not a denial.
#[derive(Clone, Debug, Default)]
pub struct MappedSourceAccess {
    pub accesses: BTreeMap<String, Option<QualitySourceAccess>>,
}

impl MappedSourceAccess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, hold_id: &str, access: Option<QualitySourceAccess>) -> Self {
        self.accesses.insert(hold_id.to_owned(), access);
        self
    }
}

impl SourceAccess for MappedSourceAccess {
    fn source_access(&self, hold_id: &str) -> Result<Option<QualitySourceAccess>, BusinessError> {
        Ok(self.accesses.get(hold_id).cloned().flatten())
    }
}
