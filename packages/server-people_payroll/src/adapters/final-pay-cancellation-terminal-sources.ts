import 'server-only';
import {recordAttachmentVersionSteps,recordAttachmentVersionSourceReadLocks} from '@erpx/server-files/public/record-version-receipts';
import {payrollFinalPayCancellationJournalSources,payrollFinalPayCancellationFinanceSourceLocks} from '@erpx/server-finance/public/payroll-final-pay-cancellation-sources';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Existing native rows only: safe before1067, absent receipts never settle a
 * preparation. The scope CTE is supplied by the two original owner readers. */
export const finalPayCancellationTerminalRows=`WITH candidates AS MATERIALIZED(
 SELECT p.id preparation_id,p.company_id,p.version,r.id run_id,r.version run_version,(receipt.result_json::jsonb->>'version')::bigint terminal_version
 FROM hr_payroll_final_pay_preparations p JOIN hr_payroll_runs r ON r.id=p.run_id AND r.company_id=p.company_id AND r.scope_engagement_id=p.engagement_id
 JOIN people_preparation_tasks t ON t.id=p.task_id AND t.company_id=p.company_id AND t.case_id=p.case_id AND t.clearance_type='payroll'
 JOIN people_lifecycle_cases c ON c.id=p.case_id AND c.company_id=p.company_id AND c.engagement_id=p.engagement_id
 JOIN hr_engagements e ON e.id=p.engagement_id AND e.company_id=p.company_id AND e.person_id=c.person_id
 JOIN hr_payroll_final_pay_events event ON event.preparation_id=p.id AND event.company_id=p.company_id AND event.run_id=r.id AND event.task_id=t.id AND event.action='cancelled'
 JOIN command_receipts receipt ON event.id='final-pay-cancel-event-'||receipt.command_id
 CROSS JOIN scope WHERE p.company_id=scope.company_id AND to_regprocedure('hr_final_pay_cancellation_valid(text,text)') IS NOT NULL AND p.status='cancelled' AND r.status='cancelled'
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
 AND NOT EXISTS(SELECT 1 FROM (${payrollFinalPayCancellationJournalSources}) x WHERE x.company_id=r.company_id AND x.source_type='payroll' AND x.source_id=r.id)
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
 AND EXISTS(SELECT 1 FROM outbox_events message WHERE message.command_id=receipt.command_id AND message.event_type=receipt.operation AND message.aggregate_id=p.id AND message.aggregate_version=(receipt.result_json::jsonb->>'version')::bigint)),settled AS(SELECT candidate.* FROM candidates candidate WHERE EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT candidate.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (${recordAttachmentVersionSteps}) binding ON binding.previous_version=versions.version
 AND binding.company_id=candidate.company_id AND binding.target_type='payroll_final_pay' AND binding.target_id=candidate.preparation_id WHERE binding.next_version<=candidate.version)
 SELECT 1 FROM versions WHERE version=candidate.version)) SELECT * FROM settled`;
export async function finalPayCancellationTerminalSourceReadLocks(database:SqlDatabase){
 await database.prepare('LOCK TABLE command_receipts,hr_payroll_final_pay_events,hr_issued_payslips,hr_payroll_deduction_consumptions,audit_events,outbox_events IN SHARE MODE').run();
 await database.batch([...recordAttachmentVersionSourceReadLocks(database),...payrollFinalPayCancellationFinanceSourceLocks(database)]);
}
export async function finalPayCancellationTerminal(database:SqlDatabase,companyId:string,preparationId:string){return !!await database.prepare(`WITH scope AS(SELECT ?::text company_id) SELECT 1 FROM (${finalPayCancellationTerminalRows}) terminal WHERE terminal.preparation_id=? AND terminal.company_id=?`).bind(companyId,preparationId,companyId).first();}
