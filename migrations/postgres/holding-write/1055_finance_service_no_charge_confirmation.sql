-- A covered Service visit can fulfill its contractual duty without a monetary
-- demand. Keep that outcome distinct from payment, posting and cancellation.
ALTER TABLE finance_invoices DROP CONSTRAINT finance_invoices_status_check;
ALTER TABLE finance_invoices ADD CONSTRAINT finance_invoices_status_check
 CHECK(status IN ('draft','in_review','posted','partially_paid','paid','voided','no_charge_confirmed'));

CREATE TABLE finance_service_no_charge_confirmations (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL,
 invoice_id TEXT NOT NULL UNIQUE REFERENCES finance_invoices(id),
 service_order_id TEXT NOT NULL REFERENCES field_service_orders(id),
 billing_release_id TEXT NOT NULL UNIQUE REFERENCES field_service_billing_releases(id),
 invoice_version_before BIGINT NOT NULL CHECK(invoice_version_before>0),
 invoice_version_after BIGINT NOT NULL CHECK(invoice_version_after=invoice_version_before+1),
 source_hash TEXT NOT NULL CHECK(source_hash ~ '^[a-f0-9]{64}$'),
 status TEXT NOT NULL CHECK(status='no_charge_confirmed'),
 independence_checked BOOLEAN NOT NULL CHECK(independence_checked),
 confirmed_by TEXT NOT NULL,
 reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 10 AND 3000),
 command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id),
 confirmed_at BIGINT NOT NULL
);

-- This view describes only the original fully covered warranty handoff. A bare
-- zero, source label, later FX quote or an unrelated Business bill is no proof.
CREATE VIEW finance_service_no_charge_candidates AS
 WITH source AS (
 SELECT invoice.id invoice_id,invoice.company_id,invoice.version invoice_version,
 invoice.status invoice_status,invoice.created_by,invoice.approved_by,release.released_by,service.branch_id,service.department_id,
 service.id service_order_id,release.id billing_release_id,
 service.technician_id,completion.actor_id completed_by,
 jsonb_build_object(
  'invoice',to_jsonb(invoice)-ARRAY['status','version','approved_by','updated_at'],
  'release',to_jsonb(release),'event',to_jsonb(event),'completion',to_jsonb(completion),'completionReceipt',jsonb_build_object('id',completion_receipt.command_id,'actor',completion_receipt.actor_id,'hash',completion_receipt.request_hash,'result',completion_receipt.result_json),
  'command',jsonb_build_object('id',receipt.command_id,'actor',receipt.actor_id,'hash',receipt.request_hash,'result',receipt.result_json),
  'order',jsonb_build_object('id',service.id,'company',service.company_id,'branch',service.branch_id,'department',service.department_id,'agreement',service.agreement_id,'technician',service.technician_id,'completedBy',completion.actor_id,'completedAt',service.visit_completed_at),
  'agreementMoney',to_jsonb(agreement_money),'releaseMoney',to_jsonb(release_money),'invoiceMoney',to_jsonb(invoice_money)
 )::text source_image
 FROM field_service_billing_releases release
 JOIN field_service_orders service ON service.id=release.order_id AND service.agreement_id=release.agreement_id
  AND service.status='billed' AND service.version>=release.source_order_version
 JOIN LATERAL (SELECT completed.* FROM field_service_order_events completed WHERE completed.order_id=service.id AND completed.action='complete' AND completed.status='completed' AND completed.version<release.source_order_version ORDER BY completed.version DESC LIMIT 1) completion ON completion.created_at=service.visit_completed_at
  AND completion.technician_id=service.technician_id
  AND completion.details_json->>'evidence'=service.completion_evidence
  AND completion.details_json->>'customerConfirmation'=service.customer_confirmation
  AND completion.details_json->>'laborMinutes'=service.labor_minutes::text
 JOIN command_receipts completion_receipt ON completion.id='service-order-event-'||completion_receipt.command_id
  AND completion_receipt.operation='field.order.complete' AND completion_receipt.record_id=service.id
  AND completion_receipt.actor_id=completion.actor_id
  AND completion_receipt.authority_json::jsonb->>'companyId'=service.company_id
  AND completion_receipt.result_json::jsonb->>'id'=service.id
  AND completion_receipt.result_json::jsonb->>'status'='completed'
  AND completion_receipt.result_json::jsonb->>'version'=completion.version::text
 JOIN field_service_agreements agreement ON agreement.id=service.agreement_id AND agreement.company_id=service.company_id
  AND agreement.branch_id=service.branch_id AND agreement.department_id IS NOT DISTINCT FROM service.department_id
  AND agreement.customer_id=release.customer_id
 JOIN field_service_order_events event ON event.order_id=service.id AND event.version=release.source_order_version
  AND event.action='release-billing' AND event.status='billed' AND event.actor_id=release.released_by
 JOIN command_receipts receipt ON release.id='field-billing-'||receipt.command_id
  AND event.id='service-order-event-'||receipt.command_id AND receipt.operation='field.order.release-billing'
  AND receipt.record_id=service.id AND receipt.actor_id=release.released_by
  AND receipt.authority_json::jsonb->>'companyId'=service.company_id
  AND receipt.result_json::jsonb->>'id'=service.id AND receipt.result_json::jsonb->>'status'='billed'
  AND receipt.result_json::jsonb->>'version'=release.source_order_version::text
 JOIN finance_invoices invoice ON invoice.id='field-invoice-'||receipt.command_id AND invoice.company_id=service.company_id
  AND invoice.kind='receivable' AND invoice.source_type='field_service' AND invoice.source_id=service.id
  AND invoice.party_id=release.customer_id AND invoice.currency=release.currency AND invoice.invoice_date=release.pricing_date
 JOIN money_transaction_snapshots agreement_money ON agreement_money.company_id=service.company_id AND agreement_money.source_type='field_service_agreement' AND agreement_money.source_id=agreement.id AND agreement_money.amount_kind='visit_rate' AND agreement_money.currency=release.currency AND agreement_money.amount_units=agreement.visit_rate_minor AND agreement_money.amount_units>0
 JOIN money_transaction_snapshots release_money ON release_money.company_id=service.company_id
  AND release_money.source_type='field_service_billing_release' AND release_money.source_id=release.id
  AND release_money.amount_kind='invoice_total' AND release_money.currency=release.currency
  AND release_money.transaction_date::text=release.pricing_date AND release_money.amount_units=0 AND release_money.usd_units=0
 JOIN money_transaction_snapshots invoice_money ON invoice_money.company_id=invoice.company_id
  AND invoice_money.source_type='finance_invoice' AND invoice_money.source_id=invoice.id AND invoice_money.amount_kind='draft:v1'
  AND invoice_money.currency=release_money.currency AND invoice_money.currency_scale=release_money.currency_scale
  AND invoice_money.transaction_date=release_money.transaction_date AND invoice_money.amount_units=0 AND invoice_money.usd_units=0
  AND invoice_money.fx_rate_id IS NOT DISTINCT FROM release_money.fx_rate_id
  AND invoice_money.used_latest_fallback IS NOT DISTINCT FROM release_money.used_latest_fallback
  AND invoice_money.captured_at=release_money.captured_at
  -- The original zero snapshot writer deliberately has no carrying pointer.
  -- Preserve it; exact release/command/invoice and copied quote prove this0.
  AND invoice_money.carried_source_type IS NULL AND invoice_money.carried_source_id IS NULL
  AND invoice_money.carried_amount_kind IS NULL
 WHERE release.status='released' AND release.warranty_applied AND release.warranty_discount_bps=10000
  AND release.warranty_end_snapshot>=release.pricing_date AND release.amount_minor=0
  AND invoice.status IN ('draft','no_charge_confirmed') AND invoice.subtotal_minor=0 AND invoice.tax_minor=0
  AND invoice.total_minor=0 AND invoice.outstanding_minor=0 AND invoice.base_total_minor=0 AND invoice.base_outstanding_minor=0
  AND invoice.line_revision=1 AND invoice.journal_id IS NULL AND invoice.posted_at IS NULL
  AND NOT EXISTS(SELECT 1 FROM finance_journals journal WHERE journal.company_id=invoice.company_id AND journal.source_type='invoice' AND journal.source_id=invoice.id)
  AND NOT EXISTS(SELECT 1 FROM finance_allocations allocation WHERE allocation.invoice_id=invoice.id)
  AND NOT EXISTS(SELECT 1 FROM command_receipts payment WHERE payment.operation='finance.payment.create' AND payment.authority_json::jsonb->>'companyId'=invoice.company_id AND payment.result_json::jsonb->>'settlementInvoiceId'=invoice.id)
  AND NOT EXISTS(SELECT 1 FROM finance_refund_obligations refund WHERE refund.invoice_id=invoice.id)
  AND NOT EXISTS(SELECT 1 FROM finance_billing_corrections correction WHERE correction.invoice_id=invoice.id)
  AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs business WHERE business.invoice_id=invoice.id)
 ) SELECT source.*,encode(sha256(convert_to(source.source_image,'UTF8')),'hex') source_hash FROM source;

CREATE VIEW finance_service_no_charge_completed AS
 SELECT source.invoice_id id,source.company_id,confirmation.id confirmation_id,confirmation.command_id,
 confirmation.confirmed_by,confirmation.confirmed_at,confirmation.reason,confirmation.invoice_version_after version
 FROM finance_service_no_charge_candidates source
 JOIN finance_service_no_charge_confirmations confirmation ON confirmation.invoice_id=source.invoice_id
  AND confirmation.company_id=source.company_id AND confirmation.service_order_id=source.service_order_id
  AND confirmation.billing_release_id=source.billing_release_id AND confirmation.source_hash=source.source_hash
  AND confirmation.invoice_version_after=source.invoice_version AND confirmation.status='no_charge_confirmed'
 JOIN command_receipts receipt ON receipt.command_id=confirmation.command_id
  AND receipt.operation='finance.service-no-charge.confirm' AND receipt.record_id=source.invoice_id
  AND receipt.actor_id=confirmation.confirmed_by AND receipt.authority_json::jsonb->>'companyId'=source.company_id
  AND receipt.result_json::jsonb->>'id'=source.invoice_id
  AND receipt.result_json::jsonb->>'status'='no_charge_confirmed'
  AND receipt.result_json::jsonb->>'confirmationId'=confirmation.id
  AND receipt.result_json::jsonb->>'version'=confirmation.invoice_version_after::text
 WHERE source.invoice_status='no_charge_confirmed' AND source.approved_by=confirmation.confirmed_by
  AND confirmation.independence_checked;
-- Independence is established under current IAM at the original decision and
-- retained immutably. Later lawful identity linking must not rewrite history.

CREATE FUNCTION guard_service_no_charge_confirmation() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE invoice TEXT;
BEGIN
 invoice:=CASE WHEN TG_TABLE_NAME='finance_invoices' THEN COALESCE(to_jsonb(NEW)->>'id',to_jsonb(OLD)->>'id') ELSE COALESCE(to_jsonb(NEW)->>'invoice_id',to_jsonb(OLD)->>'invoice_id') END;
 IF EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=invoice AND i.status='no_charge_confirmed')
  AND NOT EXISTS(SELECT 1 FROM finance_service_no_charge_completed complete WHERE complete.id=invoice) THEN
  RAISE EXCEPTION 'SERVICE_NO_CHARGE_PROOF_REQUIRED' USING ERRCODE='23514';
 END IF;
 IF TG_TABLE_NAME='finance_service_no_charge_confirmations' AND NOT EXISTS(SELECT 1 FROM finance_service_no_charge_candidates source WHERE source.invoice_id=invoice AND NOT iam_same_person(to_jsonb(NEW)->>'confirmed_by',source.created_by,source.company_id) AND NOT iam_same_person(to_jsonb(NEW)->>'confirmed_by',source.released_by,source.company_id)) THEN
  RAISE EXCEPTION 'SERVICE_NO_CHARGE_INDEPENDENT_REVIEW_REQUIRED' USING ERRCODE='23514';
 END IF;
 IF TG_TABLE_NAME='finance_service_no_charge_confirmations'
  AND NOT EXISTS(SELECT 1 FROM finance_service_no_charge_completed complete WHERE complete.id=invoice) THEN
  RAISE EXCEPTION 'SERVICE_NO_CHARGE_PROOF_REQUIRED' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER finance_service_no_charge_invoice_proof AFTER INSERT OR UPDATE ON finance_invoices
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_service_no_charge_confirmation();
CREATE CONSTRAINT TRIGGER finance_service_no_charge_decision_proof AFTER INSERT ON finance_service_no_charge_confirmations
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_service_no_charge_confirmation();
CREATE FUNCTION protect_service_no_charge_confirmation() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='finance_service_no_charge_confirmations' OR OLD.status='no_charge_confirmed' THEN
  RAISE EXCEPTION 'SERVICE_NO_CHARGE_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER finance_service_no_charge_decision_immutable BEFORE UPDATE OR DELETE ON finance_service_no_charge_confirmations
 FOR EACH ROW EXECUTE FUNCTION protect_service_no_charge_confirmation();
CREATE TRIGGER finance_service_no_charge_invoice_immutable BEFORE UPDATE OR DELETE ON finance_invoices
 FOR EACH ROW WHEN(OLD.status='no_charge_confirmed') EXECUTE FUNCTION protect_service_no_charge_confirmation();

CREATE FUNCTION guard_service_no_charge_transition() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.status='no_charge_confirmed' THEN
  IF TG_OP='INSERT' OR OLD.status<>'draft' OR NEW.version<>OLD.version+1
   OR (to_jsonb(NEW)-ARRAY['status','version','approved_by','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','approved_by','updated_at']) THEN
   RAISE EXCEPTION 'SERVICE_NO_CHARGE_TRANSITION_INVALID' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER finance_service_no_charge_transition BEFORE INSERT OR UPDATE ON finance_invoices
 FOR EACH ROW EXECUTE FUNCTION guard_service_no_charge_transition();

-- A later contradictory update to the retained completed visit cannot silently
-- erase its confirmed contractual history. Original unrelated Service records
-- keep their existing mutation contract.
CREATE FUNCTION guard_service_no_charge_retained_order() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM finance_service_no_charge_confirmations decision WHERE decision.service_order_id=NEW.id
   AND NOT EXISTS(SELECT 1 FROM finance_service_no_charge_completed complete WHERE complete.confirmation_id=decision.id)) THEN
  RAISE EXCEPTION 'SERVICE_NO_CHARGE_PROOF_REQUIRED' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER finance_service_no_charge_retained_order AFTER UPDATE ON field_service_orders
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_service_no_charge_retained_order();
