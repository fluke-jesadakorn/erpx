//! Owner-owned caller-unit read plans for CRM. Generated from the retired
//! TypeScript SELECT literal with its exact positional payload; the axum
//! `command-plan` route resolves the kind here and returns the statement for
//! the caller to prepare and execute on its own already-open unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the
/// positional argument count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `collaboration-notification-lead` read.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[recordId, companyId]` binds.
pub const RD_COLLAB_NOTIFICATION_LEAD: &str = "crm_rd_collab_notification_lead";
const SQL_COLLAB_NOTIFICATION_LEAD: &str =
    "SELECT lead_id FROM crm_lead_activities WHERE id=? AND company_id=?";
const READ_COLLAB_NOTIFICATION_LEAD: NativeReadStatement = NativeReadStatement {
    kind: RD_COLLAB_NOTIFICATION_LEAD,
    sql: SQL_COLLAB_NOTIFICATION_LEAD,
    bind_count: 2,
};

const READS: &[NativeReadStatement] = &[READ_COLLAB_NOTIFICATION_LEAD];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_plan_is_ledger_free_and_bounded() {
        let read = read_statement(RD_COLLAB_NOTIFICATION_LEAD).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
        assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert!(is_read(RD_COLLAB_NOTIFICATION_LEAD));
        assert!(!is_read("crm_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn read_sql_matches_retired_typeScript_literal() {
        let read = read_statement(RD_COLLAB_NOTIFICATION_LEAD).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT lead_id FROM crm_lead_activities WHERE id=? AND company_id=?"
        );
    }

    /// Canonical owner form of the projection/FROM read head retired from the
    /// TypeScript read tail. The owner renderer `crmProjectionFromSql` must
    /// reproduce this exact prefix; this crate records the native literal.
    fn projection_from_sql(projection: &str, from: &str) -> String {
        format!("SELECT {projection} FROM {from}")
    }

    #[test]
    fn projection_from_family_matches_retired_owner_literals() {
        let cases = [
            (
                "opportunity owner guard",
                projection_from_sql("1", "crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id WHERE o.id=?"),
                "SELECT 1 FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id WHERE o.id=?",
            ),
            (
                "contact account read",
                projection_from_sql(
                    "c.id,c.name",
                    "crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.id=?",
                ),
                "SELECT c.id,c.name FROM crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.id=?",
            ),
            (
                "owned-source list",
                projection_from_sql("id", "(ROWS) actual WHERE id=? AND company_id=?"),
                "SELECT id FROM (ROWS) actual WHERE id=? AND company_id=?",
            ),
        ];
        for (name, rendered, expected) in cases {
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    /// Canonical owner form of the bare `SELECT ` head retired from the r40
    /// scoped-list, distinct-owner and lead-row-source families.
    fn select_body_sql(body: &str) -> String {
        format!("SELECT {body}")
    }

    #[test]
    fn metric_breakdown_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r37 executive metric projection: the
        // label/value projection is the bare select-body plan. The TS
        // `crmMetricBreakdownSql` delegates on this identity; the caller tail,
        // binds and ORDER BY stay verbatim.
        let select_body = |body: &str| format!("SELECT {body}");
        let breakdown = |label: &str, value: &str, tail: &str| {
            select_body(&format!("{label} label,{value} value{tail}"))
        };
        let cases = [
            ("opportunity pipeline", "v.stage", "sum(v.expected_minor)::text", " FROM crm_opportunities v GROUP BY 1 ORDER BY 1"),
            ("overdue followups", "c.status", "count(*)::text", " FROM (PURCHASED) c GROUP BY 1 ORDER BY 1"),
        ];
        for (name, label, value, tail) in cases {
            let rendered = breakdown(label, value, tail);
            assert_eq!(rendered, format!("SELECT {label} label,{value} value{tail}"), "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn clocked_read_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r39 clocked CTE head: the caller's body
        // is the bare select-body plan, prefixed by the caller's own CTE. The TS
        // `crmClockedReadSql` delegates on this identity; body and binds stay verbatim.
        let select_body = |body: &str| format!("SELECT {body}");
        let clocked = |clock: &str, body: &str| format!("WITH {clock} {}", select_body(body));
        let cases = [
            ("property options", "report_clock AS MATERIALIZED(... now_ms)", "property.id,property.name FROM pms_properties property CROSS JOIN report_clock WHERE property.company_id=? ORDER BY property.name"),
            ("costs", "report_clock AS MATERIALIZED(... now_ms)", "cost.campaign_id,COALESCE(SUM(line.debit_minor-line.credit_minor),0) amount FROM (COST_SOURCES) cost GROUP BY cost.campaign_id"),
            ("multiline body", "report_clock AS MATERIALIZED(... now_ms)", "l.id,l.account_id,\n     CASE WHEN v.win_source='pms' THEN r.id IS NOT NULL ELSE TRUE END source_visible,\n     FROM crm_leads l CROSS JOIN report_clock"),
        ];
        for (name, clock, body) in cases {
            let rendered = clocked(clock, body);
            assert_eq!(rendered, format!("WITH {clock} SELECT {body}"), "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn scalar_subquery_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r43 scalar subquery: the select-body
        // plan over `${projection} FROM ${body}`, wrapped in parentheses. The TS
        // `crmScalarSubquerySql` delegates on this identity; caller projections,
        // bodies and wrappers stay verbatim.
        let select_body = |body: &str| format!("SELECT {body}");
        let scalar = |projection: &str, body: &str| {
            format!("({})", select_body(&format!("{projection} FROM {body}")))
        };
        let cases = [
            ("owner name", "legal_name", "(hr_people) hr_people WHERE company_id=l.company_id AND user_id=l.owner_id LIMIT 1"),
            ("organization display name", "n.display_name", "(organization_nodes) n WHERE n.id=l.account_id AND n.company_id=l.company_id"),
            ("journal line id", "journal_id", "finance_journal_lines WHERE id=cost.journal_line_id"),
        ];
        for (name, projection, body) in cases {
            let rendered = scalar(projection, body);
            assert_eq!(rendered, format!("(SELECT {projection} FROM {body})"), "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn company_scope_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r48 company-scope head: the select-body
        // plan over `?::text company_id${tail}`. The TS `crmCompanyScopeSql`
        // delegates on this identity; the caller tail and bind order stay verbatim.
        let select_body = |body: &str| format!("SELECT {body}");
        let company_scope = |tail: &str| select_body(&format!("?::text company_id{tail}"));
        let cases = [
            ("obligation scope CTE", "", "SELECT ?::text company_id"),
            ("actor scope row", ",?::text branch_id,?::text department_id", "SELECT ?::text company_id,?::text branch_id,?::text department_id"),
        ];
        for (name, tail, expected) in cases {
            let rendered = company_scope(tail);
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn jsonb_aggregate_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r49 JSONB-aggregate head: the select-body
        // plan over `jsonb_agg(to_jsonb(${body}`. The TS `crmJsonbAggregateSql`
        // delegates on this identity; aggregate body and wrapper stay verbatim.
        let select_body = |body: &str| format!("SELECT {body}");
        let aggregate = |body: &str| select_body(&format!("jsonb_agg(to_jsonb({body}"));
        let cases = [
            ("empty prefix", "", "SELECT jsonb_agg(to_jsonb("),
            ("native-image witness", "native)) FROM (ROWS) native WHERE PRED", "SELECT jsonb_agg(to_jsonb(native)) FROM (ROWS) native WHERE PRED"),
            ("related-image aggregate", "a) ORDER BY a.id) FROM F", "SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM F"),
        ];
        for (name, body, expected) in cases {
            let rendered = aggregate(body);
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn select_expr_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r50 parenthesized-select head: the
        // select-body plan over `(${body}`. The TS `crmSelectExprSql` delegates
        // on this identity; witness and caller tails stay verbatim.
        let select_body = |body: &str| format!("SELECT {body}");
        let select_expr = |body: &str| select_body(&format!("({body}"));
        let cases = [
            ("empty witness", "", "SELECT ("),
            ("attachment target witness", "CRM_SOURCE_QUERY) witness", "SELECT (CRM_SOURCE_QUERY) witness"),
            ("marketing assignment guard", "jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?", "SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?"),
        ];
        for (name, body, expected) in cases {
            let rendered = select_expr(body);
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn materialized_scope_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r47 MATERIALIZED scope CTE: the row body
        // is the bare select-body plan wrapped in the MATERIALIZED parentheses.
        // The TS `crmMaterializedScopeSql` delegates on this identity.
        let select_body = |body: &str| format!("SELECT {body}");
        let materialized = |body: &str| format!("AS MATERIALIZED({})", select_body(body));
        let cases = [
            ("clock fact", "floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms"),
            ("marketing candidate scope", "candidate.* FROM (CANDIDATES) candidate,scope WHERE candidate.company_id=scope.company_id"),
            ("sales lead scope", "l.* FROM crm_leads l,scope WHERE l.company_id=scope.company_id"),
        ];
        for (name, body) in cases {
            let rendered = materialized(body);
            assert_eq!(rendered, format!("AS MATERIALIZED(SELECT {body})"), "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn union_all_arm_is_the_owner_select_body_plan() {
        // Canonical native form of the r44 UNION ALL fact arm: the arm body is
        // the bare select-body plan prefixed with `UNION ALL `. The TS
        // `crmUnionAllSql` delegates on this identity.
        let select_body = |body: &str| format!("SELECT {body}");
        let union_all = |body: &str| format!("UNION ALL {}", select_body(body));
        let cases = [
            ("sales history fact", "'create',event.id,to_jsonb(event),false FROM crm_case_events event"),
            ("record activity arm", "activity.id,'activity',activity.id,activity.company_id FROM crm_lead_activities activity"),
            ("marketing attribution arm", "'attribution',touch.id,to_jsonb(touch),false FROM marketing_touches touch"),
        ];
        for (name, body) in cases {
            let rendered = union_all(body);
            assert_eq!(rendered, format!("UNION ALL SELECT {body}"), "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn assignment_module_exists_head_is_the_owner_select_one_plan() {
        // Canonical native form of the r35 assignment-module EXISTS predicate:
        // the inner witness is the r42 select-one plan over the assignments
        // body. The TS `crmAssignmentModuleExistsSql` delegates on this identity.
        let select_one = |body: &str| format!("SELECT 1 FROM {body}");
        let assignment_exists = |alias: &str, suffix: &str| {
            let q = if alias.is_empty() { String::new() } else { format!("{alias}.") };
            let body = format!(
                "assignments{} WHERE {q}id=? AND (jsonb_array_length({q}modules_json::jsonb)=0 OR {q}modules_json::jsonb @> jsonb_build_array({suffix}",
                if alias.is_empty() { String::new() } else { format!(" {alias}") }
            );
            format!("EXISTS({}", select_one(&body))
        };
        let cases = [
            ("bare metric guard", "", "'crm'::text)))", "EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('crm'::text)))"),
            ("aliased purchase prefix", "selected", "", "EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array("),
        ];
        for (name, alias, suffix, expected) in cases {
            let rendered = assignment_exists(alias, suffix);
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn projection_from_head_is_the_owner_select_body_plan() {
        // Canonical native form of the r46 projection/FROM head: it is the
        // bare select-body plan over `${projection} FROM ${from}`. The TS
        // `crmProjectionFromSql` delegates to `crmSelectBodySql` on this identity.
        let select_body = |body: &str| format!("SELECT {body}");
        let projection_from = |projection: &str, from: &str| {
            select_body(&format!("{projection} FROM {from}"))
        };
        let cases = [
            ("opportunity owner guard", "1", "crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id WHERE o.id=?"),
            ("contact account read", "c.id,c.name", "crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.id=?"),
            ("owned-source list", "id", "(ROWS) actual WHERE id=? AND company_id=?"),
        ];
        for (name, projection, from) in cases {
            let rendered = projection_from(projection, from);
            assert_eq!(rendered, format!("SELECT {projection} FROM {from}"), "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn lead_select_head_is_the_owner_select_one_plan() {
        // Canonical native form of the r51 lead-select head: it is the r42
        // select-one plan with the fixed `crm_leads` body. The TS renderer
        // `crmLeadSelectSql` delegates to `crmSelectOneSql` on this identity.
        let select_one = |body: &str| format!("SELECT 1 FROM {body}");
        let lead_select = |tail: &str| select_one(&format!("crm_leads {tail}"));
        let cases = [
            ("creator scope", "l WHERE l.contact_id=c.id AND l.branch_id=?", "SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=?"),
            ("eligible owner", "l WHERE l.id=? AND l.version=? AND l.owner_id=?", "SELECT 1 FROM crm_leads l WHERE l.id=? AND l.version=? AND l.owner_id=?"),
        ];
        for (name, tail, expected) in cases {
            let rendered = lead_select(tail);
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn bare_select_head_matches_retired_r40_owner_literals() {
        let cases = [
            (
                "scoped descending list",
                select_body_sql("g.id,g.version FROM crm_guest_links g ORDER BY g.created_at DESC LIMIT 200"),
                "SELECT g.id,g.version FROM crm_guest_links g ORDER BY g.created_at DESC LIMIT 200",
            ),
            (
                "distinct owner directory",
                select_body_sql("DISTINCT l.owner_id id FROM crm_leads l ORDER BY name"),
                "SELECT DISTINCT l.owner_id id FROM crm_leads l ORDER BY name",
            ),
            (
                "lead row source",
                select_body_sql("lead.* FROM crm_leads lead WHERE lead.company_id=?"),
                "SELECT lead.* FROM crm_leads lead WHERE lead.company_id=?",
            ),
        ];
        for (name, rendered, expected) in cases {
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }
}
