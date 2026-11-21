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
        kind: "runtime_stmt_1",
        sql: r#"INSERT INTO executive_reports(id,company_id,owner_id,definition_json,filters_json,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_2",
        sql: r#"INSERT INTO executive_report_revisions(report_id,version,definition_json,filters_json,author_id,created_at) VALUES($1,1,$2,$3,$4,$5)"#,
        bind_count: 5,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_3",
        sql: r#"UPDATE executive_reports SET definition_json=$1,filters_json=$2,version=$3,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_4",
        sql: r#"INSERT INTO executive_report_revisions(report_id,version,definition_json,filters_json,author_id,created_at) VALUES($1,$2,$3,$4,$5,$6)"#,
        bind_count: 6,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_5",
        sql: r#"UPDATE executive_report_shares SET definition_json=$1,version=version+1 WHERE id=$2"#,
        bind_count: 2,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_6",
        sql: r#"INSERT INTO executive_report_shares(id,report_id,company_id,recipient_id,recipient_name,role,mode,widget_ids_json,definition_json,filters_json,publisher_json,expires_at,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(report_id,recipient_id) DO UPDATE SET recipient_name=excluded.recipient_name,role=excluded.role,mode=excluded.mode,widget_ids_json=excluded.widget_ids_json,definition_json=excluded.definition_json,filters_json=excluded.filters_json,publisher_json=excluded.publisher_json,expires_at=excluded.expires_at,revoked_at=NULL,version=executive_report_shares.version+1"#,
        bind_count: 13,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_7",
        sql: r#"UPDATE executive_reports SET owner_id=$1,version=$2,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_8",
        sql: r#"INSERT INTO executive_report_revisions(report_id,version,definition_json,filters_json,author_id,created_at) VALUES($1,$2,$3,$4,$5,$6)"#,
        bind_count: 6,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_9",
        sql: r#"UPDATE executive_report_shares SET revoked_at=$1,version=version+1 WHERE report_id=$2 AND revoked_at IS NULL"#,
        bind_count: 2,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_10",
        sql: r#"UPDATE executive_generation_jobs SET state='cancelled',error='Report ownership changed. Start a new generation with the current owner.',updated_at=$1 WHERE report_id=$2 AND state IN ('queued','running')"#,
        bind_count: 2,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_11",
        sql: r#"UPDATE executive_report_shares SET revoked_at=$1,version=version+1 WHERE id=$2 AND report_id=$3 AND revoked_at IS NULL"#,
        bind_count: 3,
        scope: "holding",
    }, // reports.ts
    NativeStatement {
        kind: "runtime_stmt_12",
        sql: r#"INSERT INTO executive_generation_jobs(id,report_id,company_id,user_id,assignment_id,scope_key,question,input_json,actor_json,provider_json,state,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'queued',$11,$12)"#,
        bind_count: 12,
        scope: "holding",
    }, // generation.ts
    NativeStatement {
        kind: "runtime_stmt_13",
        sql: r#"UPDATE executive_generation_jobs SET state='cancelled',updated_at=$1 WHERE id=$2 AND report_id=$3 AND user_id=$4 AND assignment_id=$5 AND state IN ('queued','running')"#,
        bind_count: 5,
        scope: "holding",
    }, // generation.ts
    NativeStatement {
        kind: "runtime_stmt_14",
        sql: r#"UPDATE executive_generation_jobs SET state='ready',proposal_json=$1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
        scope: "holding",
    }, // generation.ts
    NativeStatement {
        kind: "runtime_stmt_15",
        sql: r#"UPDATE executive_generation_jobs SET state='failed',error=$1,updated_at=$2 WHERE id=$3 AND state='running' AND lease_token=$4"#,
        bind_count: 4,
        scope: "holding",
    }, // generation.ts
    NativeStatement {
        kind: "runtime_stmt_16",
        sql: r#"UPDATE executive_external_tokens SET last_used_at=$1 WHERE id=$2 AND revoked_at IS NULL"#,
        bind_count: 2,
        scope: "control",
    }, // mcp.ts
    NativeStatement {
        kind: "runtime_stmt_17",
        sql: r#"INSERT INTO executive_mcp_audit(id,token_id,user_id,company_id,assignment_id,branch_id,department_id,action,outcome,code,started_at,duration_ms)
      VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
        scope: "control",
    }, // mcp.ts
    NativeStatement {
        kind: "runtime_stmt_18",
        sql: r#"INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES($1,$2,$3,$4,$5,$6,'active') ON CONFLICT(holding_id) DO NOTHING"#,
        bind_count: 6,
        scope: "control",
    }, // beta-onboarding-effects.ts
    NativeStatement {
        kind: "runtime_stmt_19",
        sql: r#"INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES('billing',$1,'{}') ON CONFLICT(service) DO UPDATE SET heartbeat_at=excluded.heartbeat_at"#,
        bind_count: 1,
        scope: "control",
    },
    // Dynamic kinds: the SQL is rendered at runtime by the feature-predicate
    // port (never a dev-only TRUE). These placeholders fix only the arity.
    NativeStatement {
        kind: "runtime_stmt_20",
        sql: r#"UPDATE executive_generation_jobs j SET state='failed',updated_at=$1 WHERE state='running' AND lease_until<$2"#,
        bind_count: 2,
        scope: "holding",
    },
    NativeStatement {
        kind: "runtime_stmt_21",
        sql: r#"UPDATE executive_generation_jobs SET lease_token=$1,lease_until=$2,updated_at=$3 WHERE state='queued'"#,
        bind_count: 3,
        scope: "holding",
    },
    NativeStatement {
        kind: "runtime_stmt_22",
        sql: r#"INSERT INTO erpx_migrations(name,checksum) VALUES($1,$2)"#,
        bind_count: 2,
        scope: "control",
    },
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

/// Kinds whose SQL is rendered at runtime by a composition port rather than
/// taken from this static registry.
pub fn is_dynamic_statement(kind: &str) -> bool {
    matches!(kind, "runtime_stmt_20" | "runtime_stmt_21")
}
