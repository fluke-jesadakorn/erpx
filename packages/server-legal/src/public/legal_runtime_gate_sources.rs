//! Native Legal runtime-gate contract. Callers supply only trusted code-owned
//! company expressions and the scheduled-feature predicate.
pub use crate::adapters::legal_runtime_gate_sources::{
    LEGAL_RUNTIME_GATE_SOURCES_JSON, LegalRuntimeGateSources, legal_runtime_gate_sources,
    legal_runtime_health_sql, render_legal_capability_probe, render_legal_runtime_predicate,
};
