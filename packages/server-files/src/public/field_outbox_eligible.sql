((e.event_type='attachment.set.create' AND e.aggregate_type='attachment_set')
 OR (e.event_type='attachment.record.bind' AND e.aggregate_type='attachment_record'))
AND EXISTS(SELECT 1 FROM attachment_sets s JOIN command_receipts r ON r.command_id=e.command_id
 WHERE ((s.target_type='field_service_visit' AND s.component_kind='record'
 AND s.purpose_key IN ('site_photo','technician_report','signed_job_sheet','exception_evidence'))
 OR (s.target_type='finance_payment' AND erpx_finance_field_payment_owned(s.target_id)
 AND s.component_kind='action' AND s.component_ref='bank-confirmation'
 AND s.purpose_key='payment_proof' AND s.classification='confidential')
 OR (s.target_type='finance_journal' AND erpx_finance_field_journal_owned(s.target_id,s.company_id)
 AND s.component_kind='record' AND s.component_ref IS NULL AND s.purpose_key='journal_support'
 AND s.classification='confidential' AND s.max_files=50 AND s.required_count=0 AND s.policy_version=1))
 AND s.target_id=e.aggregate_id AND s.company_id=e.payload_json::jsonb->>'companyId'
 AND r.result_json::jsonb->>'setId'=s.id
 AND ((e.event_type='attachment.set.create'
  AND r.result_json::jsonb->'expectedTargetVersion'=to_jsonb(e.aggregate_version)
  AND r.result_json::jsonb->'policyVersion'=to_jsonb(s.policy_version)
  AND EXISTS(SELECT 1 FROM attachment_events origin WHERE origin.set_id=s.id AND origin.company_id=s.company_id
   AND origin.actor_id=r.actor_id AND origin.created_at=s.created_at
   AND origin.created_at>=r.created_at AND origin.action='set.native-origin'
   AND origin.details_json::jsonb->>'commandId'=e.command_id AND origin.details_json::jsonb->>'outboxId'=e.id
   AND origin.details_json::jsonb->>'receiptHash'=encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex')
   AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=e.command_id AND a.id=origin.details_json::jsonb->>'auditId')))
 OR (e.event_type='attachment.record.bind' AND s.state='bound' AND s.bound_command_id=e.command_id
  AND s.bound_at>=r.created_at AND s.expected_target_version+1=e.aggregate_version
  AND r.result_json::jsonb->'version'=to_jsonb(e.aggregate_version)
  AND jsonb_typeof(r.result_json::jsonb->'manifest')='array' AND jsonb_array_length(r.result_json::jsonb->'manifest')>0
  AND EXISTS(SELECT 1 FROM attachment_events bound WHERE bound.set_id=s.id AND bound.company_id=s.company_id
   AND bound.actor_id=r.actor_id AND bound.created_at=s.bound_at AND bound.action='set.bound'
   AND bound.details_json::jsonb->>'bindingType'='record_document' AND bound.details_json::jsonb->>'bindingId'=s.target_id
   AND bound.details_json::jsonb->'manifest'=r.result_json::jsonb->'manifest'))))
