//! Native write statements owned by this crate. Generated from the retired
//! TypeScript write literals with full-parity positional payloads. The axum
//! executor runs these in the caller-owned serializable unit.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

const STATEMENTS: &[NativeStatement] = &[
    // request-ai.ts
    NativeStatement {
        kind: "records_stmt_1",
        sql: r#"UPDATE request_ai_proposals SET state='queued',error=NULL,attempts=0,lease_token=NULL,claim_until=NULL,actor_json=$1,updated_at=$2 WHERE id=$3 AND state IN ('failed','stale')"#,
        bind_count: 3,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_2",
        sql: r#"INSERT INTO request_ai_proposals(id,company_id,request_id,source_version,provider_json,actor_json,state,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,'queued',$7,$8)"#,
        bind_count: 8,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_3",
        sql: r#"UPDATE request_ai_usage SET state='reserved',lease_until=$1 WHERE id=$2"#,
        bind_count: 2,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_4",
        sql: r#"INSERT INTO request_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) VALUES($1,$2,$3,$4,'reserved',$5,$6)"#,
        bind_count: 6,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_5",
        sql: r#"UPDATE request_ai_proposals SET state='running',lease_token=$1,claim_until=$2,attempts=attempts+1,updated_at=$3 WHERE id=$4 AND attempts<3 AND (state='queued' OR (state='running' AND claim_until<$5)) RETURNING *"#,
        bind_count: 5,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_6",
        sql: r#"UPDATE request_ai_proposals SET state='ready',result_json=$1,error=NULL,lease_token=NULL,claim_until=NULL,updated_at=$2 WHERE id=$3 AND lease_token=$4"#,
        bind_count: 4,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_7",
        sql: r#"UPDATE request_ai_usage SET state='succeeded',lease_until=0,input_tokens=$1,output_tokens=$2 WHERE id=$3"#,
        bind_count: 3,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_8",
        sql: r#"UPDATE request_ai_proposals SET state=$1,error=$2,lease_token=NULL,claim_until=NULL,updated_at=$3 WHERE id=$4 AND state='running' AND lease_token=$5 RETURNING id"#,
        bind_count: 5,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_9",
        sql: r#"UPDATE request_ai_usage SET state='failed',lease_until=0 WHERE id=$1"#,
        bind_count: 1,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_10",
        sql: r#"UPDATE request_ai_proposals SET state='failed',error='Preparation was interrupted. Retry or continue manually.',lease_token=NULL,claim_until=NULL WHERE state='running' AND claim_until<$1 AND attempts>=3"#,
        bind_count: 1,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_11",
        sql: r#"UPDATE request_ai_usage SET state='failed',lease_until=0 WHERE state='reserved' AND id IN (SELECT id FROM request_ai_proposals WHERE state IN ('failed','stale'))"#,
        bind_count: 0,
        scope: "holding",
    },
    // employee-requests.ts
    NativeStatement {
        kind: "records_stmt_12",
        sql: r#"INSERT INTO employee_request_events(id,company_id,request_id,request_version,action,actor_id,note,evidence_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_13",
        sql: r#"UPDATE employee_requests SET branch_id=$1,department_id=$2,category=$3,title=$4,description=$5,location=$6,impact=$7,needed_date=$8,items_json=$9,currency=$10,state='draft',blocked_reason=NULL,approval_json='{}',version=$11,updated_at=$12 WHERE id=$13 AND company_id=$14"#,
        bind_count: 14,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_14",
        sql: r#"INSERT INTO employee_requests(id,company_id,engagement_id,branch_id,department_id,category,title,description,location,impact,needed_date,items_json,currency,kind,requester_user_id,created_by,parent_request_id,version,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)"#,
        bind_count: 20,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_15",
        sql: r#"UPDATE employee_requests SET engagement_id=$1 WHERE id=$2 AND company_id=$3"#,
        bind_count: 3,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_16",
        sql: r#"UPDATE employee_requests SET state=$1,team_id=$2,assignee_user_id=$3,routing_json=$4,approval_json=$5,blocked_reason=$6,resolution=$7,purchase_order_id=$8,handover_json=$9,version=$10,updated_at=$11 WHERE id=$12 AND company_id=$13"#,
        bind_count: 13,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_17",
        sql: r#"UPDATE employee_requests SET purchase_order_id=COALESCE(purchase_order_id,$1),version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4"#,
        bind_count: 4,
        scope: "holding",
    },
    // records.ts
    NativeStatement {
        kind: "records_stmt_18",
        sql: r#"INSERT INTO record_principal_mentions(host_module,host_id,target_kind,target_id,target_label,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING"#,
        bind_count: 7,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_19",
        sql: r#"INSERT INTO record_field_suggestions(id,module_id,record_id,field_key,proposed_value_json,previous_value_digest,source_versions_json,provenance_json,state,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_20",
        sql: r#"UPDATE workspace_records SET fields_json=$1,version=version+1,updated_by=$2,updated_at=$3 WHERE module_id=$4 AND id=$5"#,
        bind_count: 5,
        scope: "holding",
    },
    NativeStatement {
        kind: "records_stmt_21",
        sql: r#"UPDATE record_field_suggestions SET state=$1,applied_by=$2,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
        scope: "holding",
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

    #[test]
    fn covers_every_retired_writer_statement() {
        assert_eq!(STATEMENTS.len(), 21);
    }
}
