import {finalPayCancellationTerminalRows,finalPayCancellationTerminalSourceReadLocks} from '@erpx/server-people_payroll/adapters/final-pay-cancellation-terminal-sources';
import 'server-only';
import {expensePaymentCancellations} from '@erpx/server-people_payroll/adapters/expense-payment-cancellation-sources';
import {createHash} from 'node:crypto';
import {recordAttachmentVersionSteps,recordAttachmentVersionSourceReadLocks} from '@erpx/server-files/public/record-version-receipts';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '@erpx/server-commercial/public/package-settlement-sources';

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
type Fact={kind:string;id:string;version:number;state:string|null;image:unknown;pending:boolean};
const sources=new WeakMap<SqlDatabase,string>();
/** These are native addon owners, not the base People directory, Workforce,
 * Evaluation, Benefits plans or contractor payment batches. The settlement
 * wrapper holds its package fence before these absent-row/source fences. */
export async function lockPeopleAddonPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await finalPayCancellationTerminalSourceReadLocks(database);
 await database.prepare(`LOCK TABLE command_receipts,hr_people,hr_engagements,hr_pay_groups,
 hr_payroll_runs,hr_payroll_items,hr_payroll_postings,hr_payroll_run_cancellations,
 hr_payroll_corrections,hr_payroll_correction_events,hr_payroll_advances,hr_payroll_advance_repayments,hr_payroll_advance_events,
 hr_payroll_recoveries,hr_payroll_recovery_events,hr_payroll_deduction_consumptions,
 hr_payroll_adjustments,hr_payroll_adjustment_revisions,hr_payroll_adjustment_time_claims,
 hr_payroll_final_pay_preparations,hr_payroll_final_pay_events,people_lifecycle_cases,people_preparation_tasks,
 hr_payroll_account_mappings,hr_payment_batches,hr_payment_batch_items,hr_payment_batch_exports,
 hr_payroll_payment_settlements,hr_payroll_statutory_remittances,
 finance_expenses,expense_decisions,expense_payment_drafts,expense_ai_proposals,expense_ai_usage IN SHARE MODE`).run();
 await database.batch(recordAttachmentVersionSourceReadLocks(database));sources.set(database,companyId);
}
export function assertPeopleAddonPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){
 owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error('PEOPLE_ADDON_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
}
export const peopleAddonSettlementSources={
 expensePaymentCancellations,
 runs:'SELECT * FROM hr_payroll_runs',items:'SELECT * FROM hr_payroll_items',postings:'SELECT * FROM hr_payroll_postings',
 batches:'SELECT * FROM hr_payment_batches',batchItems:'SELECT * FROM hr_payment_batch_items',
 settlements:'SELECT * FROM hr_payroll_payment_settlements',remittances:'SELECT * FROM hr_payroll_statutory_remittances',
 recoveries:'SELECT * FROM hr_payroll_recoveries',advances:'SELECT * FROM hr_payroll_advances',repayments:'SELECT * FROM hr_payroll_advance_repayments',
} as const;
const payrollFacts=`WITH scope AS(SELECT ?::text company_id),
 runs AS MATERIALIZED(SELECT r.* FROM hr_payroll_runs r,scope WHERE r.company_id=scope.company_id),
 final_pay_terminals AS MATERIALIZED(${finalPayCancellationTerminalRows}),
 facts AS(
 SELECT 'run' kind,r.id,r.version,r.status state,to_jsonb(r) image,
 r.status NOT IN ('posted','cancelled') OR g.id IS NULL OR g.company_id IS DISTINCT FROM r.company_id
 OR (r.status='cancelled' AND NOT EXISTS(SELECT 1 FROM final_pay_terminals fp WHERE fp.run_id=r.id AND fp.company_id=r.company_id AND fp.run_version=r.version) AND NOT EXISTS(SELECT 1 FROM hr_payroll_run_cancellations x JOIN command_receipts c ON c.command_id=x.command_id
 WHERE x.run_id=r.id AND x.company_id=r.company_id AND x.prior_version+1=r.version AND x.run_revision=r.revision
 AND c.operation='people.payroll.cancel' AND c.record_id=r.id AND c.actor_id=x.cancelled_by AND c.authority_json::jsonb->>'companyId'=r.company_id
 AND c.result_json::jsonb->>'id'=r.id AND c.result_json::jsonb->>'status'='cancelled' AND c.result_json::jsonb->>'version'=r.version::text
 AND c.result_json::jsonb->>'cancellationId'=x.id
 AND (x.source_snapshot_json::jsonb-ARRAY['status','version','updated_at'])=(to_jsonb(r)-ARRAY['status','version','updated_at']))) pending
 FROM runs r LEFT JOIN hr_pay_groups g ON g.id=r.pay_group_id
 UNION ALL SELECT 'schedule',g.id,g.schedule_version,CASE WHEN g.schedule_enabled THEN 'enabled' ELSE 'disabled' END,to_jsonb(g),g.schedule_enabled
 FROM hr_pay_groups g,scope WHERE g.company_id=scope.company_id
 UNION ALL SELECT 'item',i.id,1,i.status,to_jsonb(i),r.id IS NULL OR e.id IS NULL OR e.company_id IS DISTINCT FROM r.company_id
 FROM hr_payroll_items i LEFT JOIN runs r ON r.id=i.run_id LEFT JOIN hr_engagements e ON e.id=i.engagement_id CROSS JOIN scope
 WHERE r.id IS NOT NULL OR e.company_id=scope.company_id
 UNION ALL SELECT 'correction',c.id,c.version,c.status,to_jsonb(c),c.status='pending' OR r.id IS NULL OR (c.status<>'pending' AND NOT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='people.payroll.correction.'||CASE c.status WHEN 'approved' THEN 'approve' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END AND receipt.record_id=c.id AND receipt.authority_json::jsonb->>'companyId'=c.company_id AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'=c.status AND receipt.result_json::jsonb->>'version'=c.version::text))
 FROM hr_payroll_corrections c CROSS JOIN scope LEFT JOIN runs r ON r.id=c.run_id WHERE c.company_id=scope.company_id
 UNION ALL SELECT 'advance',a.id,a.version,a.status,to_jsonb(a),a.status IN ('pending','approved') OR e.id IS NULL OR e.company_id IS DISTINCT FROM a.company_id OR e.person_id IS DISTINCT FROM a.person_id
 OR (a.status IN ('rejected','cancelled') AND NOT EXISTS(SELECT 1 FROM hr_payroll_advance_events event WHERE event.advance_id=a.id AND event.company_id=a.company_id AND event.action=a.status AND event.advance_version=a.version AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='people.payroll.advance.'||CASE a.status WHEN 'rejected' THEN 'reject' ELSE 'cancel' END AND receipt.record_id=a.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=a.company_id AND receipt.result_json::jsonb->>'id'=a.id AND receipt.result_json::jsonb->>'status'=a.status AND receipt.result_json::jsonb->>'version'=a.version::text)))
 OR (a.status='closed' AND ((SELECT SUM(p.amount_minor) FROM hr_payroll_advance_repayments p WHERE p.advance_id=a.id AND p.status='collected') IS DISTINCT FROM a.amount_minor OR NOT EXISTS(SELECT 1 FROM hr_payroll_advance_repayments p WHERE p.advance_id=a.id) OR EXISTS(SELECT 1 FROM hr_payroll_advance_repayments p WHERE p.advance_id=a.id AND p.status<>'collected')))
 FROM hr_payroll_advances a CROSS JOIN scope LEFT JOIN hr_engagements e ON e.id=a.engagement_id WHERE a.company_id=scope.company_id
 UNION ALL SELECT 'repayment',p.id,p.version,p.status,to_jsonb(p),p.status IN ('open','reserved','waived') OR a.id IS NULL OR a.company_id IS DISTINCT FROM p.company_id
 OR (p.status='collected' AND NOT EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions c JOIN runs r ON r.id=c.run_id JOIN hr_payroll_items i ON i.id=c.item_id
 JOIN command_receipts receipt ON receipt.command_id=c.command_id WHERE c.repayment_id=p.id AND c.company_id=p.company_id AND c.run_id=p.payroll_run_id
 AND c.source_version+1=p.version AND c.amount_minor=p.amount_minor AND r.status='posted' AND i.run_id=r.id AND i.engagement_id=a.engagement_id AND i.status='posted'
 AND receipt.operation='people.payroll.post' AND receipt.record_id=r.id AND receipt.authority_json::jsonb->>'companyId'=p.company_id AND receipt.result_json::jsonb->>'journalId'=c.journal_id))
 FROM hr_payroll_advance_repayments p CROSS JOIN scope LEFT JOIN hr_payroll_advances a ON a.id=p.advance_id WHERE p.company_id=scope.company_id
 UNION ALL SELECT 'recovery',r.id,r.version,r.status,to_jsonb(r),r.status IN ('consent_pending','review_pending','approved') OR e.id IS NULL OR e.company_id IS DISTINCT FROM r.company_id OR e.person_id IS DISTINCT FROM r.person_id
 OR NOT EXISTS(SELECT 1 FROM runs origin JOIN hr_payroll_items i ON i.run_id=origin.id WHERE origin.id=r.original_run_id AND i.id=r.original_item_id AND i.engagement_id=r.engagement_id AND origin.status='posted' AND i.status='posted')
 OR NOT EXISTS(SELECT 1 FROM hr_payroll_recovery_events event JOIN command_receipts receipt ON receipt.command_id=event.command_id WHERE event.recovery_id=r.id AND event.company_id=r.company_id AND event.recovery_version=r.version
 AND receipt.record_id=r.id AND receipt.authority_json::jsonb->>'companyId'=r.company_id AND receipt.result_json::jsonb->>'status'=r.status AND receipt.result_json::jsonb->>'version'=r.version::text)
 FROM hr_payroll_recoveries r CROSS JOIN scope LEFT JOIN hr_engagements e ON e.id=r.engagement_id WHERE r.company_id=scope.company_id
 UNION ALL SELECT 'mapping',m.id,m.version,m.status,to_jsonb(m),m.status='draft' OR g.id IS NULL OR g.company_id IS DISTINCT FROM m.company_id
 FROM hr_payroll_account_mappings m CROSS JOIN scope LEFT JOIN hr_pay_groups g ON g.id=m.pay_group_id WHERE m.company_id=scope.company_id
 UNION ALL SELECT 'final-pay',p.id,p.version,p.status,to_jsonb(p),r.id IS NULL OR e.id IS NULL OR e.company_id IS DISTINCT FROM p.company_id OR c.id IS NULL OR c.company_id IS DISTINCT FROM p.company_id OR c.engagement_id IS DISTINCT FROM p.engagement_id
 OR t.id IS NULL OR t.case_id IS DISTINCT FROM p.case_id OR t.company_id IS DISTINCT FROM p.company_id OR (p.status='cancelled' AND NOT EXISTS(SELECT 1 FROM final_pay_terminals fp WHERE fp.preparation_id=p.id AND fp.company_id=p.company_id AND fp.version=p.version)) OR (p.status='prepared' AND r.status<>'posted') OR NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN hr_payroll_final_pay_events event ON event.id='final-pay-event-'||receipt.command_id WHERE receipt.operation='people.payroll.final-pay.prepare' AND receipt.record_id=p.id AND receipt.actor_id=p.prepared_by AND receipt.authority_json::jsonb->>'companyId'=p.company_id AND receipt.result_json::jsonb->>'id'=p.id AND receipt.result_json::jsonb->>'taskId'=p.task_id AND receipt.result_json::jsonb->>'runId'=p.run_id AND receipt.result_json::jsonb->>'status'='prepared' AND receipt.result_json::jsonb->>'version'='1' AND event.company_id=p.company_id AND event.preparation_id=p.id AND event.run_id=p.run_id AND event.task_id=p.task_id AND event.actor_id=p.prepared_by AND event.action='prepared')
 FROM hr_payroll_final_pay_preparations p CROSS JOIN scope LEFT JOIN runs r ON r.id=p.run_id LEFT JOIN hr_engagements e ON e.id=p.engagement_id LEFT JOIN people_lifecycle_cases c ON c.id=p.case_id LEFT JOIN people_preparation_tasks t ON t.id=p.task_id WHERE p.company_id=scope.company_id
 UNION ALL SELECT 'adjustment',a.run_id,a.source_revision,r.status,to_jsonb(a),r.id IS NULL OR origin.id IS NULL OR origin.status<>'posted' OR r.adjustment_of_run_id IS DISTINCT FROM origin.id OR r.status NOT IN ('posted','cancelled')
 OR NOT EXISTS(SELECT 1 FROM hr_payroll_adjustment_revisions rev JOIN command_receipts receipt ON receipt.command_id=rev.command_id WHERE rev.run_id=a.run_id AND rev.revision=a.source_revision AND receipt.authority_json::jsonb->>'companyId'=a.company_id)
 OR (r.status='cancelled' AND (a.active OR EXISTS(SELECT 1 FROM hr_payroll_adjustment_time_claims claim WHERE claim.run_id=a.run_id AND claim.active)))
 FROM hr_payroll_adjustments a CROSS JOIN scope LEFT JOIN runs r ON r.id=a.run_id LEFT JOIN runs origin ON origin.id=a.original_run_id WHERE a.company_id=scope.company_id
 ) SELECT kind,id,version,state,image,COALESCE(pending,true) pending FROM facts ORDER BY kind,id`;
const expenseFacts=`WITH scope AS(SELECT ?::text company_id),
 terminals AS MATERIALIZED(SELECT e.id,e.company_id,e.version,(receipt.result_json::jsonb->>'version')::bigint terminal_version
 FROM finance_expenses e CROSS JOIN scope JOIN command_receipts receipt ON receipt.record_id=e.id AND receipt.authority_json::jsonb->>'companyId'=e.company_id
 WHERE e.company_id=scope.company_id AND e.status='rejected' AND receipt.result_json::jsonb->>'id'=e.id AND receipt.result_json::jsonb->>'version'~'^[1-9][0-9]*$'
 AND ((receipt.operation='expense.reject' AND e.cancelled_at IS NULL AND e.journal_id IS NULL AND receipt.result_json::jsonb->>'status'='rejected')
 OR (receipt.operation='expense.cancel' AND e.cancelled_at IS NOT NULL AND e.journal_id IS NULL AND receipt.result_json::jsonb->>'status'=CASE WHEN e.workflow_version=1 THEN 'rejected' ELSE 'cancelled' END)
 OR (receipt.operation='expense.reverse' AND e.cancelled_at IS NOT NULL AND e.reversal_journal_id IS NOT NULL AND receipt.result_json::jsonb->>'status'='cancelled' AND receipt.result_json::jsonb->>'reversalJournalId'=e.reversal_journal_id))),
 settled AS MATERIALIZED(SELECT terminal.id FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (${recordAttachmentVersionSteps}) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_type='expense_claim' AND binding.target_id=terminal.id WHERE binding.next_version<=terminal.version)
 SELECT 1 FROM versions WHERE version=terminal.version)),facts AS(
 SELECT 'expense' kind,e.id,e.version,e.status state,to_jsonb(e) image,
 e.status NOT IN ('rejected','reimbursed') OR (e.status='rejected' AND NOT EXISTS(SELECT 1 FROM settled WHERE settled.id=e.id)) pending
 FROM finance_expenses e,scope WHERE e.company_id=scope.company_id
 UNION ALL SELECT 'payment-draft',p.id,p.version,p.status,to_jsonb(p),(p.status<>'confirmed' AND NOT EXISTS(SELECT 1 FROM (${expensePaymentCancellations}) cancelled WHERE cancelled.id=p.id AND cancelled.company_id=p.company_id)) OR e.id IS NULL OR e.company_id IS DISTINCT FROM p.company_id
 FROM expense_payment_drafts p CROSS JOIN scope LEFT JOIN finance_expenses e ON e.id=p.expense_id WHERE p.company_id=scope.company_id
 UNION ALL SELECT 'ai-proposal',p.id,p.attempts,p.state,to_jsonb(p),p.state IN ('queued','running') OR e.id IS NULL OR e.company_id IS DISTINCT FROM p.company_id
 OR (p.payment_draft_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM expense_payment_drafts d WHERE d.id=p.payment_draft_id AND d.company_id=p.company_id AND d.expense_id=p.expense_id))
 FROM expense_ai_proposals p CROSS JOIN scope LEFT JOIN finance_expenses e ON e.id=p.expense_id WHERE p.company_id=scope.company_id
 UNION ALL SELECT 'ai-reservation',u.id,1,u.state,to_jsonb(u),u.state='reserved' FROM expense_ai_usage u,scope WHERE u.company_id=scope.company_id
 ) SELECT kind,id,version,state,image,COALESCE(pending,true) pending FROM facts ORDER BY kind,id`;

/** Child evidence contributes to the revision without inventing a duty for
 * every standing worker or every unrelated Benefit election. */
export async function peopleAddonPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertPeopleAddonPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=='people_payroll'||!['people-payroll','people-expenses'].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error('PEOPLE_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const payroll=input.packageId==='people-payroll';
 const facts=(await database.prepare(payroll?payrollFacts:expenseFacts).bind(input.companyId).all<Fact>()).results;
 const receipts=(await database.prepare(`SELECT to_jsonb(receipt)::text image FROM command_receipts receipt WHERE receipt.authority_json::jsonb->>'companyId'=? AND ${payroll?"(receipt.operation LIKE 'people.payroll%' OR receipt.operation LIKE 'payroll.%' OR receipt.operation LIKE 'people.payment-batch.%')":"receipt.operation LIKE 'expense.%'"} ORDER BY receipt.command_id`).bind(input.companyId).all()).results;
 const evidence=(await database.prepare(payroll?`SELECT kind,id,image FROM (
 SELECT 'cancellation' kind,x.id,to_jsonb(x) image FROM hr_payroll_run_cancellations x WHERE x.company_id=?
 UNION ALL SELECT 'deduction',x.id,to_jsonb(x) FROM hr_payroll_deduction_consumptions x WHERE x.company_id=?
 UNION ALL SELECT 'adjustment-revision',x.id,to_jsonb(x) FROM hr_payroll_adjustment_revisions x JOIN hr_payroll_adjustments a ON a.run_id=x.run_id WHERE a.company_id=?
 UNION ALL SELECT 'adjustment-time',x.run_id||':'||x.source_revision||':'||x.time_entry_id,to_jsonb(x) FROM hr_payroll_adjustment_time_claims x JOIN hr_payroll_adjustments a ON a.run_id=x.run_id WHERE a.company_id=?
 UNION ALL SELECT 'advance-event',x.id,to_jsonb(x) FROM hr_payroll_advance_events x WHERE x.company_id=?
 UNION ALL SELECT 'recovery-event',x.id,to_jsonb(x) FROM hr_payroll_recovery_events x WHERE x.company_id=?
 UNION ALL SELECT 'correction-event',x.id,to_jsonb(x) FROM hr_payroll_correction_events x WHERE x.company_id=?
 UNION ALL SELECT 'final-pay-event',x.id,to_jsonb(x) FROM hr_payroll_final_pay_events x WHERE x.company_id=?
 UNION ALL SELECT 'batch-item',x.id,to_jsonb(x) FROM hr_payment_batch_items x JOIN hr_payment_batches b ON b.id=x.batch_id WHERE b.company_id=? AND b.source_type='payroll'
 UNION ALL SELECT 'batch-export',x.id,to_jsonb(x) FROM hr_payment_batch_exports x JOIN hr_payment_batches b ON b.id=x.batch_id WHERE b.company_id=? AND b.source_type='payroll'
 ) evidence ORDER BY kind,id`:`SELECT d.id,to_jsonb(d) image FROM expense_decisions d JOIN finance_expenses e ON e.id=d.expense_id WHERE e.company_id=? ORDER BY d.id`)
 .bind(...(payroll?Array(10).fill(input.companyId):[input.companyId])).all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:input.moduleId,observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts,receipts,evidence})).digest('hex'),outstanding:facts.filter(fact=>fact.pending).length};
}
