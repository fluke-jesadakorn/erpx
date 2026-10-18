-- A future private closure cannot adopt unsupported pre-install rows.
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM command_receipts WHERE operation='people.payroll.final-pay.cancel')
  OR EXISTS(SELECT 1 FROM hr_payroll_final_pay_events WHERE action='cancelled')
  OR EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations WHERE status='cancelled')
  OR EXISTS(SELECT 1 FROM hr_payroll_runs WHERE mode='final_pay' AND status='cancelled') THEN
  RAISE EXCEPTION 'PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED' USING ERRCODE='23514';
 END IF;
END $$;
-- Only close a stranded, unreviewed preparation after its actual HR case and
-- Payroll clearance task were normally cancelled. No active-case withdrawal.
-- Original 0260/0933 files, ordinary cancellation policy and UNIQUE task remain.
CREATE FUNCTION hr_final_pay_cancellation_candidate(original_run hr_payroll_runs) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations p JOIN hr_payroll_runs r ON r.id=p.run_id AND r.company_id=p.company_id AND r.scope_engagement_id=p.engagement_id
 JOIN people_preparation_tasks t ON t.id=p.task_id AND t.company_id=p.company_id AND t.case_id=p.case_id AND t.clearance_type='payroll'
 JOIN people_lifecycle_cases c ON c.id=p.case_id AND c.company_id=p.company_id AND c.engagement_id=p.engagement_id
 JOIN hr_engagements e ON e.id=p.engagement_id AND e.company_id=p.company_id AND e.person_id=c.person_id
 JOIN hr_payroll_final_pay_events event ON event.preparation_id=p.id AND event.company_id=p.company_id AND event.run_id=r.id AND event.task_id=t.id AND event.action='cancelled'
 JOIN command_receipts receipt ON event.id='final-pay-cancel-event-'||receipt.command_id
 WHERE r.id=(original_run).id AND p.status='prepared' AND r.status IN ('draft','calculating','failed','preview')
 AND event.evidence_json::jsonb->'run'=to_jsonb(original_run) AND to_jsonb(r)=to_jsonb(original_run)
 AND event.evidence_json::jsonb->'preparation'=to_jsonb(p) AND event.evidence_json::jsonb->'task'=to_jsonb(t) AND event.evidence_json::jsonb->'case'=to_jsonb(c)
 AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT 1::bigint UNION SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version AND binding.company_id=p.company_id AND binding.target_type='payroll_final_pay' AND binding.target_id=p.id WHERE binding.next_version<=p.version) SELECT 1 FROM versions WHERE version=p.version)
 AND receipt.operation='people.payroll.final-pay.cancel' AND receipt.record_id=p.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=p.company_id
 AND receipt.result_json::jsonb->>'id'=p.id AND receipt.result_json::jsonb->>'runId'=r.id AND receipt.result_json::jsonb->>'taskId'=t.id AND receipt.result_json::jsonb->>'caseId'=c.id
 AND receipt.result_json::jsonb->>'status'='cancelled' AND receipt.result_json::jsonb->>'cancellationId'=event.id
 AND (receipt.result_json::jsonb->>'version')::bigint=(event.evidence_json::jsonb->'preparation'->>'version')::bigint+1
 AND (receipt.result_json::jsonb->>'runVersion')::bigint=(event.evidence_json::jsonb->'run'->>'version')::bigint+1
 AND (receipt.result_json::jsonb->>'taskVersion')::bigint=(event.evidence_json::jsonb->'task'->>'version')::bigint
 AND (receipt.result_json::jsonb->>'caseVersion')::bigint=(event.evidence_json::jsonb->'case'->>'version')::bigint
 AND event.task_version=(event.evidence_json::jsonb->'task'->>'version')::bigint
 AND length(btrim(event.evidence_json::jsonb->>'reason')) BETWEEN 8 AND 1000
 AND r.mode='final_pay' AND r.submitted_by IS NULL AND r.approved_by IS NULL AND r.posted_by IS NULL
 AND c.status='cancelled' AND t.status='cancelled' AND (iam_same_person(p.prepared_by,event.actor_id,p.company_id) OR t.assignee_id=event.actor_id)
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_postings x WHERE x.run_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_issued_payslips x WHERE x.run_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_payment_batches x WHERE x.company_id=r.company_id AND x.source_type='payroll' AND x.source_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM finance_journals x WHERE x.company_id=r.company_id AND x.source_type='payroll' AND x.source_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions x WHERE x.company_id=r.company_id AND x.run_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_items x WHERE x.run_id=r.id AND x.status IN ('approved','posted'))
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_corrections x WHERE x.run_id=r.id AND x.company_id=r.company_id) AND EXISTS(SELECT 1 FROM command_receipts original JOIN hr_payroll_final_pay_events born ON born.id='final-pay-event-'||original.command_id
 WHERE original.operation='people.payroll.final-pay.prepare' AND original.record_id=p.id AND original.actor_id=p.prepared_by AND original.authority_json::jsonb->>'companyId'=p.company_id
 AND original.result_json::jsonb->>'id'=p.id AND original.result_json::jsonb->>'taskId'=p.task_id AND original.result_json::jsonb->>'runId'=p.run_id AND original.result_json::jsonb->>'status'='prepared' AND original.result_json::jsonb->>'version'='1'
 AND born.company_id=p.company_id AND born.preparation_id=p.id AND born.run_id=p.run_id AND born.task_id=p.task_id AND born.actor_id=p.prepared_by AND born.action='prepared') AND EXISTS(SELECT 1 FROM command_receipts parent JOIN audit_events a ON a.command_id=parent.command_id AND a.action=parent.operation AND a.record_id=c.id
 JOIN outbox_events o ON o.command_id=parent.command_id AND o.event_type=parent.operation AND o.aggregate_id=c.id AND o.aggregate_version=a.record_version
 WHERE parent.operation='people.lifecycle.cancel' AND parent.record_id=c.id AND parent.authority_json::jsonb->>'companyId'=c.company_id
 AND parent.result_json::jsonb->>'id'=c.id AND parent.result_json::jsonb->>'status'='cancelled' AND a.record_version<=c.version))
$$;
CREATE FUNCTION hr_final_pay_cancellation_valid(preparation_key text,event_key text DEFAULT NULL) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations p JOIN hr_payroll_runs r ON r.id=p.run_id AND r.company_id=p.company_id AND r.scope_engagement_id=p.engagement_id
 JOIN people_preparation_tasks t ON t.id=p.task_id AND t.company_id=p.company_id AND t.case_id=p.case_id AND t.clearance_type='payroll'
 JOIN people_lifecycle_cases c ON c.id=p.case_id AND c.company_id=p.company_id AND c.engagement_id=p.engagement_id
 JOIN hr_engagements e ON e.id=p.engagement_id AND e.company_id=p.company_id AND e.person_id=c.person_id
 JOIN hr_payroll_final_pay_events event ON event.preparation_id=p.id AND event.company_id=p.company_id AND event.run_id=r.id AND event.task_id=t.id AND event.action='cancelled'
 JOIN command_receipts receipt ON event.id='final-pay-cancel-event-'||receipt.command_id
 WHERE p.id=preparation_key AND (event_key IS NULL OR event.id=event_key) AND p.status='cancelled' AND r.status='cancelled'
 AND p.version>=(event.evidence_json::jsonb->'preparation'->>'version')::bigint+1 AND r.version=(event.evidence_json::jsonb->'run'->>'version')::bigint+1
 AND (to_jsonb(p)-ARRAY['status','version','updated_at'])=((event.evidence_json::jsonb->'preparation')-ARRAY['status','version','updated_at'])
 AND (to_jsonb(r)-ARRAY['status','version','updated_at'])=((event.evidence_json::jsonb->'run')-ARRAY['status','version','updated_at'])
 AND event.evidence_json::jsonb->'task'->>'id'=t.id AND event.evidence_json::jsonb->'task'->>'case_id'=c.id
 AND event.evidence_json::jsonb->'task'->>'company_id'=p.company_id AND event.evidence_json::jsonb->'task'->>'status'='cancelled'
 AND event.evidence_json::jsonb->'case'->>'id'=c.id AND event.evidence_json::jsonb->'case'->>'company_id'=p.company_id AND event.evidence_json::jsonb->'case'->>'status'='cancelled'
 AND t.version>=(event.evidence_json::jsonb->'task'->>'version')::bigint AND c.version>=(event.evidence_json::jsonb->'case'->>'version')::bigint
 AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT 1::bigint UNION SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version AND binding.company_id=p.company_id AND binding.target_type='payroll_final_pay' AND binding.target_id=p.id WHERE binding.next_version<=(event.evidence_json::jsonb->'preparation'->>'version')::bigint) SELECT 1 FROM versions WHERE version=(event.evidence_json::jsonb->'preparation'->>'version')::bigint)
 AND receipt.operation='people.payroll.final-pay.cancel' AND receipt.record_id=p.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=p.company_id
 AND receipt.result_json::jsonb->>'id'=p.id AND receipt.result_json::jsonb->>'runId'=r.id AND receipt.result_json::jsonb->>'taskId'=t.id AND receipt.result_json::jsonb->>'caseId'=c.id
 AND receipt.result_json::jsonb->>'status'='cancelled' AND receipt.result_json::jsonb->>'cancellationId'=event.id
 AND (receipt.result_json::jsonb->>'version')::bigint=(event.evidence_json::jsonb->'preparation'->>'version')::bigint+1
 AND (receipt.result_json::jsonb->>'runVersion')::bigint=(event.evidence_json::jsonb->'run'->>'version')::bigint+1
 AND (receipt.result_json::jsonb->>'taskVersion')::bigint=(event.evidence_json::jsonb->'task'->>'version')::bigint
 AND (receipt.result_json::jsonb->>'caseVersion')::bigint=(event.evidence_json::jsonb->'case'->>'version')::bigint
 AND event.task_version=(event.evidence_json::jsonb->'task'->>'version')::bigint
 AND length(btrim(event.evidence_json::jsonb->>'reason')) BETWEEN 8 AND 1000
 AND r.mode='final_pay' AND r.submitted_by IS NULL AND r.approved_by IS NULL AND r.posted_by IS NULL
 AND c.status='cancelled' AND t.status='cancelled' AND (iam_same_person(p.prepared_by,event.actor_id,p.company_id) OR t.assignee_id=event.actor_id)
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_postings x WHERE x.run_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_issued_payslips x WHERE x.run_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_payment_batches x WHERE x.company_id=r.company_id AND x.source_type='payroll' AND x.source_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM finance_journals x WHERE x.company_id=r.company_id AND x.source_type='payroll' AND x.source_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions x WHERE x.company_id=r.company_id AND x.run_id=r.id)
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_items x WHERE x.run_id=r.id AND x.status IN ('approved','posted'))
 AND NOT EXISTS(SELECT 1 FROM hr_payroll_corrections x WHERE x.run_id=r.id AND x.company_id=r.company_id) AND EXISTS(SELECT 1 FROM command_receipts original JOIN hr_payroll_final_pay_events born ON born.id='final-pay-event-'||original.command_id
 WHERE original.operation='people.payroll.final-pay.prepare' AND original.record_id=p.id AND original.actor_id=p.prepared_by AND original.authority_json::jsonb->>'companyId'=p.company_id
 AND original.result_json::jsonb->>'id'=p.id AND original.result_json::jsonb->>'taskId'=p.task_id AND original.result_json::jsonb->>'runId'=p.run_id AND original.result_json::jsonb->>'status'='prepared' AND original.result_json::jsonb->>'version'='1'
 AND born.company_id=p.company_id AND born.preparation_id=p.id AND born.run_id=p.run_id AND born.task_id=p.task_id AND born.actor_id=p.prepared_by AND born.action='prepared') AND EXISTS(SELECT 1 FROM command_receipts parent JOIN audit_events a ON a.command_id=parent.command_id AND a.action=parent.operation AND a.record_id=c.id
 JOIN outbox_events o ON o.command_id=parent.command_id AND o.event_type=parent.operation AND o.aggregate_id=c.id AND o.aggregate_version=a.record_version
 WHERE parent.operation='people.lifecycle.cancel' AND parent.record_id=c.id AND parent.authority_json::jsonb->>'companyId'=c.company_id
 AND parent.result_json::jsonb->>'id'=c.id AND parent.result_json::jsonb->>'status'='cancelled' AND a.record_version<=c.version)
 AND EXISTS(SELECT 1 FROM audit_events audit WHERE audit.command_id=receipt.command_id AND audit.action=receipt.operation AND audit.record_id=p.id AND audit.record_version=(receipt.result_json::jsonb->>'version')::bigint AND audit.actor_id=receipt.actor_id)
 AND EXISTS(SELECT 1 FROM outbox_events message WHERE message.command_id=receipt.command_id AND message.event_type=receipt.operation AND message.aggregate_id=p.id AND message.aggregate_version=(receipt.result_json::jsonb->>'version')::bigint))
$$;
CREATE OR REPLACE FUNCTION hr_payroll_draft_cancellation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND NEW.status='cancelled' AND OLD.mode='final_pay' AND OLD.status!='cancelled' THEN
  IF NEW.version!=OLD.version+1 OR (to_jsonb(NEW)-ARRAY['status','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','updated_at'])
   OR NOT hr_final_pay_cancellation_candidate(OLD) THEN
   RAISE EXCEPTION 'PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
 END IF;
 IF OLD.status='cancelled' THEN
  RAISE EXCEPTION 'Cancelled payroll source is retained and terminal' USING ERRCODE='23514';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 IF NEW.status='cancelled' THEN
  IF OLD.status NOT IN ('draft','calculating','failed','preview') OR OLD.mode='final_pay'
    OR OLD.submitted_by IS NOT NULL OR OLD.approved_by IS NOT NULL OR OLD.posted_by IS NOT NULL
    OR NEW.version!=OLD.version+1
    OR (to_jsonb(NEW)-ARRAY['status','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','updated_at'])
    OR EXISTS(SELECT 1 FROM hr_payroll_postings p WHERE p.run_id=OLD.id)
    OR EXISTS(SELECT 1 FROM hr_issued_payslips p WHERE p.run_id=OLD.id)
    OR EXISTS(SELECT 1 FROM hr_payment_batches p WHERE p.company_id=OLD.company_id AND p.source_type='payroll' AND p.source_id=OLD.id)
    OR EXISTS(SELECT 1 FROM finance_journals j WHERE j.company_id=OLD.company_id AND j.source_type='payroll' AND j.source_id=OLD.id) THEN
   RAISE EXCEPTION 'Only an unsubmitted ordinary payroll source can be cancelled without changing its evidence' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION hr_payroll_run_cancellation_receipt_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.status='cancelled' AND OLD.status!='cancelled' AND OLD.mode='final_pay' THEN
  IF NOT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations p WHERE p.run_id=NEW.id AND p.company_id=NEW.company_id AND hr_final_pay_cancellation_valid(p.id)) THEN
   RAISE EXCEPTION 'PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
 END IF;
 IF NEW.status='cancelled' AND OLD.status!='cancelled' AND NOT EXISTS(
   SELECT 1 FROM hr_payroll_run_cancellations c WHERE c.run_id=NEW.id AND c.company_id=NEW.company_id
     AND c.prior_version=OLD.version AND c.prior_status=OLD.status AND c.run_revision=OLD.revision
     AND c.source_snapshot_json::jsonb=to_jsonb(OLD)) THEN
  RAISE EXCEPTION 'Payroll cancellation must retain its immutable owner receipt' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;

CREATE FUNCTION hr_final_pay_cancellation_preparation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' AND OLD.status='cancelled' THEN RAISE EXCEPTION 'Cancelled final-pay preparation is retained' USING ERRCODE='23514'; END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 IF TG_OP='INSERT' THEN
  IF NEW.status='cancelled' THEN RAISE EXCEPTION 'PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  RETURN NEW;
 END IF;
 IF NEW.status='cancelled' AND OLD.status!='cancelled' THEN
  IF OLD.status!='prepared' OR NEW.version!=OLD.version+1 OR (to_jsonb(NEW)-ARRAY['status','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','updated_at'])
   OR NOT EXISTS(SELECT 1 FROM hr_payroll_final_pay_events event JOIN command_receipts receipt ON event.id='final-pay-cancel-event-'||receipt.command_id
    WHERE event.preparation_id=OLD.id AND event.company_id=OLD.company_id AND event.action='cancelled' AND event.evidence_json::jsonb->'preparation'=to_jsonb(OLD)
    AND receipt.operation='people.payroll.final-pay.cancel' AND receipt.record_id=OLD.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=OLD.company_id) THEN
   RAISE EXCEPTION 'PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED' USING ERRCODE='23514';
  END IF;
 ELSIF OLD.status='cancelled' AND (NEW.status!='cancelled' OR (to_jsonb(NEW)-ARRAY['version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['version','updated_at'])) THEN
  RAISE EXCEPTION 'Cancelled final-pay preparation is retained' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_final_pay_cancellation_preparation_guard BEFORE INSERT OR UPDATE OR DELETE ON hr_payroll_final_pay_preparations
 FOR EACH ROW EXECUTE FUNCTION hr_final_pay_cancellation_preparation_guard();
CREATE FUNCTION hr_final_pay_cancellation_receipt_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE preparation_key text; event_key text;
BEGIN
 IF TG_ARGV[0]='receipt' THEN
  IF NEW.operation!='people.payroll.final-pay.cancel' THEN RETURN NEW; END IF;
  preparation_key:=NEW.record_id;event_key:='final-pay-cancel-event-'||NEW.command_id;
 ELSIF TG_ARGV[0]='event' THEN
  IF NEW.action!='cancelled' THEN RETURN NEW; END IF;
  preparation_key:=NEW.preparation_id;event_key:=NEW.id;
 ELSE
  IF NEW.status!='cancelled' THEN RETURN NEW; END IF;
  preparation_key:=NEW.id;
 END IF;
 IF NOT hr_final_pay_cancellation_valid(preparation_key,event_key) THEN
  RAISE EXCEPTION 'PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER hr_final_pay_cancellation_receipt_guard AFTER INSERT OR UPDATE ON command_receipts
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_final_pay_cancellation_receipt_guard('receipt');
CREATE CONSTRAINT TRIGGER hr_final_pay_cancellation_event_guard AFTER INSERT ON hr_payroll_final_pay_events
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_final_pay_cancellation_receipt_guard('event');
CREATE CONSTRAINT TRIGGER hr_final_pay_cancellation_preparation_receipt_guard AFTER INSERT OR UPDATE ON hr_payroll_final_pay_preparations
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_final_pay_cancellation_receipt_guard('preparation');

-- A retained cancellation cannot be retrofitted into a second command/event.
CREATE FUNCTION hr_final_pay_cancellation_event_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.action!='cancelled' THEN RETURN NEW; END IF;
 IF NOT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations p JOIN hr_payroll_runs r ON r.id=p.run_id AND r.company_id=p.company_id
  JOIN people_preparation_tasks t ON t.id=p.task_id AND t.company_id=p.company_id AND t.case_id=p.case_id JOIN people_lifecycle_cases c ON c.id=p.case_id AND c.company_id=p.company_id
  JOIN command_receipts receipt ON NEW.id='final-pay-cancel-event-'||receipt.command_id AND receipt.operation='people.payroll.final-pay.cancel' AND receipt.record_id=p.id AND receipt.actor_id=NEW.actor_id AND receipt.authority_json::jsonb->>'companyId'=p.company_id
  WHERE p.id=NEW.preparation_id AND p.company_id=NEW.company_id AND p.run_id=NEW.run_id AND p.task_id=NEW.task_id AND p.status='prepared'
   AND r.mode='final_pay' AND r.status IN ('draft','calculating','failed','preview') AND r.submitted_by IS NULL AND r.approved_by IS NULL AND r.posted_by IS NULL
   AND t.status='cancelled' AND t.clearance_type='payroll' AND c.status='cancelled'
   AND NEW.evidence_json::jsonb->'preparation'=to_jsonb(p) AND NEW.evidence_json::jsonb->'run'=to_jsonb(r) AND NEW.evidence_json::jsonb->'task'=to_jsonb(t) AND NEW.evidence_json::jsonb->'case'=to_jsonb(c)
   AND NEW.task_version=t.version AND (iam_same_person(p.prepared_by,NEW.actor_id,p.company_id) OR t.assignee_id=NEW.actor_id)) THEN
  RAISE EXCEPTION 'PAYROLL_FINAL_PAY_CANCELLATION_SOURCE_CHANGED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_final_pay_cancellation_event_birth BEFORE INSERT ON hr_payroll_final_pay_events FOR EACH ROW EXECUTE FUNCTION hr_final_pay_cancellation_event_birth();
CREATE FUNCTION hr_final_pay_cancellation_command_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.operation='people.payroll.final-pay.cancel' AND EXISTS(SELECT 1 FROM hr_payroll_final_pay_events event WHERE event.id='final-pay-cancel-event-'||OLD.command_id AND event.action='cancelled') THEN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Final-pay cancellation receipt is retained and immutable' USING ERRCODE='23514'; END IF;
  IF to_jsonb(NEW) IS DISTINCT FROM to_jsonb(OLD) THEN RAISE EXCEPTION 'Final-pay cancellation receipt is retained and immutable' USING ERRCODE='23514'; END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_final_pay_cancellation_command_immutable BEFORE UPDATE OR DELETE ON command_receipts FOR EACH ROW EXECUTE FUNCTION hr_final_pay_cancellation_command_immutable();
