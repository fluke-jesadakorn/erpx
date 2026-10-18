import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '../../commercial/public/package-settlement-sources';
import {lockPerformanceFilePackageObligationSources,performanceFilePackageObligationFacts} from '../../files/public/performance-package-obligation-sources';
import {performanceSourceReadLocks,performanceSourceImage} from './performance-purchases';
import {performanceCompensationMoneySources as moneySources,performanceCompensationMoneySourceReadLocks} from '../../finance/public/performance-compensation-sources';
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockPerformancePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);await database.batch([...performanceSourceReadLocks(database),...performanceCompensationMoneySourceReadLocks(database)]);await database.prepare('LOCK TABLE command_receipts IN SHARE MODE').run();await lockPerformanceFilePackageObligationSources(owner,lock);sources.set(database,companyId);
}
/** Evaluation owns the actual compensation revision handoff. Once independently
 * approved, standing employment/pay is retained property, not an endless duty
 * to buy Evaluation. No Benefit election, Finance posting or generic Work
 * projection is substituted for these native sources. */
export async function performancePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error('PERFORMANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 if(input.packageId!=='people-performance'||input.packageVersion!==1||input.moduleId!=='people_payroll'||!Number.isSafeInteger(input.now)||input.now<0)throw Error('PERFORMANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id),
 definitions AS MATERIALIZED(SELECT d.* FROM hr_evaluation_definitions d,scope WHERE d.company_id=scope.company_id),
 assessments AS MATERIALIZED(SELECT a.* FROM hr_evaluation_assessments a,scope WHERE a.company_id=scope.company_id),
 drafts AS MATERIALIZED(SELECT d.* FROM hr_evaluation_compensation_drafts d,scope WHERE d.company_id=scope.company_id),
 facts AS(
 SELECT 'cycle' kind,d.id,d.version,d.payload_json::jsonb->>'status' state,to_jsonb(d) image,
 d.payload_json::jsonb->>'status'<>'closed' OR NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=d.id AND command.operation='people.evaluation.criteria.save' AND command.actor_id=d.updated_by AND command.authority_json::jsonb->>'companyId'=d.company_id AND command.result_json::jsonb->>'version'=d.version::text)
 OR NOT EXISTS(SELECT 1 FROM hr_evaluation_definition_history policy WHERE policy.id=d.payload_json::jsonb->'policy'->>'id' AND policy.company_id=d.company_id AND policy.kind='policy' AND policy.version::text=d.payload_json::jsonb->'policy'->>'version') pending
 FROM definitions d WHERE d.kind='cycle'
 UNION ALL SELECT 'assessment',a.id,a.version,a.status,to_jsonb(a),a.status<>'confirmed' OR e.id IS NULL OR e.company_id IS DISTINCT FROM a.company_id OR cycle.id IS NULL OR cycle.kind<>'cycle'
 OR a.confirmed_by IS NULL OR a.confirmed_by=a.created_by OR NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.operation='people.evaluation.assessment.transition' AND command.record_id=a.id AND command.actor_id=a.confirmed_by AND command.actor_id<>a.created_by AND command.authority_json::jsonb->>'companyId'=a.company_id AND command.result_json::jsonb->>'status'='confirmed' AND command.result_json::jsonb->>'version'=a.version::text)
 FROM assessments a LEFT JOIN hr_engagements e ON e.id=a.engagement_id LEFT JOIN definitions cycle ON cycle.id=a.cycle_id
 UNION ALL SELECT 'compensation-draft',d.id,d.version,d.status,to_jsonb(d)||jsonb_build_object('wage',to_jsonb(wage),'previous',to_jsonb(previous),'engagement',to_jsonb(e)),
 d.status='draft' OR a.id IS NULL OR wage.id IS NULL OR previous.id IS NULL OR e.id IS NULL OR e.company_id IS DISTINCT FROM d.company_id OR previous.engagement_id IS DISTINCT FROM wage.engagement_id OR wage.engagement_id IS DISTINCT FROM a.engagement_id
 OR (d.status='approved' AND (wage.status NOT IN ('approved','superseded') OR d.approved_by IS NULL OR d.approved_by=d.created_by OR wage.approved_by IS DISTINCT FROM d.approved_by OR previous.effective_to IS DISTINCT FROM (wage.effective_from::date-1)::text OR NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.operation='people.evaluation.compensation.approve' AND command.record_id=d.id AND command.actor_id=d.approved_by AND command.actor_id<>d.created_by AND command.authority_json::jsonb->>'companyId'=d.company_id AND command.result_json::jsonb->>'status'='approved' AND command.result_json::jsonb->>'version'=d.version::text)))
 OR (d.status='cancelled' AND (wage.status<>'superseded' OR NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.operation='people.evaluation.compensation.cancel' AND command.record_id=d.id AND command.authority_json::jsonb->>'companyId'=d.company_id AND command.result_json::jsonb->>'status'='cancelled' AND command.result_json::jsonb->>'version'=d.version::text)))
 FROM drafts d LEFT JOIN assessments a ON a.id=d.assessment_id LEFT JOIN hr_compensation_revisions wage ON wage.id=d.compensation_id LEFT JOIN hr_compensation_revisions previous ON previous.id=d.previous_id LEFT JOIN hr_engagements e ON e.id=wage.engagement_id
 UNION ALL SELECT 'compensation-money',d.id,d.version,d.status,to_jsonb(d)||jsonb_build_object('money',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.amount_kind) FROM (${moneySources}) m WHERE m.company_id=d.company_id AND m.source_id=d.compensation_id)),
 EXISTS(SELECT 1 FROM (VALUES('base',d.proposed_json::jsonb->>'base_amount_minor'),('hourly_rate',d.proposed_json::jsonb->>'hourly_rate_minor'),('daily_rate',d.proposed_json::jsonb->>'daily_rate_minor')) expected(kind,units) WHERE NOT EXISTS(SELECT 1 FROM (${moneySources}) m WHERE m.company_id=d.company_id AND m.source_id=d.compensation_id AND m.amount_kind=expected.kind AND m.amount_units::numeric=expected.units::numeric AND m.currency=d.proposed_json::jsonb->>'currency' AND m.currency_scale=2 AND m.transaction_date::text=d.proposed_json::jsonb->>'effective_from'))
 OR EXISTS(SELECT 1 FROM jsonb_array_elements((d.proposed_json::jsonb->>'components_json')::jsonb) component WHERE NOT EXISTS(SELECT 1 FROM (${moneySources}) m WHERE m.company_id=d.company_id AND m.source_id=d.compensation_id AND m.amount_kind='component:'||(component->>'code') AND m.amount_units::numeric=round((component->>'amount')::numeric*100) AND m.currency=d.proposed_json::jsonb->>'currency' AND m.currency_scale=2 AND m.transaction_date::text=d.proposed_json::jsonb->>'effective_from')) FROM drafts d
 UNION ALL SELECT 'payroll-capture',item.id,COALESCE((item.input_snapshot_json::jsonb->'compensation'->>'version')::int,0),run.status,to_jsonb(item)||jsonb_build_object('run',to_jsonb(run),'draft',to_jsonb(d)),
 item.engagement_id IS DISTINCT FROM assessment.engagement_id OR run.company_id IS DISTINCT FROM d.company_id OR item.currency IS DISTINCT FROM d.proposed_json::jsonb->>'currency' OR d.status<>'approved'
 OR EXISTS(SELECT 1 FROM (VALUES('base_amount_minor'),('hourly_rate_minor'),('daily_rate_minor'),('currency'),('pay_frequency'),('components_json')) expected(field) WHERE item.input_snapshot_json::jsonb->'compensation'->>expected.field IS DISTINCT FROM d.proposed_json::jsonb->>expected.field)
 FROM drafts d JOIN hr_payroll_items item ON item.input_snapshot_json::jsonb->'compensation'->>'id'=d.compensation_id LEFT JOIN hr_payroll_runs run ON run.id=item.run_id LEFT JOIN assessments assessment ON assessment.id=d.assessment_id
 UNION ALL SELECT 'ai-job',j.id,j.attempts,j.state,to_jsonb(j),j.state IN ('queued','running') OR (j.state='ready' AND j.result_json IS NULL) FROM hr_evaluation_ai_jobs j,scope WHERE j.company_id=scope.company_id
 UNION ALL SELECT 'evidence',evidence.id,1,evidence.source_type,to_jsonb(evidence),a.id IS NULL FROM hr_evaluation_evidence evidence CROSS JOIN scope LEFT JOIN assessments a ON a.id=evidence.assessment_id WHERE evidence.company_id=scope.company_id
 ) SELECT kind,id,version,state,image,COALESCE(pending,true) pending FROM facts ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;version:number;state:string;image:unknown;pending:boolean}>()).results;
 const files=await performanceFilePackageObligationFacts(owner,database,input.companyId),source=await performanceSourceImage(database,input.companyId),receipts=(await database.prepare("SELECT to_jsonb(command)::text image FROM command_receipts command WHERE command.operation LIKE 'people.evaluation.%' AND command.authority_json::jsonb->>'companyId'=? ORDER BY command.command_id").bind(input.companyId).all()).results;
 const missingFileTargets=files.filter(file=>!facts.some(fact=>fact.kind==='assessment'&&fact.id===file.target_id));
 return {companyId:input.companyId,packageId:input.packageId,moduleId:input.moduleId,observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts,files,source,receipts})).digest('hex'),outstanding:facts.filter(fact=>fact.pending).length+files.filter(file=>file.pending).length+missingFileTargets.length};
}
