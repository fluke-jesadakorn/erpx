//! Native-owned Projects evaluation read contract.
//!
//! The source-identity UNION and the two People evaluation project reads were
//! the last fixed read statements in `evaluation-source-policy.ts` and
//! `people-evaluation-facts.ts`. Their exact templates now live in this owner's
//! JSON contract, bound into Rust with `include_str!` and imported by the
//! TypeScript adapters. Callers supply only the owner-built visibility scope for
//! the two People reads; the identity UNION takes no binds. Every read stays on
//! the caller's already-open unit and carries no receipt/audit/outbox ledger.

/// Native-owned evaluation read contract bytes.
pub const EVALUATION_READ_SOURCES_JSON: &str =
    include_str!("evaluation-read-sources.json");

#[derive(serde::Deserialize)]
struct EvaluationReadSources {
    #[serde(rename = "nativeSources")]
    native_sources: String,
    #[serde(rename = "peopleSourceImage")]
    people_source_image: String,
    #[serde(rename = "peopleSourceRows")]
    people_source_rows: String,
}

static SOURCES: std::sync::OnceLock<EvaluationReadSources> = std::sync::OnceLock::new();

fn sources() -> &'static EvaluationReadSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(EVALUATION_READ_SOURCES_JSON)
            .expect("projects evaluation read SQL contract must be valid JSON")
    })
}

/// Frozen source-identity UNION with no positional binds.
pub fn native_sources() -> &'static str {
    &sources().native_sources
}

/// Render the People evaluation project image for one owner-built scope.
pub fn source_image(scope: &str) -> String {
    sources().people_source_image.replace("{scope}", scope)
}

/// Render the People evaluation paged project rows for one owner-built scope.
pub fn source_rows(scope: &str) -> String {
    sources().people_source_rows.replace("{scope}", scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE_SOURCES: &str = concat!(
        "SELECT id,id project_id,company_id,'project' kind,'projects.wbs' capability,NULL::text source_user_id FROM staff_projects\n",
        " UNION ALL SELECT id,project_id,company_id,'milestone','projects.wbs',NULL::text FROM staff_project_milestones\n",
        " UNION ALL SELECT id,project_id,company_id,'work_log','projects.time',user_id FROM staff_project_work_logs\n",
        " UNION ALL SELECT id,project_id,company_id,'allocation','projects.costs',NULL::text FROM staff_project_cost_allocations\n",
        " UNION ALL SELECT id,project_id,company_id,'handoff','projects.handover',NULL::text FROM staff_project_handoffs\n",
        " UNION ALL SELECT id,project_id,company_id,'task','projects.tasks',NULL::text FROM staff_tasks WHERE project_id IS NOT NULL",
    );

    const PEOPLE_SOURCE_IMAGE: &str = "SELECT jsonb_build_object('projects',(SELECT jsonb_agg(to_jsonb(evaluation_project)||jsonb_build_object('visible',{scope}) ORDER BY evaluation_project.id) FROM staff_projects evaluation_project WHERE evaluation_project.company_id=?),'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.project_id,m.user_id) FROM staff_project_members m JOIN staff_projects p ON p.id=m.project_id WHERE p.company_id=?))::text image";

    const PEOPLE_SOURCE_ROWS: &str = "SELECT evaluation_project.id,evaluation_project.title,evaluation_project.version,evaluation_project.updated_at FROM staff_projects evaluation_project JOIN staff_project_members m ON m.project_id=evaluation_project.id WHERE evaluation_project.company_id=? AND m.user_id=? AND (?::text IS NULL OR evaluation_project.id=?) AND (?::text IS NULL OR evaluation_project.branch_id=?) AND (?::text IS NULL OR evaluation_project.department_id=?) AND {scope} ORDER BY evaluation_project.updated_at DESC LIMIT 100";

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literals() {
        assert_eq!(native_sources(), NATIVE_SOURCES);
        assert_eq!(native_sources().len(), 695);
        assert_eq!(sources().people_source_image, PEOPLE_SOURCE_IMAGE);
        assert_eq!(sources().people_source_image.len(), 412);
        assert_eq!(sources().people_source_rows, PEOPLE_SOURCE_ROWS);
        assert_eq!(sources().people_source_rows.len(), 501);
    }

    #[test]
    fn every_contract_statement_is_a_read_only_select() {
        for sql in [native_sources(), PEOPLE_SOURCE_IMAGE, PEOPLE_SOURCE_ROWS] {
            let upper = sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT "), "{sql}");
            assert!(!upper.contains("INSERT INTO"), "{sql}");
            assert!(!upper.contains("DELETE FROM"), "{sql}");
            assert!(!upper.contains("UPDATE "), "{sql}");
            assert!(!upper.contains("FOR UPDATE"), "{sql}");
            assert!(!upper.contains("FOR SHARE"), "{sql}");
            assert!(!upper.contains("LOCK TABLE"), "{sql}");
            assert!(!upper.contains("PG_ADVISORY"), "{sql}");
        }
        assert_eq!(native_sources().matches('?').count(), 0);
        assert_eq!(native_sources().matches("SELECT ").count(), 6);
        assert_eq!(PEOPLE_SOURCE_IMAGE.matches("SELECT ").count(), 3);
        assert_eq!(PEOPLE_SOURCE_ROWS.matches("SELECT ").count(), 1);
    }

    #[test]
    fn scope_templates_render_once_and_never_accept_sql_text() {
        for (template, render) in [
            (PEOPLE_SOURCE_IMAGE, source_image as fn(&str) -> String),
            (PEOPLE_SOURCE_ROWS, source_rows as fn(&str) -> String),
        ] {
            assert_eq!(template.matches("{scope}").count(), 1);
            let rendered = render("evaluation_project.company_id=? AND TRUE");
            assert!(rendered.contains("evaluation_project.company_id=? AND TRUE"));
            assert!(!rendered.contains("{scope}"));
            assert_eq!(rendered.matches("SELECT ").count(), template.matches("SELECT ").count());
        }
    }

    #[test]
    fn typescript_adapters_carry_no_retired_statement_tokens() {
        let policy = include_str!("evaluation-source-policy.ts");
        assert!(!policy.contains("SELECT id,id project_id"));
        assert!(!policy.contains("UNION ALL ${nativeSourceColumns}"));
        assert!(!policy.contains("selectBodySql("));
        assert!(policy.contains("evaluation-read-sources.json"));
        assert!(policy.contains("contract.nativeSources"));
        assert_eq!(policy.matches("SELECT ").count(), 2);

        let facts = include_str!("people-evaluation-facts.ts");
        assert!(!facts.contains("SELECT "));
        assert!(facts.contains("evaluation-read-sources.json"));
        assert!(facts.contains("fill(contract.peopleSourceImage"));
        assert!(facts.contains("fill(contract.peopleSourceRows"));
    }

    #[test]
    fn public_port_reexports_the_contract() {
        let port = include_str!("../public/evaluation_read_sources.rs");
        assert!(port.contains("pub use crate::adapters::evaluation_read_sources::"));
        for symbol in [
            "native_sources",
            "source_image",
            "source_rows",
            "EVALUATION_READ_SOURCES_JSON",
        ] {
            assert!(port.contains(symbol));
        }
    }
}
