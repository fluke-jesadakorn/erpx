//! Native PMS owner-write kind registry (wf1a2 full retirement slice).
//!
//! The authoritative SQL and its positional argument order for EVERY retained
//! TypeScript write literal in `packages/server-pms/src/**/*.ts`. Callers send
//! only an allow-listed `kind` plus ordered values; the SQL text is never
//! transported or reconstructed from caller input. The axum executor runs the
//! selected statement on a caller-owned serializable unit with
//! receipt/audit/outbox effects. No TypeScript SQL copy or fallback remains.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

/// One authoritative owner write: kind, exact SQL and caller-owned values.
#[derive(Clone, Debug)]
pub struct PmsOwnerStatement {
    pub kind: &'static str,
    pub sql: String,
    pub values: Vec<Value>,
}

/// Every write kind accepted by the native executor. Anything else fails closed.
pub const KINDS: &[&str] = &[
    "pms_w001", // hospitality-rollout.ts
    "pms_w002", // pms-booking-changes.ts
    "pms_w003", // pms-booking-changes.ts
    "pms_w004", // pms-booking-changes.ts
    "pms_w005", // pms-booking-changes.ts
    "pms_w006", // pms-booking-changes.ts
    "pms_w007", // pms-booking-changes.ts
    "pms_w008", // pms-booking-changes.ts
    "pms_w009", // pms-booking-refunds.ts
    "pms_w010", // pms-booking-refunds.ts
    "pms_w011", // pms-booking-refunds.ts
    "pms_w012", // pms-booking-refunds.ts
    "pms_w013", // pms-booking-refunds.ts
    "pms_w014", // pms-booking-refunds.ts
    "pms_w015", // pms-booking-refunds.ts
    "pms_w016", // pms-booking-refunds.ts
    "pms_w017", // pms-booking-refunds.ts
    "pms_w018", // pms-booking-refunds.ts
    "pms_w019", // pms-booking-refunds.ts
    "pms_w020", // pms-booking-refunds.ts
    "pms_w021", // pms-booking-refunds.ts
    "pms_w022", // pms-booking-refunds.ts
    "pms_w023", // pms-booking-refunds.ts
    "pms_w024", // pms-booking-refunds.ts
    "pms_w025", // pms-booking.ts
    "pms_w026", // pms-booking.ts
    "pms_w027", // pms-booking.ts
    "pms_w028", // pms-booking.ts
    "pms_w029", // pms-booking.ts
    "pms_w030", // pms-booking.ts
    "pms_w031", // pms-booking.ts
    "pms_w032", // pms-booking.ts
    "pms_w033", // pms-booking.ts
    "pms_w034", // pms-booking.ts
    "pms_w035", // pms-booking.ts
    "pms_w036", // pms-booking.ts
    "pms_w037", // pms-booking.ts
    "pms_w038", // pms-booking.ts
    "pms_w039", // pms-booking.ts
    "pms_w040", // pms-booking.ts
    "pms_w041", // pms-booking.ts
    "pms_w042", // pms-booking.ts
    "pms_w043", // pms-booking.ts
    "pms_w044", // pms-booking.ts
    "pms_w045", // pms-booking.ts
    "pms_w046", // pms-booking.ts
    "pms_w047", // pms-booking.ts
    "pms_w048", // pms-booking.ts
    "pms_w049", // pms-stripe.ts
    "pms_w050", // pms-stripe.ts
    "pms_w051", // pms-stripe.ts
    "pms_w052", // pms-stripe.ts
    "pms_w053", // pms-stripe.ts
    "pms_w054", // pms-stripe.ts
    "pms_w055", // pms-stripe.ts
    "pms_w056", // pms-stripe.ts
    "pms_w057", // pms-stripe.ts
    "pms_w058", // pms-stripe.ts
    "pms_w059", // pms-stripe.ts
    "pms_w060", // pms-stripe.ts
    "pms_w061", // pms-stripe.ts
    "pms_w062", // pms-stripe.ts
    "pms_w063", // pms-stripe.ts
    "pms_w064", // pms-stripe.ts
    "pms_w065", // pms-stripe.ts
    "pms_w066", // pms-stripe.ts
    "pms_w067", // pms-stripe.ts
    "pms_w068", // pms-stripe.ts
    "pms_w069", // pms-stripe.ts
    "pms_w070", // pms-stripe.ts
    "pms_w071", // pms-stripe.ts
    "pms_w072", // pms-stripe.ts
    "pms_w073", // pms-stripe.ts
    "pms_w074", // pms.ts
    "pms_w075", // pms.ts
    "pms_w076", // pms.ts
    "pms_w077", // pms.ts
    "pms_w078", // pms.ts
    "pms_w079", // pms.ts
    "pms_w080", // pms.ts
    "pms_w081", // pms.ts
    "pms_w082", // pms.ts
    "pms_w083", // pms.ts
    "pms_w084", // pms.ts
    "pms_w085", // pms.ts
    "pms_w086", // pms.ts
    "pms_w087", // pms.ts
    "pms_w088", // pms.ts
    "pms_w089", // pms.ts
    "pms_w090", // pms.ts
    "pms_w091", // pms.ts
    "pms_w092", // pms.ts
    "pms_w093", // pms.ts
    "pms_w094", // pms.ts
    "pms_w095", // pms.ts
    "pms_w096", // pms.ts
    "pms_w097", // pms.ts
    "pms_w098", // pos-stripe-facts.ts
    "pms_w099", // pos-stripe-facts.ts
];

pub fn supported(kind: &str) -> bool { KINDS.contains(&kind) }

pub fn invalid() -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, "PMS_OWNER_WRITE_INVALID", "Review the hotel write request.")
}

/// Return the authoritative SQL for an allow-listed owner write kind.
fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "pms_w001" => r#"INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at)
        VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,module_id) DO UPDATE SET
        rollout_state=excluded.rollout_state,version=excluded.version,reason=excluded.reason,
        changed_by=excluded.changed_by,changed_at=excluded.changed_at"#,
        "pms_w002" => r#"UPDATE pms_booking_changes SET state='expired' WHERE reservation_id=? AND state='payment_pending' AND expires_at<=?"#,
        "pms_w003" => r#"INSERT INTO pms_booking_changes(id,property_id,reservation_id,request_hash,reservation_version,proposal_json,state,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)"#,
        "pms_w004" => r#"INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,status,expires_at,created_at,updated_at,allocation_json,methods_json,clearing_account,purpose,proposal_id) VALUES(?,?,?,?,?,?,?,?,'THB','payment_pending',?,?,?,?,?,?,'change',?)"#,
        "pms_w005" => r#"INSERT INTO pms_booking_allocations(id,payment_id,stay_id,guest_account_id,entry_id,amount_minor,created_at) VALUES(?,?,?,?,?,?,?)"#,
        "pms_w006" => r#"INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,'[]','Change payment could not be assigned','system:stripe','requested',?,?)"#,
        "pms_w007" => r#"UPDATE pms_booking_changes SET state=? WHERE id=?"#,
        "pms_w008" => r#"UPDATE pms_booking_payments SET status=?,refund_state=?,updated_at=? WHERE id=?"#,
        "pms_w009" => r#"INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,?,?,?,'requested',?,?)"#,
        "pms_w010" => r#"UPDATE pms_booking_payments SET refund_state='requested',refund_requested_at=?,updated_at=? WHERE id=?"#,
        "pms_w011" => r#"UPDATE pms_booking_refund_requests SET state='approved',legacy_review=false,approved_by=?,updated_at=? WHERE id=?"#,
        "pms_w012" => r#"UPDATE pms_booking_payments SET refund_state='approved',refund_approved_by=?,updated_at=? WHERE id=?"#,
        "pms_w013" => r#"UPDATE pms_booking_payments SET paid_at=?,provider_succeeded_at=?,payment_intent_id=?,last_error=NULL,updated_at=? WHERE id=?"#,
        "pms_w014" => r#"INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING"#,
        "pms_w015" => r#"UPDATE pms_booking_refund_requests SET provider_id=?,state='processing',updated_at=? WHERE id=? AND provider_id IS NULL"#,
        "pms_w016" => r#"UPDATE pms_booking_payments SET refund_id=?,refund_state='processing',updated_at=? WHERE id=?"#,
        "pms_w017" => r#"UPDATE pms_booking_refund_requests SET state='failed',last_error='Provider refund failed; liability remains outstanding',updated_at=? WHERE id=?"#,
        "pms_w018" => r#"UPDATE pms_booking_payments SET refund_state='failed',updated_at=? WHERE id=?"#,
        "pms_w019" => r#"UPDATE pms_booking_refund_requests SET state=?,updated_at=? WHERE id=?"#,
        "pms_w020" => r#"UPDATE pms_booking_refund_requests SET state='succeeded',last_error=NULL,updated_at=? WHERE id=?"#,
        "pms_w021" => r#"UPDATE pms_booking_payments SET refund_state='succeeded',updated_at=? WHERE id=?"#,
        "pms_w022" => r#"INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING"#,
        "pms_w023" => r#"UPDATE pms_booking_refund_requests SET generation=generation+1,provider_id=NULL,state='approved',last_error=?,updated_at=? WHERE id=?"#,
        "pms_w024" => r#"UPDATE pms_booking_payments SET refund_state='approved',updated_at=? WHERE id=?"#,
        "pms_w025" => r#"UPDATE pms_properties SET version=?,updated_at=? WHERE id=?"#,
        "pms_w026" => r#"INSERT INTO pms_booking_tokens(id,property_id,reservation_id,token_hash,expires_at,created_at) VALUES(?,?,?,?,?,?)"#,
        "pms_w027" => r#"INSERT INTO pms_booking_emails(id,property_id,reservation_id,token_id,holding_slug,kind,payload_json,next_attempt_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)"#,
        "pms_w028" => r#"INSERT INTO pms_booking_receipts(id,property_id,body_hash,reservation_id,created_at,response_json) VALUES(?,?,?,?,?,?)"#,
        "pms_w029" => r#"INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,status,expires_at,created_at,updated_at,allocation_json,methods_json,clearing_account) VALUES(?,?,?,?,?,?,?,?,'THB','payment_pending',?,?,?,?,?,?)"#,
        "pms_w030" => r#"INSERT INTO pms_booking_receipts(id,property_id,body_hash,reservation_id,created_at,response_json) VALUES(?,?,?,?,?,?)"#,
        "pms_w031" => r#"UPDATE pms_booking_payments SET checkout_id=?,checkout_url=?,updated_at=? WHERE id=? AND checkout_id IS NULL AND status='payment_pending'"#,
        "pms_w032" => r#"UPDATE pms_booking_payments SET payment_intent_id=?,paid_at=COALESCE(paid_at,?) WHERE id=?"#,
        "pms_w033" => r#"UPDATE pms_booking_payments SET payment_intent_id=?,provider_succeeded_at=COALESCE(provider_succeeded_at,?),paid_at=? WHERE id=?"#,
        "pms_w034" => r#"UPDATE pms_booking_payments SET payment_intent_id=?,provider_succeeded_at=?,paid_at=? WHERE id=?"#,
        "pms_w035" => r#"INSERT INTO pms_booking_allocations(id,payment_id,stay_id,guest_account_id,entry_id,amount_minor,created_at) VALUES(?,?,?,?,?,?,?)"#,
        "pms_w036" => r#"UPDATE pms_booking_payments SET status='confirmed',updated_at=? WHERE id=?"#,
        "pms_w037" => r#"INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,'[]','Payment cannot be assigned','system:stripe','requested',?,?)"#,
        "pms_w038" => r#"UPDATE pms_booking_payments SET status=?,refund_state=?,refund_requested_at=?,updated_at=? WHERE id=?"#,
        "pms_w039" => r#"UPDATE pms_booking_payments SET last_error=NULL,updated_at=? WHERE id=?"#,
        "pms_w040" => r#"UPDATE pms_booking_payments SET last_error=?,updated_at=? WHERE id=?"#,
        "pms_w041" => r#"UPDATE pms_booking_refund_requests SET last_error=?,updated_at=? WHERE payment_id=? AND state IN ('approved','processing','requires_action')"#,
        "pms_w042" => r#"INSERT INTO pms_booking_limits(key,bucket,count) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET count=CASE WHEN pms_booking_limits.bucket=excluded.bucket THEN pms_booking_limits.count+1 ELSE 1 END,bucket=excluded.bucket RETURNING count"#,
        "pms_w043" => r#"DELETE FROM pms_booking_limits WHERE bucket<?"#,
        "pms_w044" => r#"UPDATE pms_booking_emails SET state='sending',attempts=attempts+1,lease_until=? WHERE id=(SELECT id FROM pms_booking_emails WHERE (state='pending' OR (state='sending' AND lease_until<?)) AND next_attempt_at<=? ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING *"#,
        "pms_w045" => r#"UPDATE pms_booking_tokens SET expires_at=? WHERE id=?"#,
        "pms_w046" => r#"UPDATE pms_booking_emails SET state='pending',attempts=GREATEST(0,attempts-1),lease_until=NULL,next_attempt_at=?,last_error=? WHERE id=?"#,
        "pms_w047" => r#"UPDATE pms_booking_emails SET state='sent',sent_at=?,lease_until=NULL,last_error=NULL WHERE id=?"#,
        "pms_w048" => r#"UPDATE pms_booking_emails SET state=?,next_attempt_at=?,lease_until=NULL,last_error='EMAIL_DELIVERY_FAILED' WHERE id=?"#,
        "pms_w049" => r#"INSERT INTO pms_stripe_payments(id,request_hash,property_id,guest_account_id,company_id,account_id,livemode,kind,amount_minor,payer_email,state,expires_at,created_by,created_at,updated_at,clearing_account) VALUES(?,?,?,?,?,?,?,?,?,?,'requested',?,?,?,?,?)"#,
        "pms_w050" => r#"UPDATE pms_stripe_payments SET provider_id=?,qr_data=?,state='pending',updated_at=? WHERE id=? AND state='requested'"#,
        "pms_w051" => r#"UPDATE pms_stripe_payments SET state='cancelled',updated_at=? WHERE id=?"#,
        "pms_w052" => r#"UPDATE pms_stripe_payments SET received_at=COALESCE(received_at,?) WHERE id=?"#,
        "pms_w053" => r#"UPDATE pms_stripe_payments SET received_at=COALESCE(received_at,?),provider_succeeded_at=COALESCE(provider_succeeded_at,?) WHERE id=?"#,
        "pms_w054" => r#"UPDATE pms_stripe_payments SET state='refund_review',entry_id=?,received_at=?,refund_state='requested',refund_amount_minor=?,refund_reason='Payment received after the QR hold expired',refund_requested_by='system:stripe',updated_at=? WHERE id=?"#,
        "pms_w055" => r#"UPDATE pms_properties SET version=?,updated_at=? WHERE id=?"#,
        "pms_w056" => r#"UPDATE pms_stripe_payments SET state='succeeded',entry_id=?,received_at=?,updated_at=? WHERE id=?"#,
        "pms_w057" => r#"UPDATE pms_stripe_payments SET state='cancelled',updated_at=? WHERE id=? AND state IN ('requested','pending')"#,
        "pms_w058" => r#"UPDATE pms_stripe_payments SET last_error=NULL WHERE id=?"#,
        "pms_w059" => r#"UPDATE pms_stripe_payments SET last_error=? WHERE id=?"#,
        "pms_w060" => r#"UPDATE pms_stripe_payments SET last_error=? WHERE id=?"#,
        "pms_w061" => r#"UPDATE pms_stripe_payments SET refund_generation=refund_generation+CASE WHEN refund_state='succeeded' THEN 1 ELSE 0 END,refund_id=NULL,refund_approved_by=NULL,refund_liability_posted=false,refund_state='requested',refund_reserved=true,refund_amount_minor=?,refund_reason=?,refund_requested_by=?,updated_at=? WHERE id=?"#,
        "pms_w062" => r#"UPDATE pms_stripe_payments SET refund_state='approved',refund_reserved=false,refund_liability_posted=true,refund_approved_by=?,updated_at=? WHERE id=?"#,
        "pms_w063" => r#"INSERT INTO pms_counter_refund_attempts(payment_id,generation,provider_id,amount_minor,reason,requested_by,approved_by,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'processing',?,?) ON CONFLICT(payment_id,generation) DO NOTHING"#,
        "pms_w064" => r#"UPDATE pms_stripe_payments SET refund_id=?,refund_state='processing',updated_at=? WHERE id=? AND refund_id IS NULL"#,
        "pms_w065" => r#"UPDATE pms_counter_refund_attempts SET state='failed',updated_at=? WHERE payment_id=? AND generation=?"#,
        "pms_w066" => r#"UPDATE pms_stripe_payments SET refund_state='failed',updated_at=? WHERE id=?"#,
        "pms_w067" => r#"UPDATE pms_stripe_payments SET refund_state=?,updated_at=? WHERE id=?"#,
        "pms_w068" => r#"UPDATE pms_counter_refund_attempts SET state=?,updated_at=? WHERE payment_id=? AND generation=?"#,
        "pms_w069" => r#"UPDATE pms_properties SET version=?,updated_at=? WHERE id=?"#,
        "pms_w070" => r#"UPDATE pms_counter_refund_attempts SET state='succeeded',updated_at=? WHERE payment_id=? AND generation=?"#,
        "pms_w071" => r#"UPDATE pms_stripe_payments SET refund_state='succeeded',updated_at=? WHERE id=?"#,
        "pms_w072" => r#"UPDATE pms_stripe_payments SET refund_id=NULL,refund_generation=refund_generation+1,refund_state='approved',updated_at=? WHERE id=?"#,
        "pms_w073" => r#"UPDATE pms_stripe_payments SET payer_email=COALESCE(?,payer_email),last_error=?,updated_at=? WHERE id=?"#,
        "pms_w074" => r#"INSERT INTO pms_postings(id,property_id,source_key,record_id,journal_id,business_date) VALUES(?,?,?,?,?,?)"#,
        "pms_w075" => r#"INSERT INTO pms_stock_moves(id,property_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at,source_number) VALUES(?,?,?,?,?,?,?,?,?)"#,
        "pms_w076" => r#"INSERT INTO pms_sources(property_id,source_key,record_id) VALUES(?,?,?)"#,
        "pms_w077" => r#"INSERT INTO pms_documents(id,property_id,guest_account_id,kind,finance_id,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?)"#,
        "pms_w078" => r#"INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,?,?,?,?,?)"#,
        "pms_w079" => r#"INSERT INTO pms_charge_corrections(id,company_id,property_id,entry_id,guest_account_id,status,version,reason,requested_by,created_at,updated_at)
            VALUES(?,?,?,?,?,'pending',1,?,?,?,?)"#,
        "pms_w080" => r#"INSERT INTO pms_charge_correction_events(id,correction_id,company_id,property_id,entry_id,action,actor_id,correction_version,reason,created_at)
            VALUES(?,?,?,?,?,'requested',?,1,?,?)"#,
        "pms_w081" => r#"UPDATE pms_properties SET version=?,updated_at=? WHERE id=? AND company_id=? AND version=?"#,
        "pms_w082" => r#"UPDATE pms_charge_corrections SET status=?,version=?,reviewed_by=?,review_reason=?,reversal_entry_id=?,reviewed_at=?,updated_at=?
            WHERE id=? AND company_id=? AND property_id=? AND status='pending' AND version=?"#,
        "pms_w083" => r#"INSERT INTO pms_charge_correction_events(id,correction_id,company_id,property_id,entry_id,action,actor_id,correction_version,reason,reversal_entry_id,created_at)
            VALUES(?,?,?,?,?,?,?,?,?,?,?)"#,
        "pms_w084" => r#"UPDATE pms_properties SET version=?,updated_at=? WHERE id=? AND company_id=? AND version=?"#,
        "pms_w085" => r#"INSERT INTO pms_transfer_requests(id,company_id,property_id,guest_account_id,shift_id,kind,business_date,currency,amount_minor,reference,state,recorded_by,created_at,updated_at)
            VALUES(?,?,?,?,?,?,?,?,?,?,'pending',?,?,?)"#,
        "pms_w086" => r#"UPDATE pms_transfer_requests SET state='cancelled',cancelled_by=?,version=version+1,updated_at=? WHERE id=? AND state='pending' AND version=?"#,
        "pms_w087" => r#"UPDATE pms_booking_emails SET state='pending',attempts=0,next_attempt_at=?,lease_until=NULL,last_error=NULL WHERE id=? AND property_id=? AND state='failed' RETURNING id"#,
        "pms_w088" => r#"UPDATE pms_documents SET guest_account_id=? WHERE property_id=? AND guest_account_id=?"#,
        "pms_w089" => r#"UPDATE pms_transfer_requests SET state='verified',verified_by=?,bank_transaction_id=?,entry_id=?,finance_payment_id=?,version=version+1,updated_at=? WHERE id=? AND state='pending'"#,
        "pms_w090" => r#"UPDATE pms_properties SET name=?,business_date=?,state=?,config_json=?,version=?,updated_at=? WHERE id=?"#,
        "pms_w091" => r#"INSERT INTO pms_jobs(id,property_id,business_date,actor_json,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(property_id,business_date) DO UPDATE SET state='pending',actor_json=excluded.actor_json,error_code=NULL,updated_at=excluded.updated_at"#,
        "pms_w092" => r#"UPDATE pms_jobs SET state=?,error_code=NULL,updated_at=? WHERE id=?"#,
        "pms_w093" => r#"UPDATE pms_jobs SET state='blocked',error_code=?,updated_at=? WHERE id=? AND state!='completed'"#,
        "pms_w094" => r#"UPDATE pms_records SET data_json=jsonb_set(jsonb_set(data_json::jsonb,'{state}','\"blocked\"'),'{message}',to_jsonb(?::text))::text,version=version+1,updated_at=? WHERE id=? AND data_json::jsonb->>'state'!='completed'"#,
        "pms_w095" => r#"UPDATE pms_properties SET version=?,updated_at=? WHERE id=?"#,
        "pms_w096" => r#"UPDATE pms_properties SET version=?,updated_at=? WHERE id=?"#,
        "pms_w097" => r#"INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at)
          SELECT row->>'id',row->>'id2',row->>'kind',row->>'fact3',
            (row->>'version')::bigint,(row->>'updatedAt')::bigint
          FROM jsonb_array_elements(?::jsonb) WITH ORDINALITY AS input(row,ordinal)
          ORDER BY ordinal
          ON CONFLICT(id) DO UPDATE SET data_json=excluded.data_json,version=excluded.version,updated_at=excluded.updated_at"#,
        "pms_w098" => r#"UPDATE pms_stripe_payments SET provider_succeeded_at=COALESCE(provider_succeeded_at,?) WHERE id=?"#,
        "pms_w099" => r#"UPDATE pms_booking_payments SET provider_succeeded_at=COALESCE(provider_succeeded_at,?) WHERE id=?"#,
        _ => return None,
    })
}

/// Build the authoritative owner write after validating the kind and that the
/// caller supplied exactly one value per SQL placeholder.
pub fn statement(kind: &str, values: Vec<Value>) -> Result<PmsOwnerStatement, BusinessError> {
    if !supported(kind) { return Err(invalid()); }
    let sql = sql(kind).ok_or_else(invalid)?;
    if values.is_empty() || values.len() > 64 || values.len() != sql.matches('?').count() {
        return Err(invalid());
    }
    let kind = KINDS.iter().copied().find(|candidate| *candidate == kind).ok_or_else(invalid)?;
    Ok(PmsOwnerStatement { kind, sql: sql.to_owned(), values })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_kind_is_allow_listed_with_parameterized_sql() {
        for kind in KINDS {
            assert!(supported(kind));
            let s = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(s.contains('?'), "write must be parameterized: {kind}");
            assert!(!s.trim().is_empty());
        }
    }
    #[test]
    fn unknown_and_wrong_arity_fail_closed() {
        assert!(!supported("not_a_pms_write"));
        assert!(statement("not_a_pms_write", vec![Value::Null]).is_err());
        let kind = KINDS[0];
        assert!(statement(kind, vec![]).is_err());
    }
    #[test]
    fn exact_arity_builds_statement() {
        for kind in KINDS {
            let n = sql(kind).unwrap().matches('?').count();
            let values = vec![Value::String("x".into()); n];
            let built = statement(kind, values).unwrap();
            assert_eq!(built.kind, *kind);
        }
    }
}
