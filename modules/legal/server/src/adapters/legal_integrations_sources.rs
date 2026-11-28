//! Native-owned Legal integrations read contract.
//!
//! The task list, pending approvals and calendar reads were the last TypeScript
//! statement tokens in legal/integrations.ts. The exact templates now live in
//! this owner's JSON contract, bound into Rust with `include_str!` and imported
//! by the TypeScript adapter. Callers supply only trusted feature tables, the
//! Workflow projection fragments and the owner-built visibility and purchase
//! predicates.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned integrations read contract.
pub const LEGAL_INTEGRATIONS_SOURCES_JSON: &str = include_str!("legal-integrations-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalIntegrationsSources {
    pub tasks: String,
    pub approvals: String,
    #[serde(rename = "calendarBase")]
    pub calendar_base: String,
    #[serde(rename = "calendarFeature")]
    pub calendar_feature: String,
    #[serde(rename = "calendarRows")]
    pub calendar_rows: String,
}

static SOURCES: OnceLock<LegalIntegrationsSources> = OnceLock::new();

pub fn legal_integrations_sources() -> &'static LegalIntegrationsSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_INTEGRATIONS_SOURCES_JSON)
            .expect("legal integrations SQL contract must be valid JSON")
    })
}

/// Render the owned task list for one trusted feature table.
pub fn render_legal_tasks(table: &str, visibility: &str, purchase: &str) -> String {
    legal_integrations_sources()
        .tasks
        .replace("{table}", table)
        .replace("{visibility}", visibility)
        .replace("{purchase}", purchase)
}

/// Render the pending approvals read with the Workflow projection fragments.
pub fn render_legal_approvals(
    approval_instances: &str,
    approval_steps: &str,
    visibility: &str,
    purchase: &str,
) -> String {
    legal_integrations_sources()
        .approvals
        .replace("{approvalInstances}", approval_instances)
        .replace("{approvalSteps}", approval_steps)
        .replace("{visibility}", visibility)
        .replace("{purchase}", purchase)
}

pub fn legal_calendar_base_sql() -> &'static str {
    &legal_integrations_sources().calendar_base
}

/// Render one calendar feature source for a trusted feature table.
pub fn render_legal_calendar_feature(table: &str) -> String {
    legal_integrations_sources()
        .calendar_feature
        .replace("{table}", table)
}

/// Render the calendar rows over the assembled source union.
pub fn render_legal_calendar_rows(sources: &str, visibility: &str, purchase: &str) -> String {
    legal_integrations_sources()
        .calendar_rows
        .replace("{sources}", sources)
        .replace("{visibility}", visibility)
        .replace("{purchase}", purchase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn digest(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn contract_is_byte_exact_and_read_only() {
        assert_eq!(
            digest(LEGAL_INTEGRATIONS_SOURCES_JSON),
            "ba25c8d2234a49630ba7df9caaa14b5c47642700a28da066c7872749a7b24e13"
        );
        let tasks = render_legal_tasks("legal_obligations", "v=?", "p");
        assert_eq!(
            tasks,
            "SELECT f.*,i.title record_title FROM legal_obligations f JOIN legal_items i ON i.id=f.item_id WHERE v=? AND p AND f.owner_id=? AND f.state NOT IN ('completed','closed','cancelled','acknowledged','waived','accepted','billed') AND NOT(f.category='custody' AND f.state='received') ORDER BY f.due_date NULLS LAST,f.id LIMIT 200"
        );
        let approvals = render_legal_approvals("instances", "steps", "v=?", "p");
        assert_eq!(
            approvals,
            "SELECT i.* FROM legal_items i JOIN (instances) ai ON ai.record_type='legal' AND ai.record_id=i.id AND ai.state='pending' JOIN (steps) st ON st.instance_id=ai.id AND st.ordinal=ai.current_step WHERE v=? AND p AND iam_same_person(st.approver_user_id,?,i.company_id) ORDER BY i.updated_at DESC LIMIT 200"
        );
        assert_eq!(
            legal_calendar_base_sql(),
            "SELECT id,id item_id,title,description,due_date,timezone,department_id,branch_id,owner_id,state,version,company_id,classification,kind,created_by FROM legal_items"
        );
        assert_eq!(
            render_legal_calendar_feature("legal_matters"),
            "SELECT f.id,i.id item_id,f.title,i.description,f.due_date,i.timezone,i.department_id,i.branch_id,f.owner_id,f.state,f.version,i.company_id,i.classification,i.kind,i.created_by FROM legal_matters f JOIN legal_items i ON i.id=f.item_id"
        );
        let rows = render_legal_calendar_rows("base UNION ALL feature", "v=?", "p");
        assert_eq!(
            rows,
            "SELECT i.* FROM (base UNION ALL feature) i WHERE v=? AND p AND i.due_date>=? AND i.due_date<? AND i.state NOT IN ('archived','cancelled') AND (?='team' OR i.owner_id=?) AND (?='' OR strpos(lower(i.title),lower(?))>0) ORDER BY i.id LIMIT ?"
        );
        assert_eq!(tasks.matches('?').count(), 2);
        assert_eq!(approvals.matches('?').count(), 2);
        assert_eq!(rows.matches('?').count(), 8);
        for sql in [tasks.as_str(), approvals.as_str(), rows.as_str()] {
            for write in [
                "INSERT INTO",
                "DELETE FROM",
                "UPDATE ",
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
                "pg_advisory",
            ] {
                assert!(!sql.contains(write), "{write}");
            }
        }
    }

    #[test]
    fn render_preserves_trusted_fragments_and_leaves_no_placeholders() {
        let tasks = render_legal_tasks("t.$&", "v.$1", "p.$2");
        assert!(tasks.contains("FROM t.$& f"));
        assert!(tasks.contains("v.$1"));
        assert!(tasks.contains("p.$2"));
        for token in ["{table}", "{visibility}", "{purchase}"] {
            assert!(!tasks.contains(token), "{token}");
        }
        let approvals = render_legal_approvals("i.$&", "s.$1", "v.$2", "p.$3");
        assert!(approvals.contains("(i.$&) ai"));
        assert!(approvals.contains("(s.$1) st"));
        assert!(approvals.contains("v.$2"));
        assert!(approvals.contains("p.$3"));
        for token in [
            "{approvalInstances}",
            "{approvalSteps}",
            "{visibility}",
            "{purchase}",
        ] {
            assert!(!approvals.contains(token), "{token}");
        }
        let rows = render_legal_calendar_rows("s.$&", "v.$1", "p.$2");
        assert!(rows.contains("(s.$&) i"));
        assert!(rows.contains("v.$1"));
        assert!(rows.contains("p.$2"));
        for token in ["{sources}", "{visibility}", "{purchase}"] {
            assert!(!rows.contains(token), "{token}");
        }
        assert!(!render_legal_calendar_feature("t.$&").contains("{table}"));
    }
}
