//! Authoritative SQL for every native Billing owner write kind.
//!
//! The SQL and its exact positional argument order live here, in the Billing
//! owner crate. A transport calls [`billing_owner_statement`] with a typed kind
//! and caller-supplied values; the caller executes the returned statement in
//! its own serializable unit together with receipt/audit/outbox effects.
//! No TypeScript SQL copy and no fallback writer remain for a retired kind.

use crate::domain::billing_owner_write::*;
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BillingOwnerWriteInput {
    pub kind: String,
    pub values: Vec<serde_json::Value>,
    pub now: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingOwnerStatement {
    pub sql: String,
    pub values: Vec<serde_json::Value>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "BILLING_OWNER_WRITE_INVALID",
        "Review the Billing owner write request.",
    )
}

/// Return the authoritative SQL for an allow-listed owner write kind.
fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        BETA_RETAIN_CUSTOMER => r####"UPDATE beta_billing SET customer_id=?,updated_at=? WHERE holding_id=? AND customer_id IS NULL"####,
        BETA_ENQUEUE_EVENT => r####"INSERT INTO stripe_inbox(id,event_type,created_at,payload_json) VALUES(?,?,?,?) ON CONFLICT DO NOTHING"####,
        BETA_IGNORE_EVENT => r####"UPDATE stripe_inbox SET state='ignored',applied_at=? WHERE id=?"####,
        BETA_APPLY_SUBSCRIPTION => r####"UPDATE beta_billing SET subscription_id=?,status=?,current_period_end=?,last_event_at=?,updated_at=? WHERE holding_id=? AND last_event_at<=?"####,
        BETA_APPLY_EVENT => r####"UPDATE stripe_inbox SET state='applied',applied_at=? WHERE id=?"####,
        BETA_PREPARE_BILLING => r####"INSERT INTO beta_billing(holding_id,updated_at) VALUES(?,?) ON CONFLICT DO NOTHING"####,
        BETA_MANAGEMENT_BILLING_STATUS => r####"SELECT status FROM beta_billing WHERE holding_id=?"####,
        BETA_CUSTOMER => r####"SELECT customer_id FROM beta_billing WHERE holding_id=?"####,
        BETA_INBOX_STATE => r####"SELECT state FROM stripe_inbox WHERE id=?"####,
        BETA_LEGACY_CUSTOMER => r####"SELECT holding_id FROM beta_billing WHERE customer_id=?"####,
        FORECAST_CURRENT_PRICE => r####"SELECT * FROM billing_target_prices
        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0
          AND included_seats=0 AND seat_monthly_satang IS NOT NULL
          AND storage_allowance_mode='per_paid_seat_time' AND effective_month<=$1
        ORDER BY effective_month DESC,id DESC LIMIT 1"####,
        FORECAST_FUTURE_PRICE => r####"SELECT * FROM billing_target_prices
        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0
          AND included_seats=0 AND seat_monthly_satang IS NOT NULL
          AND storage_allowance_mode='per_paid_seat_time' AND effective_month>$1
        ORDER BY effective_month ASC,id ASC LIMIT 1"####,
        TARGET_ROLLOUT_EFFECTIVE_PRICE => r####"SELECT id,included_original_bytes,seat_terms_status,rating_enabled
      FROM billing_target_prices WHERE effective_month<=$1
      ORDER BY effective_month DESC,id DESC LIMIT 1"####,
        TARGET_ROLLOUT_RATED_SHADOW_PERIOD => r####"SELECT state,mode,total_satang,price_id,meter_snapshot_id,lines_json FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2"####,
        APPROVED_RATED_PERIOD => r####"SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'"####,
        ACTIVE_TARGET_ENROLLMENT => r####"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL"####,
        SELECTED_TARGET_PRICE => r####"SELECT * FROM billing_target_prices WHERE id=$1 AND effective_month<=$2"####,
        CURRENT_TARGET_PRICE => r####"SELECT * FROM billing_target_prices WHERE effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1"####,
        TARGET_ROLLOUT_AVAILABLE_OPERATING_COSTS => r####"SELECT o.category,o.amount_satang,e.source_state
       FROM billing_target_operating_costs o
       JOIN billing_evidence e ON e.id=o.evidence_id
       WHERE o.holding_id=$1 AND o.month=$2"####,
        TARGET_ROLLOUT_CAPACITY_REVIEW => r####"SELECT r.*,e.source_state FROM billing_target_capacity_reviews r
     JOIN billing_evidence e ON e.id=r.evidence_id
     WHERE r.holding_id=$1 AND r.effective_month=$2"####,
        TARGET_STATEMENTS => r####"SELECT * FROM billing_target_statements WHERE holding_id=$1 AND month=$2"####,
        EVIDENCE_RETAIN => r####"INSERT INTO billing_evidence(id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO NOTHING"####,
        EVIDENCE_STORAGE_LEDGER => r####"INSERT INTO billing_evidence_storage_ledger(id,evidence_id,holding_id,object_key,bytes,state,created_at,updated_at) SELECT $1,id,holding_id,object_key,size,'stored',$2,$2 FROM billing_evidence WHERE id=$3 ON CONFLICT(object_key) DO NOTHING"####,
        EVIDENCE_SCAN_ATTEMPT => r####"INSERT INTO billing_evidence_scan_attempts(id,evidence_id,attempt,state,created_at) VALUES($1,$2,1,'pending',$3) ON CONFLICT(evidence_id,attempt) DO NOTHING"####,
        EVIDENCE_EVENT_QUARANTINED => r####"INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES($1,$2,$3,$4,'file.quarantined',$5,$6) ON CONFLICT(id) DO NOTHING"####,
        EVIDENCE_SCOPED_READ => r####"SELECT * FROM billing_evidence WHERE id=$1 AND holding_id=$2"####,
        EVIDENCE_EXISTING_READ => r####"SELECT * FROM billing_evidence WHERE id=$1"####,
        AVAILABLE_EVIDENCE => r####"SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'"####,
        BILLING_ACCOUNT_EXISTS => r####"SELECT 1 FROM billing_accounts WHERE holding_id=$1"####,
        SCAN_SOURCE => r####"SELECT * FROM billing_evidence WHERE id=$1"####,
        APP_FINANCE_PROJECTION => r####"SELECT invoice_satang,receivable_satang,received_satang,reversed_satang,credited_satang,stripe_status,synced_at FROM billing_test_finance_projection WHERE billing_invoice_id=$1 AND customer_holding_id=$2"####,
        RECONCILIATION_RUN => r####"INSERT INTO billing_evidence_reconciliation_runs(id,mode,upload_frozen,checksums_verified,scanner_engine,scanner_definition_at,ready,report_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"####,
        SCAN_CLAIM => r####"UPDATE billing_evidence_scan_attempts SET state='running',claimed_at=$1,claim_until=$2,lease_token=$3 WHERE id=(SELECT id FROM billing_evidence_scan_attempts WHERE state IN ('pending','running') AND created_at<=$1 AND (claim_until IS NULL OR claim_until<$1) ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *"####,
        SCAN_MARK_SCANNING => r####"UPDATE billing_evidence SET source_state='scanning' WHERE id=$1 AND source_state IN ('legacy_unscanned','quarantined','scan_error')"####,
        SCAN_CLEAN_ATTEMPT => r####"UPDATE billing_evidence_scan_attempts SET state='clean',scanner_engine=?,scanner_signature=NULL,scanner_definition_at=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?"####,
        SCAN_CLEAN_EVIDENCE => r####"UPDATE billing_evidence SET source_state='available',detected_mime=?,scanned_at=?,scanner_definition_at=? WHERE id=?"####,
        SCAN_CLEAN_EVENT => r####"INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.available',?,?)"####,
        SCAN_INFECTED_ATTEMPT => r####"UPDATE billing_evidence_scan_attempts SET state='infected',scanner_engine=?,scanner_signature=?,scanner_definition_at=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?"####,
        SCAN_INFECTED_EVIDENCE => r####"UPDATE billing_evidence SET source_state='malware_rejected',detected_mime=?,scanned_at=?,scanner_definition_at=? WHERE id=?"####,
        SCAN_INFECTED_EVENT => r####"INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.malware_rejected',?,?)"####,
        SCAN_ERROR_ATTEMPT => r####"UPDATE billing_evidence_scan_attempts SET state='error',error_code=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?"####,
        SCAN_ERROR_EVIDENCE => r####"UPDATE billing_evidence SET source_state='scan_error' WHERE id=?"####,
        SCAN_ERROR_EVENT => r####"INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.error',?,?)"####,
        SCAN_RETRY_ATTEMPT => r####"INSERT INTO billing_evidence_scan_attempts(id,evidence_id,attempt,state,created_at) VALUES(?,?,?,'pending',?) ON CONFLICT(evidence_id,attempt) DO NOTHING"####,
        MIRROR_INVOICE_STATE => r####"UPDATE billing_invoices SET status=$2,hosted_url=$3,pdf_url=$4,due_at=$5,payment_state=$6,updated_at=$7 WHERE id=$1"####,
        RELEASE_VOIDED_COSTS => r####"UPDATE billing_costs SET invoice_id=NULL WHERE invoice_id=$1"####,
        MARK_VOIDED_PERIOD_INCOMPLETE => r####"UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1"####,
        RETAIN_TEST_FINANCE_PROJECTION => r####"INSERT INTO billing_test_finance_projection
      (billing_invoice_id,stripe_invoice_id,customer_holding_id,company_id,invoice_satang,
       receivable_satang,received_satang,reversed_satang,credited_satang,stripe_status,synced_at)
      VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
      ON CONFLICT(billing_invoice_id) DO UPDATE SET
        receivable_satang=EXCLUDED.receivable_satang,
        received_satang=EXCLUDED.received_satang,
        reversed_satang=EXCLUDED.reversed_satang,
        credited_satang=EXCLUDED.credited_satang,
        stripe_status=EXCLUDED.stripe_status,synced_at=EXCLUDED.synced_at"####,
        RETAIN_JOB_PROGRESS => r####"UPDATE billing_jobs SET progress_json=$2 WHERE id=$1"####,
        RETAIN_STRIPE_INVOICE => r####"UPDATE billing_invoices SET stripe_id=$2,status='draft' WHERE id=$1"####,
        RETAIN_INVOICE_MODE => r####"UPDATE billing_invoices SET payment_mode=$2 WHERE id=$1"####,
        COMPLETE_JOB => r####"UPDATE billing_jobs SET state='done',last_error=NULL WHERE id=$1"####,
        RETAIN_CURRENT_CARD => r####"UPDATE billing_accounts SET card_json=$2 WHERE holding_id=$1"####,
        RESCHEDULE_JOB => r####"UPDATE billing_jobs SET attempts=attempts+1,state=$2,next_at=$3,last_error=$4 WHERE id=$1"####,
        RETAIN_SETUP_STATE => r####"UPDATE billing_accounts SET setup_state=$2 WHERE holding_id=$1"####,
        RETAIN_SETUP_CARD => r####"UPDATE billing_accounts SET card_json=$2 WHERE holding_id=$1"####,
        APPLY_STRIPE_EVENT => r####"UPDATE stripe_inbox SET state='applied',applied_at=$2,last_error=NULL WHERE id=$1"####,
        RESCHEDULE_STRIPE_EVENT => r####"UPDATE stripe_inbox SET attempts=attempts+1,next_at=$2,last_error=$3 WHERE id=$1"####,
        JOBS_RETRY_LEGACY_EVENT => r####"UPDATE stripe_inbox SET attempts=attempts+1,next_at=?,last_error=? WHERE id=?"####,
        RECORD_RATING_RUN => r####"INSERT INTO billing_target_rating_runs
     (id,holding_id,month,price_id,meter_snapshot_id,state,missing_json,
      lines_json,total_satang,actor,reason,created_at)
     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"####,
        RETAIN_RATED_PERIOD => r####"INSERT INTO billing_target_rated_periods
     (holding_id,month,price_id,assignment_month,mode,meter_snapshot_id,
      service_start,service_end,state,missing_json,lines_json,total_satang,rated_at)
     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
     ON CONFLICT(holding_id,month) DO UPDATE SET
       price_id=excluded.price_id, assignment_month=excluded.assignment_month,
       mode=excluded.mode, meter_snapshot_id=excluded.meter_snapshot_id,
       service_start=excluded.service_start, service_end=excluded.service_end,
       state=excluded.state, missing_json=excluded.missing_json,
       lines_json=excluded.lines_json, total_satang=excluded.total_satang,
       rated_at=excluded.rated_at
     WHERE billing_target_rated_periods.state<>'approved'"####,
        RECORD_SCHEDULED_CHANGE => r####"INSERT INTO billing_audit VALUES($1,$2,$3,$4,$5,$6,$7)"####,
        END_ENROLLMENT => r####"UPDATE billing_enrollments SET ended_at=effective_at,pending_end=FALSE,effective_at=NULL WHERE id=$1"####,
        ACTIVATE_PENDING_PLAN => r####"UPDATE billing_enrollments SET plan_id=$2,pending_plan=NULL,effective_at=NULL WHERE id=$1"####,
        CREATE_BILLING_PERIOD => r####"INSERT INTO billing_periods(id,holding_id,enrollment_id,month,start_at,end_at,plan_id,plan_revision,support_satang) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING"####,
        CLOSE_ELAPSED_PERIODS => r####"UPDATE billing_periods SET closed_at=$2 WHERE holding_id=$1 AND end_at<=$2 AND closed_at IS NULL"####,
        ENSURE_ACCOUNT => r####"INSERT INTO billing_accounts(holding_id,customer_id,created_at) SELECT $1,(SELECT customer_id FROM beta_billing WHERE holding_id=$1),$2 ON CONFLICT DO NOTHING"####,
        RETAIN_CUSTOMER => r####"UPDATE billing_accounts SET customer_id=$2 WHERE holding_id=$1"####,
        ENROLL => r####"INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES($1,$2,$3,$4)"####,
        SCHEDULE_ENROLLMENT_CHANGE => r####"UPDATE billing_enrollments SET pending_plan=$2,pending_end=$3,effective_at=$4 WHERE id=$1"####,
        RETAIN_CONSENTED_CARD => r####"UPDATE billing_accounts SET card_json=$2,consent_at=$3,consent_actor=$4 WHERE holding_id=$1"####,
        QUEUE_MANUAL_INVOICE => r####"INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'manual',$4,$4) ON CONFLICT DO NOTHING"####,
        WITHDRAW_CONSENT => r####"UPDATE billing_accounts SET consent_at=NULL,consent_actor=NULL WHERE holding_id=$1"####,
        SET_PAYMENT_MODE => r####"UPDATE billing_accounts SET payment_mode=$2 WHERE holding_id=$1"####,
        INSERT_PLAN_REVISION => r####"INSERT INTO billing_plans VALUES($1,$2,$3,$4,$5,$6)"####,
        ASSIGN_TARGET_PRICE => r####"INSERT INTO billing_target_assignments
             (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING holding_id"####,
        RECORD_PROVIDER_COST => r####"INSERT INTO billing_target_provider_costs
           (holding_id,month,source_key,provider,statement_id,original_currency,original_minor,
            fx_numerator,fx_denominator,provider_cost_satang,evidence_id,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"####,
        RECORD_PROVIDER_STATEMENT => r####"INSERT INTO billing_target_statements
           (holding_id,month,provider,statement_id,evidence_id,provider_total_minor,
            original_currency,reconciled_total_minor,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$7,$8,$9) ON CONFLICT DO NOTHING RETURNING holding_id"####,
        RECORD_OPERATING_COST => r####"INSERT INTO billing_target_operating_costs
           (holding_id,month,category,amount_satang,evidence_id,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING RETURNING holding_id"####,
        RECORD_CAPACITY_REVIEW => r####"INSERT INTO billing_target_capacity_reviews
           (holding_id,effective_month,projected_original_bytes,primary_available_bytes,
            backup_available_bytes,reserved_bytes,company_limit_bytes,evidence_id,
            approved_by,reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           ON CONFLICT DO NOTHING RETURNING holding_id"####,
        RECORD_ROLLOUT_APPROVAL => r####"INSERT INTO billing_target_rollout_approvals
           (holding_id,effective_month,shadow_month_1,shadow_month_2,
            minimum_margin_bps,approved_by,reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING holding_id"####,
        RECORD_TARGET_EXTRA => r####"INSERT INTO billing_target_extras
           (id,holding_id,month,kind,description,amount_satang,service_start,service_end,
            evidence_id,approved_by,approval_reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"####,
        ENROLL_TARGET => r####"INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES($1,$2,'platform_usage',$3)"####,
        INSERT_TARGET_INVOICE => r####"INSERT INTO billing_invoices
           (id,holding_id,period_id,kind,total_satang,lines_json,payment_mode,created_at,
            approved_by,original_invoice_id,updated_at,rating_model)
           VALUES($1,$2,$3,'main',$4,$5,$6,$7,$8,NULL,$7,'target')"####,
        APPROVE_TARGET_RATING => r####"UPDATE billing_target_rated_periods SET state='approved', approved_by=$3,
           approved_at=$4,approval_reason=$5,invoice_id=$6
           WHERE holding_id=$1 AND month=$2 AND state='ready'"####,
        QUEUE_TARGET_ISSUE => r####"INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'issue',$4,$4)"####,
        RETRY_LEGACY_EVENT => r####"UPDATE stripe_inbox SET next_at=$2 WHERE id=$1 AND state='pending' AND payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$3) RETURNING id"####,
        RETRY_JOB => r####"UPDATE billing_jobs SET state='pending',next_at=$2 WHERE id=$1"####,
        INCREMENT_ACCOUNT_VERSION => r####"UPDATE billing_accounts SET version=version+1 WHERE holding_id=$1"####,
        RECORD_COMMAND_AUDIT => r####"INSERT INTO billing_audit VALUES($1,$2,$3,$4,$5,$6,$7)"####,
        RECORD_COMMAND_RECEIPT => r####"INSERT INTO billing_receipts VALUES($1,$2,$3,$4)"####,
        _ => return None,
    })
}

/// Count `?` or the highest `$N` placeholder. Owner SQL is fixed-shape, so
/// the caller value count must match exactly before a statement is returned.
fn placeholder_count(sql: &str) -> usize {
    let mut max = 0usize;
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let mut j = i + 1;
            let mut n = 0usize;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                n = n * 10 + (bytes[j] - b'0') as usize;
                j += 1;
            }
            if j > i + 1 && n > max { max = n; }
            i = j;
        } else { i += 1; }
    }
    if max > 0 { max } else { sql.matches('?').count() }
}

/// Build the owner statement. The kind is allow-listed in the domain, the SQL
/// is selected here, and only positional argument values come from the
/// transport - never statement text. The caller owns execution and effects.
pub fn billing_owner_statement(
    holding: &HoldingId,
    input: &BillingOwnerWriteInput,
) -> Result<BillingOwnerStatement, BusinessError> {
    let kind = OwnerWriteKind::parse(&input.kind)?;
    if holding.as_str().is_empty()
        || input.now <= 0
        || input.values.is_empty()
        || input.values.len() > 64
    {
        return Err(invalid());
    }
    let statement = sql(kind.as_str()).ok_or_else(invalid)?;
    if input.values.len() != placeholder_count(statement) {
        return Err(invalid());
    }
    Ok(BillingOwnerStatement {
        sql: statement.to_owned(),
        values: input.values.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_owner_sql_with_matching_placeholder_shape() {
        for kind in OWNER_WRITE_KINDS {
            let statement = sql(kind).unwrap_or_else(|| panic!("missing owner SQL: {kind}"));
            assert!(placeholder_count(statement) > 0, "write must be parameterized: {kind}");
            assert!(OwnerWriteKind::parse(kind).is_ok());
        }
    }
    #[test]
    fn placeholder_counter_handles_question_and_dollar_forms() {
        assert_eq!(placeholder_count("UPDATE t SET a=?,b=? WHERE id=?"), 3);
        assert_eq!(placeholder_count("UPDATE t SET a=$2 WHERE id=$1"), 2);
    }
    #[test]
    fn statement_rejects_unknown_kind_and_wrong_value_count() {
        let holding = HoldingId::new("h001").unwrap();
        assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: "not_a_billing_write".into(), values: vec![serde_json::json!(1)], now: 1,
        }).is_err());
        assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: BETA_PREPARE_BILLING.into(), values: vec![serde_json::json!(1)], now: 1,
        }).is_err());
    }
    #[test]
    fn beta_management_billing_status_read_is_owner_authored_and_single_bound() {
        let holding = HoldingId::new("h001").unwrap();
        let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: BETA_MANAGEMENT_BILLING_STATUS.into(),
            values: vec![serde_json::json!("h001")],
            now: 1,
        })
        .expect("beta_management_billing_status is an allow-listed owner read kind");
        assert_eq!(statement.sql, "SELECT status FROM beta_billing WHERE holding_id=?");
        assert_eq!(statement.values, vec![serde_json::json!("h001")]);
        // The holding is the only bind; a missing bind cannot widen this read.
        assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: BETA_MANAGEMENT_BILLING_STATUS.into(),
            values: vec![],
            now: 1,
        })
        .is_err());
    }
    #[test]
    fn beta_webhook_reads_are_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        for (kind, sql, value) in [
            (
                BETA_CUSTOMER,
                "SELECT customer_id FROM beta_billing WHERE holding_id=?",
                "h001",
            ),
            (
                BETA_INBOX_STATE,
                "SELECT state FROM stripe_inbox WHERE id=?",
                "evt-1",
            ),
            (
                BETA_LEGACY_CUSTOMER,
                "SELECT holding_id FROM beta_billing WHERE customer_id=?",
                "cus-1",
            ),
        ] {
            let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![serde_json::json!(value)],
                now: 1,
            })
            .unwrap_or_else(|_| panic!("{kind} is an allow-listed owner read kind"));
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.values, vec![serde_json::json!(value)]);
            // A missing witness bind cannot widen an owner read.
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![],
                now: 1,
            })
            .is_err());
        }
    }
    #[test]
    fn forecast_price_reads_are_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        for (kind, sql) in [
            (
                FORECAST_CURRENT_PRICE,
                "SELECT * FROM billing_target_prices\n        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0\n          AND included_seats=0 AND seat_monthly_satang IS NOT NULL\n          AND storage_allowance_mode='per_paid_seat_time' AND effective_month<=$1\n        ORDER BY effective_month DESC,id DESC LIMIT 1",
            ),
            (
                FORECAST_FUTURE_PRICE,
                "SELECT * FROM billing_target_prices\n        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0\n          AND included_seats=0 AND seat_monthly_satang IS NOT NULL\n          AND storage_allowance_mode='per_paid_seat_time' AND effective_month>$1\n        ORDER BY effective_month ASC,id ASC LIMIT 1",
            ),
        ] {
            let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![serde_json::json!("2027-02")],
                now: 1,
            })
            .unwrap_or_else(|_| panic!("{kind} is an allow-listed owner read kind"));
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.values, vec![serde_json::json!("2027-02")]);
            // A missing or extra forecast month bind cannot widen an owner read.
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![],
                now: 1,
            })
            .is_err());
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![serde_json::json!("2027-02"), serde_json::json!("h001")],
                now: 1,
            })
            .is_err());
        }
        assert_ne!(FORECAST_CURRENT_PRICE, FORECAST_FUTURE_PRICE);
    }
    #[test]
    fn target_rollout_price_and_shadow_period_reads_are_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        for (kind, sql, values, wrong) in [
            (
                TARGET_ROLLOUT_EFFECTIVE_PRICE,
                "SELECT id,included_original_bytes,seat_terms_status,rating_enabled\n      FROM billing_target_prices WHERE effective_month<=$1\n      ORDER BY effective_month DESC,id DESC LIMIT 1",
                vec![serde_json::json!("2027-02")],
                vec![serde_json::json!("2027-02"), serde_json::json!("h001")],
            ),
            (
                TARGET_ROLLOUT_RATED_SHADOW_PERIOD,
                "SELECT state,mode,total_satang,price_id,meter_snapshot_id,lines_json FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2",
                vec![serde_json::json!("h001"), serde_json::json!("2027-02")],
                vec![serde_json::json!("h001")],
            ),
        ] {
            let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: values.clone(),
                now: 1,
            })
            .unwrap_or_else(|_| panic!("{kind} is an allow-listed owner read kind"));
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.values, values);
            // A missing or widened bind cannot broaden an owner-local read.
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![],
                now: 1,
            })
            .is_err());
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: wrong,
                now: 1,
            })
            .is_err());
        }
        assert_ne!(TARGET_ROLLOUT_EFFECTIVE_PRICE, TARGET_ROLLOUT_RATED_SHADOW_PERIOD);
    }
    #[test]
    fn approved_rated_period_read_is_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: APPROVED_RATED_PERIOD.into(),
            values: vec![serde_json::json!("h001"), serde_json::json!("2027-02")],
            now: 1,
        })
        .expect("approved_rated_period is an allow-listed owner read kind");
        assert_eq!(
            statement.sql,
            "SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'"
        );
        assert_eq!(
            statement.values,
            vec![serde_json::json!("h001"), serde_json::json!("2027-02")]
        );
        // A missing or widened bind cannot broaden an owner-local read.
        assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: APPROVED_RATED_PERIOD.into(),
            values: vec![serde_json::json!("h001")],
            now: 1,
        })
        .is_err());
        assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: APPROVED_RATED_PERIOD.into(),
            values: vec![serde_json::json!("h001"), serde_json::json!("2027-02"), serde_json::json!("extra")],
            now: 1,
        })
        .is_err());
    }
    #[test]
    fn active_target_enrollment_read_is_owner_authored_and_single_bound() {
        let holding = HoldingId::new("h001").unwrap();
        let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: ACTIVE_TARGET_ENROLLMENT.into(),
            values: vec![serde_json::json!("h001")],
            now: 1,
        })
        .expect("active_target_enrollment is an allow-listed owner read kind");
        assert_eq!(
            statement.sql,
            "SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL"
        );
        assert_eq!(statement.values, vec![serde_json::json!("h001")]);
        // The holding is the only bind and the plan/active predicates are
        // owner-authored; neither a missing nor a widened bind is accepted.
        for values in [vec![], vec![serde_json::json!("h001"), serde_json::json!("extra")]] {
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: ACTIVE_TARGET_ENROLLMENT.into(),
                values,
                now: 1,
            })
            .is_err());
        }
    }
    #[test]
    fn selected_and_current_target_price_reads_are_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        for (kind, sql, values, wrong) in [
            (
                SELECTED_TARGET_PRICE,
                "SELECT * FROM billing_target_prices WHERE id=$1 AND effective_month<=$2",
                vec![serde_json::json!("price-1"), serde_json::json!("2027-02")],
                vec![serde_json::json!("price-1")],
            ),
            (
                CURRENT_TARGET_PRICE,
                "SELECT * FROM billing_target_prices WHERE effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1",
                vec![serde_json::json!("2027-02")],
                vec![serde_json::json!("2027-02"), serde_json::json!("price-1")],
            ),
        ] {
            let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: values.clone(),
                now: 1,
            })
            .unwrap_or_else(|_| panic!("{kind} is an allow-listed owner read kind"));
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.values, values);
            // A missing or widened bind cannot broaden an owner-local price read.
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![],
                now: 1,
            })
            .is_err());
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: wrong,
                now: 1,
            })
            .is_err());
        }
        assert_ne!(SELECTED_TARGET_PRICE, CURRENT_TARGET_PRICE);
    }
    #[test]
    fn target_rollout_operating_and_capacity_reads_are_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        for (kind, sql, values, wrong) in [
            (
                TARGET_ROLLOUT_AVAILABLE_OPERATING_COSTS,
                "SELECT o.category,o.amount_satang,e.source_state\n       FROM billing_target_operating_costs o\n       JOIN billing_evidence e ON e.id=o.evidence_id\n       WHERE o.holding_id=$1 AND o.month=$2",
                vec![serde_json::json!("h001"), serde_json::json!("2027-02")],
                vec![serde_json::json!("h001")],
            ),
            (
                TARGET_ROLLOUT_CAPACITY_REVIEW,
                "SELECT r.*,e.source_state FROM billing_target_capacity_reviews r\n     JOIN billing_evidence e ON e.id=r.evidence_id\n     WHERE r.holding_id=$1 AND r.effective_month=$2",
                vec![serde_json::json!("h001"), serde_json::json!("2027-02")],
                vec![serde_json::json!("h001"), serde_json::json!("2027-02"), serde_json::json!("extra")],
            ),
        ] {
            let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: values.clone(),
                now: 1,
            })
            .unwrap_or_else(|_| panic!("{kind} is an allow-listed owner read kind"));
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.values, values);
            // A missing or widened bind cannot broaden an owner-local read.
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: vec![],
                now: 1,
            })
            .is_err());
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: wrong,
                now: 1,
            })
            .is_err());
        }
        assert_ne!(TARGET_ROLLOUT_AVAILABLE_OPERATING_COSTS, TARGET_ROLLOUT_CAPACITY_REVIEW);
    }
    #[test]
    fn target_statements_read_is_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: TARGET_STATEMENTS.into(),
            values: vec![serde_json::json!("h001"), serde_json::json!("2027-02")],
            now: 1,
        })
        .expect("target_statements is an allow-listed owner read kind");
        assert_eq!(
            statement.sql,
            "SELECT * FROM billing_target_statements WHERE holding_id=$1 AND month=$2"
        );
        assert_eq!(
            statement.values,
            vec![serde_json::json!("h001"), serde_json::json!("2027-02")]
        );
        // Both the holding and month must be bound; a partial bind cannot widen
        // an owner-local statements read.
        for values in [vec![serde_json::json!("h001")], vec![]] {
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: TARGET_STATEMENTS.into(),
                values,
                now: 1,
            })
            .is_err());
        }
    }
    #[test]
    fn api_evidence_and_account_reads_are_owner_authored_and_fully_bound() {
        let holding = HoldingId::new("h001").unwrap();
        for (kind, sql, values, missing) in [
            (
                EVIDENCE_SCOPED_READ,
                "SELECT * FROM billing_evidence WHERE id=$1 AND holding_id=$2",
                vec![serde_json::json!("ev-1"), serde_json::json!("h001")],
                vec![serde_json::json!("ev-1")],
            ),
            (
                EVIDENCE_EXISTING_READ,
                "SELECT * FROM billing_evidence WHERE id=$1",
                vec![serde_json::json!("ev-1")],
                vec![],
            ),
            (
                BILLING_ACCOUNT_EXISTS,
                "SELECT 1 FROM billing_accounts WHERE holding_id=$1",
                vec![serde_json::json!("h001")],
                vec![],
            ),
        ] {
            let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: values.clone(),
                now: 1,
            })
            .unwrap_or_else(|_| panic!("{kind} is an allow-listed owner read kind"));
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.values, values);
            // A missing witness bind cannot widen an owner read.
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: kind.into(),
                values: missing,
                now: 1,
            })
            .is_err());
        }
        // The scan-source and API evidence reads are independent kinds: a
        // caller of one cannot silently switch statement shape to the other.
        assert_ne!(SCAN_SOURCE, EVIDENCE_EXISTING_READ);
    }
    #[test]
    fn scan_source_read_is_owner_authored_and_single_bound() {
        let holding = HoldingId::new("h001").unwrap();
        let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: SCAN_SOURCE.into(),
            values: vec![serde_json::json!("ev-1")],
            now: 1,
        })
        .expect("scan_source is an allow-listed owner read kind");
        assert_eq!(
            statement.sql,
            "SELECT * FROM billing_evidence WHERE id=$1"
        );
        assert_eq!(statement.values, vec![serde_json::json!("ev-1")]);
        assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: SCAN_SOURCE.into(),
            values: vec![],
            now: 1,
        })
        .is_err());
    }
    #[test]
    fn app_finance_projection_read_is_owner_authored_and_holding_scoped() {
        let holding = HoldingId::new("h001").unwrap();
        let statement = billing_owner_statement(&holding, &BillingOwnerWriteInput {
            kind: APP_FINANCE_PROJECTION.into(),
            values: vec![serde_json::json!("inv-1"), serde_json::json!("h001")],
            now: 1,
        })
        .expect("app_finance_projection is an allow-listed owner read kind");
        assert_eq!(
            statement.sql,
            "SELECT invoice_satang,receivable_satang,received_satang,reversed_satang,credited_satang,stripe_status,synced_at FROM billing_test_finance_projection WHERE billing_invoice_id=$1 AND customer_holding_id=$2"
        );
        assert_eq!(
            statement.values,
            vec![serde_json::json!("inv-1"), serde_json::json!("h001")]
        );
        // Both the invoice and its owning holding must be bound; a missing
        // scope or a single bind cannot widen this owner projection read.
        for values in [
            vec![serde_json::json!("inv-1")],
            vec![],
        ] {
            assert!(billing_owner_statement(&holding, &BillingOwnerWriteInput {
                kind: APP_FINANCE_PROJECTION.into(),
                values,
                now: 1,
            })
            .is_err());
        }
    }
}
