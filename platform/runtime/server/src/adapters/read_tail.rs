//! Native-owned SQL contract for the runtime read-tail builders and report
//! guards.
//!
//! The seven closed projection shapes and the two report guards previously
//! lived as inline TypeScript literals. Their exact bytes now live in
//! `read-tail-sources.json`, bound here with `include_str!` and read by the
//! TypeScript adapters through the same contract. A caller substitutes only
//! its closed column list and verbatim suffix; no relation or predicate is
//! accepted from an untrusted caller. Lock and write statements stay out of
//! this contract.

/// Native-owned contract for the seven read-tail builders and two guards.
pub const READ_TAIL_SOURCES_JSON: &str = include_str!("read-tail-sources.json");

#[derive(serde::Deserialize)]
struct Sources {
    #[serde(rename = "holdingBindings")]
    holding_bindings: String,
    #[serde(rename = "executiveGenerationJobs")]
    executive_generation_jobs: String,
    #[serde(rename = "runtimeHealth")]
    runtime_health: String,
    #[serde(rename = "executiveExternalTokens")]
    executive_external_tokens: String,
    #[serde(rename = "executiveReportShares")]
    executive_report_shares: String,
    #[serde(rename = "executiveReportRevisions")]
    executive_report_revisions: String,
    #[serde(rename = "executiveReports")]
    executive_reports: String,
    #[serde(rename = "reportLock")]
    report_lock: String,
    #[serde(rename = "reportVersionGuard")]
    report_version_guard: String,
}

static SOURCES: std::sync::OnceLock<Sources> = std::sync::OnceLock::new();

fn sources() -> &'static Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(READ_TAIL_SOURCES_JSON)
            .expect("runtime read-tail SQL contract must be valid JSON")
    })
}

/// Closed `holding_bindings` projection template.
pub fn holding_bindings() -> &'static str {
    &sources().holding_bindings
}

/// Closed `executive_generation_jobs` projection template.
pub fn executive_generation_jobs() -> &'static str {
    &sources().executive_generation_jobs
}

/// Closed `runtime_health` projection template.
pub fn runtime_health() -> &'static str {
    &sources().runtime_health
}

/// Closed `executive_external_tokens` projection template.
pub fn executive_external_tokens() -> &'static str {
    &sources().executive_external_tokens
}

/// Closed `executive_report_shares` projection template.
pub fn executive_report_shares() -> &'static str {
    &sources().executive_report_shares
}

/// Closed `executive_report_revisions` projection template.
pub fn executive_report_revisions() -> &'static str {
    &sources().executive_report_revisions
}

/// Closed `executive_reports` projection template.
pub fn executive_reports() -> &'static str {
    &sources().executive_reports
}

/// Single-row report lock guard.
pub fn report_lock() -> &'static str {
    &sources().report_lock
}

/// Report ownership/version guard.
pub fn report_version_guard() -> &'static str {
    &sources().report_version_guard
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(value: &str) -> String {
        hex::encode(Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn templates_are_byte_exact_to_the_retired_typescript_builders() {
        for (sql, hash, len, table) in [
            (
                holding_bindings(),
                "d4da402d52732e8f9cd8e3543f184d434af504e4d6c77f701d80f6426a2212f6",
                46,
                "FROM holding_bindings{suffix}",
            ),
            (
                executive_generation_jobs(),
                "e020849b356c816f1a8d7491299686617ac3ec1fa8d9174ef9ac4ac421d079f2",
                55,
                "FROM executive_generation_jobs{suffix}",
            ),
            (
                runtime_health(),
                "eb435fb5a73703cdb575e7fe29099ce6c481b9911e5e1b3cac9531d798599dcd",
                44,
                "FROM runtime_health{suffix}",
            ),
            (
                executive_external_tokens(),
                "c727e6dbf89015e7e1620c10a2995804757c7d0f4eb0e4049390810fb0369d4c",
                55,
                "FROM executive_external_tokens{suffix}",
            ),
            (
                executive_report_shares(),
                "9f35b62c461ad16a8d831e8206fa10bb13c45904c8af698794810f9044ace1be",
                53,
                "FROM executive_report_shares{suffix}",
            ),
            (
                executive_report_revisions(),
                "63498e31e451b1227efcba1aeb279b75b5cde85d2412c999fb0bdfccf23bc0db",
                74,
                "FROM executive_report_revisions WHERE report_id=?{suffix}",
            ),
            (
                executive_reports(),
                "bb2c4aaff57372836aba4c59297562bdec016709447ba16349b0f1df7b95ab45",
                47,
                "FROM executive_reports{suffix}",
            ),
        ] {
            assert_eq!(sha(sql), hash);
            assert_eq!(sql.len(), len);
            assert_eq!(sql.matches("{columns}").count(), 1);
            assert_eq!(sql.matches("{suffix}").count(), 1);
            assert!(sql.starts_with("SELECT {columns} "), "{sql}");
            assert!(sql.ends_with(table), "{sql}");
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("UPDATE "));
        }
    }

    #[test]
    fn report_guards_are_byte_exact_and_bind_closed() {
        let lock = report_lock();
        assert_eq!(
            lock,
            "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"
        );
        assert_eq!(sha(lock), "6bc60d98469950d7d7c62501f49dd1636875a600dbfd77e56d229ede29e35df7");
        assert_eq!(lock.len(), 54);
        assert_eq!(lock.matches('?').count(), 1);

        let guard = report_version_guard();
        assert_eq!(
            guard,
            "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)"
        );
        assert_eq!(sha(guard), "4c7f66216d0c97be6848e31c1b5a4cfd921e6886f00557346c346e076257a6d3");
        assert_eq!(guard.len(), 79);
        assert_eq!(guard.matches('?').count(), 3);
        assert!(guard.starts_with("EXISTS(SELECT 1 "), "{guard}");
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        let helper = include_str!("read-tail-sql.ts");
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE"] {
            assert!(!helper.contains(token), "helper carries {token}");
        }
        assert!(helper.contains("read-tail-sources.json"));
        assert!(helper.contains("{columns}"));
        assert!(helper.contains("{suffix}"));
        assert!(!helper.contains("prepare("));
        assert!(!helper.contains(".bind("));
    }

    #[test]
    fn public_port_reexports_every_frozen_shape() {
        let port = include_str!("../public/read_tail.rs");
        assert!(port.contains("pub use crate::adapters::read_tail::"));
        for symbol in [
            "holding_bindings",
            "executive_generation_jobs",
            "runtime_health",
            "executive_external_tokens",
            "executive_report_shares",
            "executive_report_revisions",
            "executive_reports",
            "report_lock",
            "report_version_guard",
            "READ_TAIL_SOURCES_JSON",
        ] {
            assert!(port.contains(symbol), "port misses {symbol}");
        }
    }
}
