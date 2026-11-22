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
    // supplier-bills.ts
    NativeStatement {
        kind: "procurement_stmt_1",
        sql: r#"INSERT INTO procurement_bill_match_events VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_2",
        sql: r#"UPDATE procurement_bill_matches SET status='invoiced',invoice_id=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_3",
        sql: r#"UPDATE procurement_bill_matches SET status='posted',version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_4",
        sql: r#"INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,subtotal_minor,tax_minor,quantity_matches,status,created_by,created_at,updated_at)
    VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,'review',$21,$22,$23)"#,
        bind_count: 23,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_5",
        sql: r#"INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) VALUES($1,$2,$3,$4)"#,
        bind_count: 4,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_6",
        sql: r#"INSERT INTO procurement_bill_match_lines VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_7",
        sql: r#"INSERT INTO procurement_invoice_policies(company_id,matching_mode,version,updated_by,updated_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id) DO UPDATE SET matching_mode=excluded.matching_mode,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at"#,
        bind_count: 5,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_8",
        sql: r#"INSERT INTO procurement_invoice_policy_events(id,company_id,version,previous_mode,matching_mode,actor_id,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_9",
        sql: r#"UPDATE procurement_bill_match_receipts SET active=FALSE WHERE match_id=$1 AND active=TRUE"#,
        bind_count: 1,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_10",
        sql: r#"UPDATE procurement_bill_matches SET status=$1,reviewed_by=$2,review_reason=$3,variance_account_id=$4,version=version+1,updated_at=$5 WHERE id=$6"#,
        bind_count: 6,
        scope: "holding",
    },
    // rfqs.ts
    NativeStatement {
        kind: "procurement_stmt_11",
        sql: r#"INSERT INTO procurement_rfq_events(id,rfq_id,version,action,actor_id,evidence,selected_offer_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_12",
        sql: r#"UPDATE procurement_rfqs SET status='ordered',order_id=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_13",
        sql: r#"INSERT INTO procurement_rfqs(id,company_id,branch_id,department_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,created_at,updated_at,request_id,request_version,request_evidence_version,request_items_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'collecting',$10,$11,$12,$13,$14,$15,$16)"#,
        bind_count: 16,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_14",
        sql: r#"INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal,request_item_ordinal,request_quantity_milli,request_unit,request_units_per_product_unit_milli) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        bind_count: 11,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_15",
        sql: r#"INSERT INTO procurement_rfq_offers VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        bind_count: 13,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_16",
        sql: r#"INSERT INTO procurement_rfq_offer_lines VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_17",
        sql: r#"UPDATE procurement_rfqs SET selected_offer_id=$1,selection_reason=$2,submitted_by=$3,approved_by=NULL WHERE id=$4"#,
        bind_count: 4,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_18",
        sql: r#"UPDATE procurement_rfqs SET approved_by=$1 WHERE id=$2"#,
        bind_count: 2,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_19",
        sql: r#"UPDATE procurement_rfqs SET selected_offer_id=NULL,selection_reason=NULL,submitted_by=NULL,approved_by=NULL WHERE id=$1"#,
        bind_count: 1,
        scope: "holding",
    },
    NativeStatement {
        kind: "procurement_stmt_20",
        sql: r#"UPDATE procurement_rfqs SET status=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
        scope: "holding",
    },
    // records-employee-requests-owned-facts.ts
    NativeStatement {
        kind: "procurement_stmt_21",
        sql: r#"INSERT INTO procurement_request_orders(request_id,order_id,rfq_id,request_version,request_evidence_version,linked_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
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
    fn covers_every_distinct_retired_statement() {
        assert_eq!(STATEMENTS.len(), 21);
    }
}
