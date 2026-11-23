//! Owner-owned SQL statements for the Finance owner-write slice.
//!
//! This is the authoritative owner model for the retired TypeScript
//! `native*-sql` bridges. The SQL and its argument order live here, in the
//! Finance owner crate. A transport calls [`statement_sql`] with a typed
//! allow-listed kind and caller-supplied positional values. No TypeScript SQL
//! copy and no fallback writer remain for a retired kind.
use crate::application::finance_owner_write::OwnerStatementPort;
use crate::domain::attachment_version::attachment_version_target;
use crate::domain::finance_owner_write::ATTACHMENT_VERSION_STEP_PREFIX;

/// Canonical one-row money snapshot insert, repeated safely by
/// [`statement_sql`] for the bounded batch kind. The row template is a trusted
/// owner constant; only its repetition count comes from the caller.
const MONEY_SNAPSHOT_BATCH_PREFIX: &str = "INSERT INTO money_transaction_snapshots\n            (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)\n            VALUES ";
const MONEY_SNAPSHOT_BATCH_ROW: &str = "(?,?,?,?,?,?,?,?,?,?,?,?)";
pub const MONEY_SNAPSHOT_BATCH_KIND: &str = "money_snapshot_batch_insert";
pub const MONEY_SNAPSHOT_BATCH_COLUMNS: usize = 12;
pub const MONEY_SNAPSHOT_BATCH_MAX_ROWS: usize = 500;

/// Look up the canonical SQL for one allow-listed kind. Unknown kinds return
/// `None`; the application layer fails closed before any SQL is returned.
/// The money snapshot batch statement is assembled in Rust from a trusted row
/// template, never from caller-supplied SQL text.
pub fn statement_sql(kind: &str, value_count: usize) -> Option<String> {
    // Native attachment version steps are admitted through the same generic
    // owner-write ingress. The suffix is an exact allow-listed target type;
    // the descriptor owns the table, column, extra predicate and arity.
    if let Some(target_type) = kind.strip_prefix(ATTACHMENT_VERSION_STEP_PREFIX) {
        let target = attachment_version_target(target_type)?;
        let (sql, arity) = target.step_update_unscoped_q();
        return (value_count == arity).then_some(sql);
    }
    if kind == MONEY_SNAPSHOT_BATCH_KIND {
        if value_count == 0
            || value_count % MONEY_SNAPSHOT_BATCH_COLUMNS != 0
            || value_count / MONEY_SNAPSHOT_BATCH_COLUMNS > MONEY_SNAPSHOT_BATCH_MAX_ROWS
        {
            return None;
        }
        let rows = value_count / MONEY_SNAPSHOT_BATCH_COLUMNS;
        let mut sql = String::with_capacity(
            MONEY_SNAPSHOT_BATCH_PREFIX.len() + rows * (MONEY_SNAPSHOT_BATCH_ROW.len() + 1),
        );
        sql.push_str(MONEY_SNAPSHOT_BATCH_PREFIX);
        for index in 0..rows {
            if index > 0 {
                sql.push(',');
            }
            sql.push_str(MONEY_SNAPSHOT_BATCH_ROW);
        }
        return Some(sql);
    }
    static_sql(kind).map(str::to_owned)
}

fn static_sql(kind: &str) -> Option<&'static str> {
    match kind {
        "finance_stmt_1" => Some("INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)"),
        "finance_stmt_2" => Some("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,parent_id,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_3" => Some("UPDATE finance_accounts SET status=?,version=version+1,updated_at=? WHERE id=? AND chart_id=? AND company_id=? AND version=?"),
        "finance_stmt_4" => Some("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?,?,?,?)"),
        "finance_stmt_5" => Some("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,payment_terms_days,credit_limit_minor,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_6" => Some("INSERT INTO finance_tax_codes(id,company_id,code,name,rate_micros,recoverable_micros,effective_from,effective_to,status) VALUES(?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_7" => Some("INSERT INTO finance_budgets(id,company_id,name,fiscal_year,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)"),
        "finance_stmt_8" => Some("INSERT INTO finance_leases(id,company_id,lease_number,counterparty,currency,commencement_date,end_date,payment_frequency,payment_minor,discount_rate_micros,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_9" => Some("INSERT INTO finance_loans(id,company_id,loan_number,lender,currency,principal_minor,annual_rate_micros,commencement_date,maturity_date,outstanding_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_10" => Some("INSERT INTO finance_revenue_contracts(id,company_id,customer_id,contract_number,currency,transaction_price_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_11" => Some("INSERT INTO finance_revenue_obligations(id,contract_id,description,allocated_minor,recognition_method,start_date,end_date,revenue_account_id,deferred_account_id) VALUES(?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_12" => Some("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(chart_id,code) DO UPDATE SET name=excluded.name,category=excluded.category,currency=excluded.currency,is_control=excluded.is_control,control_kind=excluded.control_kind,version=finance_accounts.version+1,updated_at=excluded.updated_at"),
        "finance_stmt_13" => Some("UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=?"),
        "finance_stmt_14" => Some("UPDATE finance_invoices SET status='posted',approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE company_id=? AND source_type='opening_import' AND source_id=? AND status='draft'"),
        "finance_stmt_15" => Some("INSERT INTO finance_batch_review_items(command_id,journal_id,company_id,reviewed_version,reviewed_by,assignment_id,evidence_digest,reason,reviewed_at) VALUES(?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_16" => Some("UPDATE finance_invoices SET party_id=?,kind=?,invoice_number=?,invoice_date=?,due_date=?,currency=?,subtotal_minor=?,tax_minor=?,total_minor=?,outstanding_minor=?,tax_code=?,debit_account_id=?,credit_account_id=?,tax_account_id=?,created_by=?,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_17" => Some("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_18" => Some("UPDATE finance_invoices SET line_mode=?,line_revision=? WHERE id=? AND company_id=?"),
        "finance_stmt_19" => Some("UPDATE finance_invoices SET status='posted',journal_id=?,fx_rate_micros=?,base_total_minor=?,base_outstanding_minor=?,approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_20" => Some("UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_21" => Some("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_22" => Some("UPDATE finance_payments SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?"),
        "finance_stmt_23" => Some("UPDATE finance_payments SET status='released',released_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?"),
        "finance_stmt_24" => Some("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_25" => Some("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=base_outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_26" => Some("UPDATE finance_payments SET outstanding_minor=0,status='settled',journal_id=?,fx_rate_micros=?,base_amount_minor=?,fx_journal_id=?,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_27" => Some("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?"),
        "finance_stmt_28" => Some("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?"),
        "finance_stmt_29" => Some("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?"),
        "finance_stmt_30" => Some("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?"),
        "finance_stmt_31" => Some("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_fx_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?"),
        "finance_stmt_32" => Some("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?"),
        "finance_stmt_33" => Some("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?"),
        "finance_stmt_34" => Some("UPDATE finance_invoices SET outstanding_minor=outstanding_minor+?,base_outstanding_minor=base_outstanding_minor+?,status=CASE WHEN outstanding_minor+?=total_minor THEN 'posted' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_35" => Some("UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched'"),
        "finance_stmt_36" => Some("UPDATE finance_payments SET status='voided',version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_37" => Some("INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,fx_reversal_journal_id,reason,reversed_by,reversed_at) VALUES(?,?,?,?,?,?,?)"),
        "finance_stmt_38" => Some("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_39" => Some("UPDATE finance_payments SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'settled' ELSE status END,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_40" => Some("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=GREATEST(0,base_outstanding_minor-?),status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_41" => Some("UPDATE finance_accounts SET is_control=1,control_kind='bank',version=version+CASE WHEN control_kind='none' THEN 1 ELSE 0 END,updated_at=? WHERE id=? AND company_id=? AND control_kind='none'"),
        "finance_stmt_42" => Some("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_43" => Some("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)"),
        "finance_stmt_44" => Some("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?"),
        "finance_stmt_45" => Some("INSERT INTO finance_fx_revaluation_runs(id,company_id,chart_id,period_id,entry_date,base_currency,rates_json,source_hash,gain_account_id,loss_account_id,preparation_proof,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_46" => Some("INSERT INTO finance_fx_revaluation_lines(id,run_id,invoice_id,kind,currency,control_account_id,transaction_outstanding_minor,prior_base_minor,revalued_base_minor,adjustment_minor,rate_micros) VALUES(?,?,?,?,?,?,?,?,?,?,?)"),
        "finance_stmt_47" => Some("UPDATE finance_fx_revaluation_runs SET status='rejected',decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?"),
        "finance_stmt_48" => Some("UPDATE finance_invoices SET base_outstanding_minor=base_outstanding_minor+?,version=version+1,updated_at=? WHERE id=?"),
        "finance_stmt_49" => Some("UPDATE finance_fx_revaluation_runs SET status='posted',journal_id=?,decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?"),
        "ai_stmt_1" => Some("INSERT INTO finance_ai_events(id,company_id,proposal_id,action,actor_id,command_id,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?)"),
        "ai_stmt_2" => Some("UPDATE finance_ai_proposals SET claim_until=? WHERE id=? AND state='running' AND lease_token=? AND claim_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint"),
        "ai_stmt_3" => Some("UPDATE finance_ai_proposals SET state='running',lease_token=?,claim_until=?,attempts=attempts+1,updated_at=? WHERE id=? AND attempts<3 AND (state='queued' OR (state='running' AND claim_until<?)) RETURNING *"),
        "ai_stmt_4" => Some("INSERT INTO finance_ai_policies(company_id,enabled,version,kinds_json,timezone,daily_limit,monthly_limit,owner_actor_json,reason,changed_by,changed_at,next_scan_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,0) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,version=excluded.version,kinds_json=excluded.kinds_json,timezone=excluded.timezone,daily_limit=excluded.daily_limit,monthly_limit=excluded.monthly_limit,owner_actor_json=excluded.owner_actor_json,reason=excluded.reason,changed_by=excluded.changed_by,changed_at=excluded.changed_at,next_scan_at=0,last_error=NULL"),
        "ai_stmt_5" => Some("UPDATE finance_ai_proposals SET state='queued',version=version+1,error=NULL,attempts=0,lease_token=NULL,claim_until=NULL,actor_json=?,usage_id=?,updated_at=? WHERE id=? AND state IN ('failed','stale')"),
        "ai_stmt_6" => Some("INSERT INTO finance_ai_proposals(id,company_id,kind,source_id,source_version,source_hash,source_json,source_query_json,company_revision,policy_version,provider_json,actor_json,state,usage_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'queued',?,?,?)"),
        "ai_stmt_7" => Some("INSERT INTO finance_ai_usage(id,proposal_id,company_id,actor_id,month,business_date,state,lease_until,created_at) VALUES(?,?,?,?,?,?,'reserved',?,?)"),
        "ai_stmt_8" => Some("UPDATE finance_ai_proposals SET state='ready',result_json=?,error=NULL,lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE id=? AND lease_token=?"),
        "ai_stmt_9" => Some("UPDATE finance_ai_usage SET state='succeeded',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?"),
        "ai_stmt_10" => Some("UPDATE finance_ai_proposals SET state=?,error=?,lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE id=? AND state='running' AND lease_token=? RETURNING id"),
        "ai_stmt_11" => Some("UPDATE finance_ai_usage SET state='failed',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?"),
        "ai_stmt_12" => Some("UPDATE finance_ai_proposals SET state='failed',error='Preparation was interrupted. Retry or continue manually.',lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE state='running' AND claim_until<? AND attempts>=3"),
        "ai_stmt_13" => Some("UPDATE finance_ai_proposals SET state='dismissed',version=version+1,lease_token=NULL,claim_until=NULL,updated_at=? WHERE id=?"),
        "ai_stmt_14" => Some("UPDATE finance_ai_policies SET next_scan_at=? WHERE company_id=? AND enabled AND version=? AND next_scan_at<=? RETURNING company_id"),
        "ai_stmt_15" => Some("UPDATE finance_ai_policies SET last_error=NULL WHERE company_id=? AND version=?"),
        "ai_stmt_16" => Some("UPDATE finance_ai_policies SET last_error=?,next_scan_at=? WHERE company_id=? AND version=?"),
        "ai_stmt_17" => Some("UPDATE finance_ai_proposals SET state='applied',draft_command_id=?,version=version+1,updated_at=? WHERE id=?"),
        "ai_stmt_18" => Some("UPDATE finance_ai_usage SET state='failed',lease_until=0 WHERE state='reserved' AND proposal_id IN (SELECT id FROM finance_ai_proposals WHERE state IN ('failed','stale','dismissed'))"),
        "prepare_policy_edition" => Some("INSERT INTO finance_asset_policy_editions(id,company_id,asset_id,parent_id,content_json,source_hash,prepared_by,reason,created_at,updated_at,basis_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)"),
        "supersede_policy_edition" => Some("UPDATE finance_asset_policy_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?"),
        "decide_policy_edition" => Some("UPDATE finance_asset_policy_editions SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?"),
        "prepare_policy_run" => Some("INSERT INTO finance_asset_policy_runs(id,company_id,asset_id,policy_id,ordinal,entry_date,input_json,result_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "update_asset_carrying" => Some("UPDATE finance_assets SET accumulated_depreciation_minor=?,version=version+1,updated_at=? WHERE id=?"),
        "bump_policy_edition" => Some("UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?"),
        "asset_depreciation_event" => Some("INSERT INTO finance_asset_events(id,asset_id,company_id,event_type,asset_version,command_id,payload_json,actor_id,created_at) VALUES(?,?,?,'depreciated',?,?,?,?,?)"),
        "decide_policy_run" => Some("UPDATE finance_asset_policy_runs SET status=?,reviewed_by=?,journal_ids_json=?,version=version+1,updated_at=? WHERE id=?"),
        "reverse_asset_carrying" => Some("UPDATE finance_assets SET accumulated_depreciation_minor=accumulated_depreciation_minor-?,version=version+1,updated_at=? WHERE id=?"),
        "reverse_policy_version" => Some("UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?"),
        "reverse_policy_run" => Some("UPDATE finance_asset_policy_runs SET status='reversed',reversal_ids_json=?,reversal_entry_date=?,version=version+1,updated_at=? WHERE id=?"),
        "prepare_contract" => Some("INSERT INTO finance_business_contracts(id,company_id,root_id,parent_id,number,name,chart_id,party_id,start_date,end_date,edition,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "supersede_contract" => Some("UPDATE finance_business_contracts SET status='superseded',version=version+1,updated_at=? WHERE id=?"),
        "decide_contract" => Some("UPDATE finance_business_contracts SET status=?,reviewed_by=?,version=?,updated_at=? WHERE id=?"),
        "void_contract_invoice" => Some("UPDATE finance_invoices SET status='voided',outstanding_minor=0,base_outstanding_minor=0,version=version+1,updated_at=? WHERE id=? AND company_id=?"),
        "prepare_bill" => Some("INSERT INTO finance_business_billing_runs(id,company_id,contract_id,root_id,contract_version,billing_key,service_from,service_to,input_json,result_json,source_hash,invoice_id,invoice_version,created_by,created_at,no_charge_status,parent_run_id,parent_version,bill_edition,contract_content_json,revision_reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "prepare_recognition" => Some("INSERT INTO finance_recognition_runs(id,company_id,billing_run_id,item_key,due_date,entry_date,period_id,amount_minor,account_id,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "decide_recognition" => Some("UPDATE finance_recognition_runs SET status=?,version=?,reviewed_by=?,journal_id=?,reason=?,updated_at=? WHERE id=?"),
        "reverse_recognition" => Some("UPDATE finance_recognition_runs SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=?"),
        "approve_no_charge" => Some("UPDATE finance_business_billing_runs SET no_charge_status=?,reviewed_by=?,review_reason=?,version=version+1 WHERE id=?"),
        "money_snapshot_insert" => Some("INSERT INTO money_transaction_snapshots
      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"),
        "money_snapshot_batch_insert" => Some("INSERT INTO money_transaction_snapshots
            (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
            VALUES ${chunk.map(() => '(?,?,?,?,?,?,?,?,?,?,?,?)').join(',')}"),
        "money_snapshot_insert2" => Some("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind,carried_allocation_offset_units)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "money_snapshot_insert3" => Some("INSERT INTO money_transaction_snapshots
      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,
       original_source_type,original_source_id,original_amount_kind)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "currency_preference_insert" => Some("INSERT INTO user_currency_preferences(company_id,user_id,display_currency,updated_at)
      VALUES(?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET display_currency=excluded.display_currency,version=user_currency_preferences.version+1,updated_at=excluded.updated_at"),
        "fx_rate_insert" => Some("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)"),
        "currency_scale_insert" => Some("INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id,currency) DO NOTHING"),
        "elimination_insert" => Some("INSERT INTO finance_consolidation_eliminations(id,run_id,description,debit_account_id,credit_account_id,amount_minor,status,journal_id,version,source_transaction_id,created_by,created_at,updated_at,evidence_json)
      VALUES(?,?,?,?,?,?, 'proposed',NULL,1,?,?,?,?,?)"),
        "elimination_event_prepare" => Some("INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"),
        "elimination_approve" => Some("UPDATE finance_consolidation_eliminations SET status='approved',approved_by=?,approved_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='proposed'"),
        "journal_insert" => Some("INSERT INTO finance_consolidation_journals(id,run_id,holding_id,journal_number,entry_date,currency,memo,status,source_hash,version,created_by,posted_by,posted_at,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,'posted',?,1,?,?,?,?,?) ON CONFLICT(run_id) DO NOTHING"),
        "journal_lines_debit" => Some("INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,?,0,?)"),
        "journal_lines_credit" => Some("INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,0,?,?)"),
        "journal_post" => Some("UPDATE finance_consolidation_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=? AND run_id=?"),
        "elimination_post" => Some("UPDATE finance_consolidation_eliminations SET status='posted',consolidation_journal_id=?,posted_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='approved'"),
        "elimination_event_decide" => Some("INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"),
        "run_insert" => Some("INSERT INTO finance_consolidation_runs(id,holding_id,presentation_currency,as_of_date,status,source_snapshot_json,source_hash,version,created_by,created_at,updated_at) VALUES(?,?,?,?, 'draft',?,?,1,?,?,?)"),
        "run_event_prepare" => Some("INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)"),
        "run_event_decide" => Some("INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)"),
        "ownership_interest_insert" => Some("INSERT INTO finance_ownership_interests(id,holding_id,company_id,parent_id,investor_entity_id,investee_entity_id,accounting_method,ownership_bps,voting_bps,control_assessment,effective_from,effective_to,created_by,created_at,updated_at,source_hash,source_json,reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "ownership_interest_retire" => Some("UPDATE finance_ownership_interests SET status='retired',effective_to=?,version=version+1,updated_at=? WHERE id=? AND version=?"),
        "ownership_interest_decide" => Some("UPDATE finance_ownership_interests SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?"),
        "group_calc_run_insert" => Some("INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_version,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,prepared_by,created_at,updated_at) VALUES(?,?,?,'consolidation',1,?,2,?,?,?,?,'prepared',?,?,?)"),
        "group_calc_event_prepare" => Some("INSERT INTO finance_calculation_events VALUES(?,?,1,'prepare',?,?,?,?)"),
        "acquisition_insert" => Some("INSERT INTO finance_acquisitions(id,holding_id,company_id,ownership_id,investee_entity_id,acquisition_date,currency,consideration_minor,previous_interest_minor,nci_measurement,nci_minor,identifiable_net_assets_minor,deferred_tax_minor,goodwill_minor,bargain_purchase_minor,cgu_reference,calculation_run_id,created_by,created_at,updated_at,source_hash,source_json,reason,parent_id,parent_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "acquisition_supersede" => Some("UPDATE finance_acquisitions SET status='superseded',version=version+1,updated_at=? WHERE id=?"),
        "acquisition_decide" => Some("UPDATE finance_acquisitions SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?"),
        "group_calc_run_decide" => Some("UPDATE finance_calculation_runs SET status=?,reviewed_by=?,approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='prepared'"),
        "group_calc_event_decide" => Some("INSERT INTO finance_calculation_events VALUES(?,?,2,?,?,?,?,?)"),
        "commerce_invoice_outstanding" => Some("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=base_outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?"),
        "commerce_refund_obligation_insert" => Some("INSERT INTO finance_refund_obligations(id,company_id,branch_id,department_id,return_id,invoice_id,party_id,direction,currency,chart_id,balance_account_id,amount_minor,outstanding_minor,base_amount_minor,base_outstanding_minor,source_credit_journal_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "commerce_journal_insert" => Some("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,1,?,?,?,?,?)"),
        "commerce_journal_lines_insert" => Some("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,?,?,?,?,?)"),
        "commerce_invoice_insert" => Some("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,source_version,status,created_by,created_at,updated_at) VALUES(?,?,?,?,CASE WHEN ?='receivable' THEN erpx_sales_document_number(?,'INV',?) ELSE ? END,?,?,?,?,?,?,?,?,?,?,'commerce_fulfillment',?,?,'draft',?,?,?)"),
        "commerce_payment_insert" => Some("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,base_amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at,payment_slip_id) VALUES(?,?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?,?)"),
        "commerce_allocation_insert" => Some("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)"),
        "commerce_invoice_adjust" => Some("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=GREATEST(0,base_outstanding_minor-?),status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?"),
        "commerce_party_update" => Some("UPDATE finance_parties SET code=?,name=?,payment_terms_days=?,credit_limit_minor=?,status=?,notification_email=?,notification_consent=?,version=version+1,updated_at=? WHERE id=?"),
        "commerce_party_insert" => Some("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,notification_email,notification_consent,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)"),
        "pos_journal_insert" => Some("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'pos',?,?,'posted',1,?,?,?,?,?)"),
        "pos_journal_lines_insert" => Some("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?)"),
        "pos_payment_insert" => Some("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'receipt',?,?,?,?,0,?,'settled',?,?,?,?,?)"),
        "pos_party_insert" => Some("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer','POS-WALKIN','POS walk-in customer',?,'active',?,?) ON CONFLICT(id) DO NOTHING"),
        "pos_invoice_insert" => Some("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,?,?,?,?,0,?,?,?,'pos',?,'paid',?,?,?,?,?,?)"),
        "pos_payment_party_update" => Some("UPDATE finance_payments SET party_id=? WHERE id=?"),
        "pos_allocation_insert" => Some("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)"),
        "pos_account_insert" => Some("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)"),
        "pos_bank_match" => Some("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?"),
        "pos_bank_match_payout" => Some("UPDATE finance_bank_transactions SET match_status='matched',matched_type='pos_payout',matched_id=?,version=version+1 WHERE id=?"),
        "pos_refund_payment_insert" => Some("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'refund',?,?,?,?,0,?,'settled',?,?,?,?,?)"),
        "expense_employee_party" => Some("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,person_id,created_at,updated_at) VALUES(?,?,'employee',?,?,?,0,?,?,?) ON CONFLICT(company_id,person_id) WHERE person_id IS NOT NULL DO UPDATE SET name=excluded.name"),
        "expense_payable_invoice" => Some("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'payable',?,?,?,?,?,?,?,?,?,?,?,'expense_claim',?,'posted',?,?,?,?,?,?)"),
        "expense_reversal_journal" => Some("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'expense_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?"),
        "expense_reversal_lines" => Some("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?"),
        "expense_journal_reversed" => Some("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?"),
        "expense_invoice_voided" => Some("UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?"),
        "consolidation_run_update_review" => Some("UPDATE finance_consolidation_runs SET status=?,reviewed_by=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=? AND holding_id=? AND status=? AND version=?"),
        "consolidation_run_update_approve" => Some("UPDATE finance_consolidation_runs SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=? AND holding_id=? AND status=? AND version=?"),
        "consolidation_run_update_publish" => Some("UPDATE finance_consolidation_runs SET status=?,published_by=?,published_at=?,version=version+1,updated_at=? WHERE id=? AND holding_id=? AND status=? AND version=?"),
        _ => None,
    }
}

pub struct SqlStatementPort;

impl OwnerStatementPort for SqlStatementPort {
    fn sql(&self, kind: &str, value_count: usize) -> Option<String> {
        statement_sql(kind, value_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_kind_has_canonical_sql() {
        for kind in crate::domain::finance_owner_write::SUPPORTED_KINDS {
            if *kind == MONEY_SNAPSHOT_BATCH_KIND {
                continue;
            }
            let sql = statement_sql(kind, 1).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(!sql.is_empty());
            assert!(!sql.contains(';'), "statements must be singular: {kind}");
        }
    }

    #[test]
    fn attachment_version_step_is_arity_fenced_from_owner_fragments() {
        let kind = format!("{}finance_journal", ATTACHMENT_VERSION_STEP_PREFIX);
        assert_eq!(
            statement_sql(&kind, 3).unwrap(),
            "UPDATE finance_journals SET version=version+1,updated_at=? WHERE id=? AND version=?"
        );
        assert!(statement_sql(&kind, 2).is_none());
        let no_updated = format!("{}finance_business_bill", ATTACHMENT_VERSION_STEP_PREFIX);
        assert_eq!(
            statement_sql(&no_updated, 2).unwrap(),
            "UPDATE finance_business_billing_runs SET version=version+1 WHERE id=? AND version=? \
             AND no_charge_status='draft' AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=finance_business_billing_runs.id)"
        );
        assert!(statement_sql(&no_updated, 3).is_none());
        assert!(statement_sql(
            &format!("{}finance_unknown", ATTACHMENT_VERSION_STEP_PREFIX),
            3
        )
        .is_none());
    }

    #[test]
    fn money_batch_is_built_from_a_trusted_row_template() {
        let sql = statement_sql(MONEY_SNAPSHOT_BATCH_KIND, 24).unwrap();
        assert_eq!(sql.matches(MONEY_SNAPSHOT_BATCH_ROW).count(), 2);
        assert!(statement_sql(MONEY_SNAPSHOT_BATCH_KIND, 13).is_none());
        assert!(statement_sql(MONEY_SNAPSHOT_BATCH_KIND, 12 * 501).is_none());
    }
}
