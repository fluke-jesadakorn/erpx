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
        kind: "marketing_stmt_1",
        sql: r#"INSERT INTO marketing_public_limits(key,bucket,count) VALUES($1,$2,1) ON CONFLICT(key) DO UPDATE SET count=CASE WHEN marketing_public_limits.bucket=excluded.bucket THEN marketing_public_limits.count+1 ELSE 1 END,bucket=excluded.bucket RETURNING count"#,
        bind_count: 2,
        scope: "holding",
    }, // public-submission.ts
    NativeStatement {
        kind: "marketing_stmt_2",
        sql: r#"INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',NULL,'public-form',NULL,1,$12,$13)"#,
        bind_count: 13,
        scope: "holding",
    }, // public-submission.ts
    NativeStatement {
        kind: "marketing_stmt_3",
        sql: r#"INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES($1,$2,1,'form_submit','web',$3,'public-form',$4)"#,
        bind_count: 4,
        scope: "holding",
    }, // public-submission.ts
    NativeStatement {
        kind: "marketing_stmt_4",
        sql: r#"INSERT INTO marketing_public_receipts(key,request_hash,candidate_id,created_at) VALUES($1,$2,$3,$4)"#,
        bind_count: 4,
        scope: "holding",
    }, // public-submission.ts
    NativeStatement {
        kind: "marketing_stmt_6",
        sql: r#"INSERT INTO marketing_campaign_costs(id,company_id,campaign_id,journal_line_id,evidence,reviewed_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
        scope: "holding",
    }, // campaigns.ts cost link
    NativeStatement {
        kind: "marketing_campaigns_read",
        sql: r#"SELECT id,code,name,version FROM marketing_campaigns WHERE company_id=$1 ORDER BY code"#,
        bind_count: 1,
        scope: "holding",
    }, // campaigns.ts workspace campaign list
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
    fn admits_campaign_cost_link_with_full_payload() {
        let cost_link = statement("marketing_stmt_6").expect("cost-link statement");
        assert_eq!(cost_link.bind_count, 7);
        assert_eq!(cost_link.scope, "holding");
        assert!(cost_link.sql.starts_with("INSERT INTO marketing_campaign_costs"));
    }

    #[test]
    fn crm_touch_writer_is_retired() {
        assert!(statement("marketing_stmt_5").is_none(), "marketing_stmt_5 must be retired with the crm-touches shim");
        assert!(!supported("marketing_stmt_5"));
    }

    #[test]
    fn campaigns_read_is_owner_authored_and_single_bound() {
        let read = statement("marketing_campaigns_read").expect("campaign list read");
        assert_eq!(
            read.sql,
            "SELECT id,code,name,version FROM marketing_campaigns WHERE company_id=$1 ORDER BY code"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.scope, "holding");
        assert!(statement("marketing_stmt_5").is_none());
    }

    #[test]
    fn every_statement_is_well_formed() {
        for s in STATEMENTS {
            assert!(
                s.sql.trim_start().starts_with("INSERT")
                    || s.sql.trim_start().starts_with("UPDATE")
                    || s.sql.trim_start().starts_with("DELETE")
                    || s.sql.trim_start().starts_with("SELECT")
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
