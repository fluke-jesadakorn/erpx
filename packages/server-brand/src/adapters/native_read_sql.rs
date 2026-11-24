//! Owner-local native read ports for the Brand residual SELECT tail.
//!
//! The axum `branding/read-port` ingress resolves one of the kinds below to an
//! exact statement plan; the TypeScript caller executes that plan on its own
//! already-open unit, so no owner SQL literal remains in TypeScript and no SQL
//! is ever accepted from the wire. Reads carry no receipt/audit/outbox ledger.
//! The canonical organization node projection is inlined verbatim from the
//! Organization owner's registered `ORG_NODES_SOURCE` so the planned SQL stays
//! byte-identical to the retired owner literals.

use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::BusinessError;

/// Canonical active organization node projection (Organization owner source).
const ORG_NODES: &str = "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes";

/// A planned owner read. `sql` is the exact statement the retired TypeScript
/// literal bound; `values` is the positional payload in bind order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrandReadStatement {
    pub sql: String,
    pub values: Vec<Value>,
}

/// Discriminators shared with the TypeScript native read client.
pub const RD_METRIC_PROFILES_LABEL: &str = "brand_rd_metric_profiles_label";
pub const RD_METRIC_PROFILES_BRANCH: &str = "brand_rd_metric_profiles_branch";
pub const RD_METRIC_UPDATES_MONTH: &str = "brand_rd_metric_updates_month";
pub const RD_METRIC_UPDATES_BRANCH: &str = "brand_rd_metric_updates_branch";
pub const RD_EVIDENCE_SEARCH: &str = "brand_rd_evidence_search";
pub const RD_MANAGEMENT_BRANCH_EXISTS: &str = "brand_rd_management_branch_exists";
pub const RD_MANAGEMENT_COMPANY_EXISTS: &str = "brand_rd_management_company_exists";
pub const RD_WORKSPACE_CHOICE: &str = "brand_rd_workspace_choice";

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "BRAND_READ_PLAN_INVALID",
        "Use one bounded native Brand read plan.",
    )
}

/// Optional evidence predicates. The caller declares which predicate families
/// are active; the owner builds the matching SQL and consumes binds in order.
#[derive(Clone, Copy, Debug, Default)]
pub struct EvidenceFlags {
    pub range: bool,
    pub query: bool,
    pub cursor: bool,
}

fn as_str(value: &Value) -> Result<&str, BusinessError> {
    value.as_str().ok_or_else(invalid)
}

fn number(value: &Value) -> Result<i64, BusinessError> {
    value.as_i64().ok_or_else(invalid)
}

/// Plan one registered Brand read. `values` is the caller's positional payload;
/// `flags` only selects which owner-controlled predicate families are present.
pub fn plan_brand_read(
    kind: &str,
    values: &[Value],
    flags: EvidenceFlags,
) -> Result<BrandReadStatement, BusinessError> {
    match kind {
        RD_METRIC_PROFILES_LABEL
        | RD_METRIC_PROFILES_BRANCH
        | RD_METRIC_UPDATES_MONTH
        | RD_METRIC_UPDATES_BRANCH => {
            let updates = matches!(kind, RD_METRIC_UPDATES_MONTH | RD_METRIC_UPDATES_BRANCH);
            let expected = if updates { 5 } else { 3 };
            if values.len() != expected {
                return Err(invalid());
            }
            let label = match kind {
                RD_METRIC_PROFILES_BRANCH | RD_METRIC_UPDATES_BRANCH => "b.display_name",
                RD_METRIC_UPDATES_MONTH => {
                    "to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
                }
                _ => "'Configured profiles'",
            };
            let extra = if updates {
                " AND p.updated_at>=? AND p.updated_at<?"
            } else {
                ""
            };
            Ok(BrandReadStatement {
                sql: format!(
                    "SELECT {label} label,count(*)::text value FROM organization_brand_profiles p \
JOIN ({ORG_NODES}) b ON b.id=p.scope_id AND b.company_id=p.company_id AND b.kind='branch' AND b.status='active' \
WHERE p.scope_kind='branch' AND p.company_id=? AND (?::text IS NULL OR p.scope_id=?){extra} GROUP BY 1 ORDER BY 1"
                ),
                values: values.to_vec(),
            })
        }
        RD_EVIDENCE_SEARCH => {
            if values.len() < 4 {
                return Err(invalid());
            }
            // [companyId, branchId, branchId, ...range, ...query, ...cursor, limit]
            let mut tail = 3usize;
            let mut conditions = vec![
                "p.scope_kind='branch'",
                "p.company_id=?",
                "b.kind='branch'",
                "b.status='active'",
                "(?::text IS NULL OR p.scope_id=?)",
            ];
            if flags.range {
                conditions.push("p.updated_at>=?");
                conditions.push("p.updated_at<?");
                tail += 2;
            }
            if flags.query {
                conditions.push("(position(lower(?) in lower(b.display_name))>0 \
OR position(lower(?) in lower(p.document_mode))>0 \
OR position(lower(?) in to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)");
                tail += 3;
            }
            if flags.cursor {
                conditions.push("(b.display_name>? OR (b.display_name=? AND p.scope_id>?))");
                tail += 3;
            }
            if values.len() != tail + 1 {
                return Err(invalid());
            }
            number(values.last().ok_or_else(invalid)?)?;
            Ok(BrandReadStatement {
                sql: format!(
                    "SELECT p.scope_id id,b.display_name branch_name,p.document_mode,p.updated_at,\
p.primary_attachment_id,p.inverse_attachment_id FROM organization_brand_profiles p \
JOIN ({ORG_NODES}) b ON b.id=p.scope_id AND b.company_id=p.company_id \
WHERE {} ORDER BY b.display_name,p.scope_id LIMIT ?",
                    conditions.join(" AND ")
                ),
                values: values.to_vec(),
            })
        }
        RD_MANAGEMENT_BRANCH_EXISTS => {
            if values.len() != 2 {
                return Err(invalid());
            }
            as_str(&values[0])?;
            as_str(&values[1])?;
            Ok(BrandReadStatement {
                sql: format!(
                    "SELECT id,display_name FROM ({ORG_NODES}) organization_nodes \
WHERE id=? AND company_id=? AND kind='branch' AND status='active'"
                ),
                values: values.to_vec(),
            })
        }
        RD_MANAGEMENT_COMPANY_EXISTS => {
            if values.len() != 1 {
                return Err(invalid());
            }
            as_str(&values[0])?;
            Ok(BrandReadStatement {
                sql: format!(
                    "SELECT 1 FROM ({ORG_NODES}) organization_nodes \
WHERE id=? AND kind='company' AND status='active'"
                ),
                values: values.to_vec(),
            })
        }
        RD_WORKSPACE_CHOICE => {
            if values.is_empty() || values.len() > 1000 {
                return Err(invalid());
            }
            for value in values {
                as_str(value)?;
            }
            let placeholders = vec!["?"; values.len()].join(",");
            Ok(BrandReadStatement {
                sql: format!(
                    "SELECT scope_kind,scope_id,primary_publication_id FROM organization_brand_profiles \
WHERE scope_kind='holding' OR company_id IN ({placeholders})"
                ),
                values: values.to_vec(),
            })
        }
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serde_json::json;

    fn v(items: Value) -> Vec<Value> {
        items.as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn metric_kinds_plan_exact_sql_and_binds() {
        let p = plan_brand_read(RD_METRIC_PROFILES_LABEL, &v(json!(["co", "br", "br"])), Default::default()).unwrap();
        assert!(p.sql.contains("'Configured profiles' label,count(*)::text value"));
        assert!(p.sql.contains("p.updated_at") == false);
        assert_eq!(p.values.len(), 3);

        let b = plan_brand_read(RD_METRIC_PROFILES_BRANCH, &v(json!(["co", "br", "br"])), Default::default()).unwrap();
        assert!(b.sql.contains("b.display_name label"));

        let m = plan_brand_read(RD_METRIC_UPDATES_MONTH, &v(json!(["co", "br", "br", 1, 2])), Default::default()).unwrap();
        assert!(m.sql.contains("to_char(to_timestamp(p.updated_at/1000.0)"));
        assert!(m.sql.contains("AND p.updated_at>=? AND p.updated_at<?"));
        assert_eq!(m.values.len(), 5);

        let u = plan_brand_read(RD_METRIC_UPDATES_BRANCH, &v(json!(["co", "br", "br", 1, 2])), Default::default()).unwrap();
        assert!(u.sql.contains("b.display_name label"));
        assert!(u.sql.contains("AND p.updated_at>=? AND p.updated_at<?"));
    }

    #[test]
    fn evidence_search_plans_cursor_query_and_range() {
        let values = v(json!(["co", "br", "br", 1, 2, "q", "q", "q", "n", "n", "id", 26]));
        let p = plan_brand_read(
            RD_EVIDENCE_SEARCH,
            &values,
            EvidenceFlags { range: true, query: true, cursor: true },
        )
        .unwrap();
        assert!(p.sql.contains("p.updated_at>=?"));
        assert!(p.sql.contains("position(lower(?) in lower(b.display_name))>0"));
        assert!(p.sql.contains("(b.display_name>? OR (b.display_name=? AND p.scope_id>?))"));
        assert!(p.sql.contains("ORDER BY b.display_name,p.scope_id LIMIT ?"));
        assert_eq!(p.values.len(), 12);
        // Wrong arity for the declared families fails closed.
        assert!(plan_brand_read(RD_EVIDENCE_SEARCH, &v(json!(["co", "br", "br"])), Default::default()).is_err());
    }

    #[test]
    fn management_and_workspace_kinds_are_allow_listed() {
        let branch = plan_brand_read(RD_MANAGEMENT_BRANCH_EXISTS, &v(json!(["br", "co"])), Default::default()).unwrap();
        assert!(branch.sql.contains("kind='branch' AND status='active'"));
        assert_eq!(branch.values.len(), 2);

        let company = plan_brand_read(RD_MANAGEMENT_COMPANY_EXISTS, &v(json!(["co"])), Default::default()).unwrap();
        assert!(company.sql.contains("kind='company' AND status='active'"));
        assert_eq!(company.values.len(), 1);

        let choice = plan_brand_read(RD_WORKSPACE_CHOICE, &v(json!(["a", "b", "c"])), Default::default()).unwrap();
        assert!(choice.sql.contains("company_id IN (?,?,?)"));
        assert_eq!(choice.values.len(), 3);
        assert!(plan_brand_read(RD_WORKSPACE_CHOICE, &[], Default::default()).is_err());
        assert!(plan_brand_read("brand_rd_unknown", &[], Default::default()).is_err());
    }
}
