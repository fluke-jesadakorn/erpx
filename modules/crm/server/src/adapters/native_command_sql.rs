//! Native write statements owned by this crate. Generated from the
//! retired TypeScript write literals with full-parity positional payloads.
//! The axum executor runs these in the caller-owned serializable unit.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

const STATEMENTS: &[NativeStatement] = &[
    NativeStatement {
        kind: "crm_stmt_1",
        sql: r#"INSERT INTO crm_case_events VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_2",
        sql: r#"INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_3",
        sql: r#"INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_4",
        sql: r#"INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,created_by,created_at,updated_at,account_id,contact_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"#,
        bind_count: 16,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_5",
        sql: r#"UPDATE crm_leads SET owner_id=$1,due_date=$2,version=version+1,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_6",
        sql: r#"UPDATE crm_leads SET status=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_7",
        sql: r#"UPDATE crm_opportunities SET stage=$1,quote_id=CASE WHEN $2 THEN NULL ELSE quote_id END,quote_version=CASE WHEN $3 THEN NULL ELSE quote_version END,version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_8",
        sql: r#"UPDATE crm_leads SET version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
        scope: "holding",
    }, // leads.ts
    NativeStatement {
        kind: "crm_stmt_9",
        sql: r#"INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
        scope: "holding",
    }, // identities.ts
    NativeStatement {
        kind: "crm_stmt_10",
        sql: r#"INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
        scope: "holding",
    }, // identities.ts
    NativeStatement {
        kind: "crm_stmt_11",
        sql: r#"UPDATE crm_accounts SET finance_party_id=$1,review_evidence=$2,reviewed_by=$3,version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
        scope: "holding",
    }, // identities.ts
    NativeStatement {
        kind: "crm_stmt_12",
        sql: r#"INSERT INTO crm_guest_links(id,company_id,contact_id,guest_id,property_id,evidence,reviewed_by,created_at,sharing_state,sharing_evidence,sharing_by,sharing_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
        scope: "holding",
    }, // identities.ts
    NativeStatement {
        kind: "crm_stmt_13",
        sql: r#"INSERT INTO crm_guest_sharing_events(id,link_id,company_id,state,evidence,actor_id,version,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
        scope: "holding",
    }, // identities.ts
    NativeStatement {
        kind: "crm_stmt_14",
        sql: r#"UPDATE crm_guest_links SET sharing_state=$1,sharing_evidence=$2,sharing_by=$3,sharing_at=$4,version=version+1 WHERE id=$5 AND version=$6"#,
        bind_count: 6,
        scope: "holding",
    }, // identities.ts
    NativeStatement {
        kind: "crm_stmt_15",
        sql: r#"INSERT INTO crm_guest_sharing_events(id,link_id,company_id,state,evidence,actor_id,version,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
        scope: "holding",
    }, // identities.ts
    NativeStatement {
        kind: "crm_stmt_17",
        sql: r#"UPDATE crm_opportunities SET sales_id=$1,win_source='sales',won_at=$2,expected_minor=(SELECT subtotal_minor FROM (SELECT "id","company_id","branch_id","department_id","kind","number","party_id","warehouse_id","currency","order_date","due_date","note","state","subtotal_minor","tax_minor","source_id","version","created_by","approved_by","created_at","updated_at","price_list_id","price_list_version","discount_reason","discount_approval_required","discount_approved_by","discount_approved_at" FROM commerce_orders) commerce_orders WHERE id=$3),stage='won',version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
        scope: "holding",
    }, // leads.ts (resolved commerce projection)
];

pub fn statement(kind: &str) -> Option<&'static NativeStatement> {
    STATEMENTS.iter().find(|s| s.kind == kind)
}
pub fn supported(kind: &str) -> bool {
    statement(kind).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_statement_is_well_formed() {
        for s in STATEMENTS {
            assert!(
                s.sql.trim_start().starts_with("INSERT")
                    || s.sql.trim_start().starts_with("UPDATE")
                    || s.sql.trim_start().starts_with("DELETE")
            );
            let max = s
                .sql
                .split('$')
                .skip(1)
                .filter_map(|x| {
                    x.chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<usize>()
                        .ok()
                })
                .max()
                .unwrap_or(0);
            assert_eq!(max, s.bind_count, "{}", s.kind);
        }
    }
}
