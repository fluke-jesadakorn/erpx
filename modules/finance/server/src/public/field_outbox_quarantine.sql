(((e.event_type='finance.party.create' AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_serviceCustomerPreparation'))) OR (e.event_type='finance.invoice.post' AND EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=e.aggregate_id AND i.source_type='field_service'))) OR ((e.aggregate_type='finance_payment' AND e.event_type IN('finance.payment.create','finance.payment.approve','finance.payment.release','finance.payment.confirm') AND erpx_finance_field_payment_owned(e.aggregate_id)) OR (e.aggregate_type='finance_bank' AND e.event_type='finance.bank.import' AND erpx_finance_field_statement_owned(e.aggregate_id)))) OR (e.aggregate_type='flow_assign' AND e.event_type='flow.assign' AND erpx_finance_field_invoice_owned(e.aggregate_id,NULL))
 OR (EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_servicePartsPrerequisite'))) OR (e.event_type='finance.service-no-charge.confirm' AND erpx_finance_field_invoice_owned(e.aggregate_id,NULL)) OR (EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND r.operation='finance.service-no-charge.confirm' AND erpx_finance_field_invoice_owned(r.record_id,r.authority_json::jsonb->>'companyId'))) OR (
 (e.event_type='finance.journal.post' AND erpx_finance_field_journal_owned(e.aggregate_id,NULL))
 OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND r.operation='finance.journal.post'
  AND erpx_finance_field_journal_owned(r.record_id,r.authority_json::jsonb->>'companyId'))
 OR (
  ((e.event_type='flow.assign' AND e.aggregate_type='flow_assign' AND erpx_finance_field_journal_owned(e.aggregate_id,NULL))
   OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND r.operation='flow.assign'
    AND erpx_finance_field_journal_owned(r.record_id,r.authority_json::jsonb->>'companyId')))
  AND NOT (
   (SELECT count(*) FROM flow_events all_events WHERE all_events.command_id=e.command_id)=1
   AND EXISTS(SELECT 1 FROM flow_events f JOIN command_receipts r ON r.command_id=f.command_id
    WHERE f.command_id=e.command_id AND f.kind IN('expense','leave','staff_task','request','stock_count','transfer','order','invoice','payroll','record')
     AND f.action='assign' AND f.record_id=e.aggregate_id AND f.record_version=e.aggregate_version
     AND f.company_id=e.payload_json::jsonb->>'companyId' AND f.actor_user_id=r.actor_id
     AND r.operation='flow.assign' AND r.record_id=f.record_id AND r.authority_json::jsonb->>'companyId'=f.company_id
     AND e.payload_json::jsonb->>'id'=e.aggregate_id AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version))
  )
 )
)
