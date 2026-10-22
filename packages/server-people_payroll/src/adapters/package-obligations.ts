import {finalPayCancellationTerminalRows,finalPayCancellationTerminalSourceReadLocks} from '@erpx/server-people_payroll/adapters/final-pay-cancellation-terminal-sources';
import {workerPlacementMarkedRows as projectWorkerPlacementMarkedRows} from '@erpx/server-people_payroll/adapters/project-worker-startup-source-sql';
import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {peoplePackageReviewSources as reviews,peoplePackageReviewSourceReadLocks} from "@erpx/server-identity_access/public/people-package-obligation-sources";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
/** Closed raw owner relations for the Benefits/Finance observers. None is a
 * purchase-filtered Work projection or an authority decision. */
export const peoplePackageSettlementSources={
 people:"SELECT id,company_id,user_id,status,version,merged_into_id FROM hr_people",
 engagements:"SELECT * FROM hr_engagements",compensation:"SELECT * FROM hr_compensation_revisions",
 runs:"SELECT * FROM hr_payroll_runs",items:"SELECT * FROM hr_payroll_items",postings:"SELECT * FROM hr_payroll_postings",
 batches:"SELECT * FROM hr_payment_batches",batchItems:"SELECT * FROM hr_payment_batch_items",
 settlements:"SELECT * FROM hr_payroll_payment_settlements",remittances:"SELECT * FROM hr_payroll_statutory_remittances",
 recoveries:"SELECT * FROM hr_payroll_recoveries",advances:"SELECT * FROM hr_payroll_advances",repayments:"SELECT * FROM hr_payroll_advance_repayments",
} as const;
export async function lockPeoplePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await finalPayCancellationTerminalSourceReadLocks(database);
 await database.prepare(`LOCK TABLE hr_people,hr_engagements,hr_compensation_revisions,people_lifecycle_cases,people_preparation_tasks,
 people_lifecycle_grants,people_access_cutoffs,people_roster_weeks,people_roster_shifts,people_roster_assignments,people_roster_requests,
 hr_time_entries,leave_requests,leave_day_segments,leave_balance_entries,leave_cancellation_reviews,leave_types,leave_policy_versions,
 leave_policy_rules,leave_policy_assignments,leave_holiday_calendars,leave_holidays,overtime_requests,overtime_eligibility,overtime_payroll_handoffs,
 hr_pay_groups,hr_payroll_runs,hr_payroll_items,hr_payroll_postings,hr_payment_batches,hr_payment_batch_items,
 hr_payroll_payment_settlements,hr_payroll_statutory_remittances,hr_payroll_corrections,hr_payroll_final_pay_preparations,
 hr_payroll_advances,hr_payroll_advance_repayments,hr_payroll_recoveries,hr_payroll_account_mappings,
 hr_evaluation_assessments,hr_evaluation_compensation_drafts,hr_evaluation_ai_jobs,command_receipts IN SHARE MODE`).run();
 await database.batch(peoplePackageReviewSourceReadLocks(database));sources.set(database,companyId);
}
export function assertPeoplePackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("PEOPLE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
/** Employment, standing identity, reviewed compensation and past shifts are
 * retained property. Withdrawal counts unfinished source duties, not workers.
 * Future approved absence and recurring accrual remain conservative duties. */
export async function peoplePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertPeoplePackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="people_payroll"||!["people","people-workforce"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("PEOPLE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean is_base,?::bigint now_ms),
 people AS MATERIALIZED(SELECT p.* FROM hr_people p,scope WHERE p.company_id=scope.company_id),
 engagements AS MATERIALIZED(SELECT e.* FROM hr_engagements e,scope WHERE e.company_id=scope.company_id),
 cases AS MATERIALIZED(SELECT c.* FROM people_lifecycle_cases c,scope WHERE c.company_id=scope.company_id),
 weeks AS MATERIALIZED(SELECT w.* FROM people_roster_weeks w,scope WHERE w.company_id=scope.company_id),
 shifts AS MATERIALIZED(SELECT s.*,w.company_id,w.department_id FROM people_roster_shifts s JOIN weeks w ON w.id=s.week_id),
 leave_sources AS MATERIALIZED(SELECT l.*,p.timezone policy_timezone,t.tracked,t.company_id type_company,p.company_id policy_company,
 COALESCE(NULLIF(l.calculation_snapshot->'policy'->>'timezone',''),p.timezone,'UTC') zone
 FROM leave_requests l CROSS JOIN scope LEFT JOIN leave_policy_versions p ON p.id=l.policy_version_id LEFT JOIN leave_types t ON t.id=l.leave_type_id WHERE l.company_id=scope.company_id),
 final_pay_terminals AS MATERIALIZED(${finalPayCancellationTerminalRows}),facts AS(
 SELECT 'person' kind,p.id,p.version,p.status state,to_jsonb(p) image,
 p.merged_into_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM people target WHERE target.id=p.merged_into_id AND target.id<>p.id) pending
 FROM people p,scope WHERE scope.is_base
 UNION ALL SELECT 'engagement',e.id,e.version,e.status,to_jsonb(e),e.status IN ('draft','onboarding') OR NOT EXISTS(SELECT 1 FROM people p WHERE p.id=e.person_id)
 FROM engagements e,scope WHERE scope.is_base AND NOT EXISTS(SELECT 1 FROM command_receipts source WHERE COALESCE(source.result_json::jsonb->'_projectWorkerStartup'->>'engagementId',source.result_json::jsonb->'_learningWorkerStartup'->>'engagementId')=e.id AND (jsonb_exists(source.result_json::jsonb,'_projectWorkerStartup') OR jsonb_exists(source.result_json::jsonb,'_learningWorkerStartup')))
 UNION ALL SELECT 'compensation',c.id,c.version,c.status,to_jsonb(c),c.status='draft' FROM hr_compensation_revisions c JOIN engagements e ON e.id=c.engagement_id,scope WHERE scope.is_base
 UNION ALL SELECT 'lifecycle',c.id,c.version,c.status,to_jsonb(c),c.status NOT IN ('completed','cancelled') OR p.id IS NULL OR e.id IS NULL OR e.person_id IS DISTINCT FROM c.person_id
 OR (c.status='completed' AND (c.kind='offboarding' AND e.status<>'ended' OR c.kind='suspend' AND e.status NOT IN ('suspended','ended')))
 FROM cases c CROSS JOIN scope LEFT JOIN people p ON p.id=c.person_id LEFT JOIN engagements e ON e.id=c.engagement_id WHERE scope.is_base AND NOT EXISTS(SELECT 1 FROM (${projectWorkerPlacementMarkedRows}) marked WHERE marked.id=c.id)
 UNION ALL SELECT 'clearance',task.id,task.version,task.status,to_jsonb(task),task.status='pending' OR c.id IS NULL
 FROM people_preparation_tasks task CROSS JOIN scope LEFT JOIN cases c ON c.id=task.case_id WHERE scope.is_base AND task.company_id=scope.company_id AND task.case_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM (${projectWorkerPlacementMarkedRows}) marked WHERE marked.id=task.case_id)
 UNION ALL SELECT 'lifecycle-review',g.id,COALESCE(r.version,0),COALESCE(r.status,'unrequested'),to_jsonb(g)||jsonb_build_object('review',to_jsonb(r)),
 c.id IS NULL OR c.status NOT IN ('completed','cancelled') OR (g.review_id IS NOT NULL AND (r.id IS NULL OR r.company_id IS DISTINCT FROM g.company_id OR r.status IN ('pending','failed')))
 FROM people_lifecycle_grants g CROSS JOIN scope LEFT JOIN cases c ON c.id=g.case_id LEFT JOIN (${reviews}) r ON r.id=g.review_id WHERE scope.is_base AND g.company_id=scope.company_id
 UNION ALL SELECT 'roster-shift',s.id,s.version,s.state,to_jsonb(s),s.state='active' AND s.ends_at>scope.now_ms FROM shifts s,scope
 UNION ALL SELECT 'roster-assignment',a.id,a.version,a.state,to_jsonb(a),e.id IS NULL OR p.id IS NULL OR e.person_id IS DISTINCT FROM a.person_id
 FROM people_roster_assignments a JOIN shifts s ON s.id=a.shift_id LEFT JOIN engagements e ON e.id=a.engagement_id LEFT JOIN people p ON p.id=a.person_id
 UNION ALL SELECT 'roster-request',r.id,r.version,r.state,to_jsonb(r),r.state IN ('pending_coworker','pending_manager') OR s.id IS NULL OR s.department_id IS DISTINCT FROM r.department_id OR p.id IS NULL
 OR (r.source_assignment_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM people_roster_assignments a JOIN shifts source ON source.id=a.shift_id WHERE a.id=r.source_assignment_id AND source.company_id=r.company_id AND a.person_id=r.requester_person_id))
 OR (r.target_assignment_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM people_roster_assignments a WHERE a.id=r.target_assignment_id AND a.shift_id=s.id AND a.person_id=r.coworker_person_id))
 FROM people_roster_requests r CROSS JOIN scope LEFT JOIN shifts s ON s.id=r.target_shift_id LEFT JOIN people p ON p.id=r.requester_person_id WHERE r.company_id=scope.company_id
 UNION ALL SELECT 'time',t.id,t.version,t.status,to_jsonb(t),e.id IS NULL OR t.status IN ('draft','submitted') OR (t.status IN ('approved','rejected') AND NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.operation='people.time.'||CASE WHEN t.status='approved' THEN 'approve' ELSE 'reject' END AND command.record_id=t.id AND command.actor_id<>t.submitted_by AND (t.status<>'approved' OR command.actor_id=t.approved_by) AND command.authority_json::jsonb->>'companyId'=t.company_id AND command.result_json::jsonb->>'id'=t.id AND command.result_json::jsonb->>'status'=t.status AND command.result_json::jsonb->>'version'~'^[1-9][0-9]*$' AND (command.result_json::jsonb->>'version')::bigint<=t.version))
 FROM hr_time_entries t CROSS JOIN scope LEFT JOIN engagements e ON e.id=t.engagement_id WHERE t.company_id=scope.company_id
 UNION ALL SELECT 'leave',l.id,l.version,l.state,to_jsonb(l),e.id IS NULL OR (l.person_id IS NOT NULL AND l.person_id IS DISTINCT FROM e.person_id)
 OR l.state IN ('draft','pending_approval') OR COALESCE(l.cancellation_state='pending_review',false)
 OR l.type_company IS DISTINCT FROM l.company_id OR l.policy_company IS DISTINCT FROM l.company_id
 OR (l.state='approved' AND (NOT EXISTS(SELECT 1 FROM pg_timezone_names z WHERE z.name=l.zone)
 OR COALESCE(l.end_date::date,l.start_date::date+(l.days::int-1))>=(to_timestamp(scope.now_ms/1000.0) AT TIME ZONE CASE WHEN EXISTS(SELECT 1 FROM pg_timezone_names z WHERE z.name=l.zone) THEN l.zone ELSE 'UTC' END)::date))
 OR (l.tracked AND l.state IN ('approved','rejected','cancelled') AND
 (COALESCE((SELECT SUM(b.units) FROM leave_balance_entries b WHERE b.request_id=l.id AND b.company_id=l.company_id AND b.kind IN ('reservation','reservation_release')),0)<>0
 OR COALESCE((SELECT SUM(b.units) FROM leave_balance_entries b WHERE b.request_id=l.id AND b.company_id=l.company_id AND b.kind IN ('usage','usage_reversal')),0)<>CASE WHEN l.state='approved' THEN -l.calculated_units ELSE 0 END))
 FROM leave_sources l CROSS JOIN scope LEFT JOIN engagements e ON e.id=l.engagement_id
 UNION ALL SELECT 'leave-cancellation',r.id,r.version,r.state,to_jsonb(r),r.state='pending_approval' OR l.id IS NULL FROM leave_cancellation_reviews r CROSS JOIN scope LEFT JOIN leave_sources l ON l.id=r.request_id WHERE r.company_id=scope.company_id
 UNION ALL SELECT 'leave-ledger',b.id,1,b.kind,to_jsonb(b),e.id IS NULL OR t.id IS NULL OR t.company_id IS DISTINCT FROM b.company_id
 OR (b.request_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM leave_sources l WHERE l.id=b.request_id AND l.engagement_id=b.engagement_id AND l.leave_type_id=b.leave_type_id))
 FROM leave_balance_entries b CROSS JOIN scope LEFT JOIN engagements e ON e.id=b.engagement_id LEFT JOIN leave_types t ON t.id=b.leave_type_id WHERE b.company_id=scope.company_id
 UNION ALL SELECT 'leave-accrual-assignment',a.id,a.version,'assigned',to_jsonb(a)||jsonb_build_object('policy',to_jsonb(p),'rules',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.leave_type_id) FROM leave_policy_rules r WHERE r.policy_version_id=p.id)),
 e.id IS NULL OR p.id IS NULL OR p.company_id IS DISTINCT FROM a.company_id OR NOT EXISTS(SELECT 1 FROM pg_timezone_names z WHERE z.name=p.timezone)
 OR (EXISTS(SELECT 1 FROM leave_policy_rules r JOIN leave_types t ON t.id=r.leave_type_id AND t.company_id=a.company_id WHERE r.policy_version_id=p.id AND t.tracked AND r.grant_kind<>'untracked')
 AND (a.effective_to IS NULL OR a.effective_to::date>=(to_timestamp(scope.now_ms/1000.0) AT TIME ZONE CASE WHEN EXISTS(SELECT 1 FROM pg_timezone_names z WHERE z.name=p.timezone) THEN p.timezone ELSE 'UTC' END)::date))
 FROM leave_policy_assignments a CROSS JOIN scope LEFT JOIN engagements e ON e.id=a.engagement_id LEFT JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=scope.company_id
 UNION ALL SELECT 'overtime',r.id,r.version,r.state,to_jsonb(r),e.id IS NULL OR r.person_id IS DISTINCT FROM e.person_id OR r.state NOT IN ('approved','rejected','withdrawn') OR (r.state='approved' AND NOT EXISTS
 (SELECT 1 FROM overtime_payroll_handoffs h JOIN command_receipts command ON command.record_id=r.id AND command.operation='overtime.approve' AND command.actor_id=h.approved_by
 AND command.authority_json::jsonb->>'companyId'=h.company_id AND command.result_json::jsonb->>'id'=r.id AND command.result_json::jsonb->>'state'='approved' AND command.result_json::jsonb->>'version'=h.request_version::text
 WHERE h.request_id=r.id AND h.company_id=r.company_id AND h.engagement_id=r.engagement_id AND h.minutes=r.minutes AND h.timezone=r.timezone AND h.work_date=left(r.start_local,10)
 AND h.request_version<=r.version AND h.approved_by<>r.created_by))
 FROM overtime_requests r CROSS JOIN scope LEFT JOIN engagements e ON e.id=r.engagement_id WHERE r.company_id=scope.company_id
 UNION ALL SELECT 'overtime-handoff',h.id,h.request_version,'delivered',to_jsonb(h),r.id IS NULL OR r.state<>'approved' OR r.company_id IS DISTINCT FROM h.company_id OR r.engagement_id IS DISTINCT FROM h.engagement_id OR r.minutes IS DISTINCT FROM h.minutes
 FROM overtime_payroll_handoffs h CROSS JOIN scope LEFT JOIN overtime_requests r ON r.id=h.request_id WHERE h.company_id=scope.company_id
 UNION ALL SELECT 'payroll-run',r.id,r.version,r.status,to_jsonb(r),r.status NOT IN ('posted','cancelled') OR g.id IS NULL OR g.company_id IS DISTINCT FROM r.company_id FROM hr_payroll_runs r CROSS JOIN scope LEFT JOIN hr_pay_groups g ON g.id=r.pay_group_id WHERE scope.is_base AND r.company_id=scope.company_id
 UNION ALL SELECT 'payroll-schedule',g.id,g.schedule_version,CASE WHEN g.schedule_enabled THEN 'enabled' ELSE 'disabled' END,to_jsonb(g),g.schedule_enabled FROM hr_pay_groups g,scope WHERE scope.is_base AND g.company_id=scope.company_id
 UNION ALL SELECT 'payroll-correction',c.id,c.version,c.status,to_jsonb(c),c.status='pending' OR NOT EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=c.run_id AND r.company_id=c.company_id) FROM hr_payroll_corrections c,scope WHERE scope.is_base AND c.company_id=scope.company_id
 UNION ALL SELECT 'payroll-advance',a.id,a.version,a.status,to_jsonb(a),a.status IN ('pending','approved') OR e.id IS NULL OR e.person_id IS DISTINCT FROM a.person_id FROM hr_payroll_advances a CROSS JOIN scope LEFT JOIN engagements e ON e.id=a.engagement_id WHERE scope.is_base AND a.company_id=scope.company_id
 UNION ALL SELECT 'payroll-repayment',r.id,r.version,r.status,to_jsonb(r),r.status IN ('open','reserved') OR a.id IS NULL OR a.company_id IS DISTINCT FROM r.company_id FROM hr_payroll_advance_repayments r CROSS JOIN scope LEFT JOIN hr_payroll_advances a ON a.id=r.advance_id WHERE scope.is_base AND r.company_id=scope.company_id
 UNION ALL SELECT 'payroll-recovery',r.id,r.version,r.status,to_jsonb(r),r.status IN ('consent_pending','review_pending','approved') OR e.id IS NULL OR e.person_id IS DISTINCT FROM r.person_id OR NOT EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_payroll_runs run ON run.id=i.run_id WHERE i.id=r.original_item_id AND run.id=r.original_run_id AND run.company_id=r.company_id AND i.engagement_id=r.engagement_id) FROM hr_payroll_recoveries r CROSS JOIN scope LEFT JOIN engagements e ON e.id=r.engagement_id WHERE scope.is_base AND r.company_id=scope.company_id
 UNION ALL SELECT 'payroll-mapping',m.id,m.version,m.status,to_jsonb(m),m.status='draft' OR NOT EXISTS(SELECT 1 FROM hr_pay_groups g WHERE g.id=m.pay_group_id AND g.company_id=m.company_id) FROM hr_payroll_account_mappings m,scope WHERE scope.is_base AND m.company_id=scope.company_id
 UNION ALL SELECT 'final-pay',p.id,p.version,p.status,to_jsonb(p),c.id IS NULL OR t.id IS NULL OR t.case_id IS DISTINCT FROM p.case_id OR t.company_id IS DISTINCT FROM p.company_id OR e.id IS NULL OR c.engagement_id IS DISTINCT FROM p.engagement_id OR r.id IS NULL OR r.company_id IS DISTINCT FROM p.company_id OR (p.status='prepared' AND r.status<>'posted') OR (p.status='cancelled' AND NOT EXISTS(SELECT 1 FROM final_pay_terminals fp WHERE fp.preparation_id=p.id AND fp.company_id=p.company_id AND fp.version=p.version)) FROM hr_payroll_final_pay_preparations p CROSS JOIN scope LEFT JOIN cases c ON c.id=p.case_id LEFT JOIN people_preparation_tasks t ON t.id=p.task_id LEFT JOIN engagements e ON e.id=p.engagement_id LEFT JOIN hr_payroll_runs r ON r.id=p.run_id WHERE scope.is_base AND p.company_id=scope.company_id
 UNION ALL SELECT 'unscoped-compensation',c.id,c.version,c.status,to_jsonb(c),true FROM hr_compensation_revisions c,scope WHERE scope.is_base AND NOT EXISTS(SELECT 1 FROM hr_engagements e WHERE e.id=c.engagement_id)
 UNION ALL SELECT 'unscoped-payroll-item',i.id,1,i.status,to_jsonb(i),true FROM hr_payroll_items i,scope WHERE scope.is_base AND NOT EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=i.run_id)
 UNION ALL SELECT 'unscoped-payroll-posting',p.id,1,'ambiguous',to_jsonb(p),true FROM hr_payroll_postings p,scope WHERE scope.is_base AND NOT EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=p.run_id)
 UNION ALL SELECT 'evaluation',a.id,a.version,a.status,to_jsonb(a),a.status<>'confirmed' OR e.id IS NULL FROM hr_evaluation_assessments a CROSS JOIN scope LEFT JOIN engagements e ON e.id=a.engagement_id WHERE scope.is_base AND a.company_id=scope.company_id
 UNION ALL SELECT 'evaluation-pay-draft',d.id,d.version,d.status,to_jsonb(d),d.status='draft' OR NOT EXISTS(SELECT 1 FROM hr_compensation_revisions c JOIN engagements e ON e.id=c.engagement_id WHERE c.id=d.compensation_id AND c.status IN ('approved','superseded')) FROM hr_evaluation_compensation_drafts d,scope WHERE scope.is_base AND d.company_id=scope.company_id
 UNION ALL SELECT 'evaluation-job',j.id,j.attempts,j.state,to_jsonb(j),j.state IN ('queued','running') FROM hr_evaluation_ai_jobs j,scope WHERE scope.is_base AND j.company_id=scope.company_id
 ) SELECT kind,id,version,state,image,COALESCE(pending,true) pending FROM facts ORDER BY kind,id`).bind(input.companyId,input.packageId==='people',input.now).all<{kind:string;id:string;version:number;state:string;image:unknown;pending:boolean}>()).results;
 // An ended assignment can still have missed original accrual periods. Read
 // the raw schedule and immutable keys, without rollout or purchase filtering.
 const today=new Date(input.now).toISOString().slice(0,10),due=(await database.prepare(`SELECT a.company_id,a.engagement_id,r.leave_type_id,a.id assignment_id,p.id policy_version_id,r.grant_kind,r.grant_units,r.accrual_units,r.accrual_period,r.carryover_limit,
 greatest(a.effective_from,p.effective_from,e.start_date) first_day,least(COALESCE(a.effective_to,?),COALESCE(p.effective_to,?),COALESCE(e.end_date,?),?) last_day
 FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id JOIN leave_policy_rules r ON r.policy_version_id=p.id JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id JOIN leave_types t ON t.id=r.leave_type_id AND t.company_id=a.company_id
 WHERE a.company_id=? AND p.status IN ('active','retired') AND t.tracked AND a.effective_from<=? AND CASE WHEN e.worker_type='contractor' THEN r.contractor_eligible ELSE r.employee_eligible END ORDER BY a.id,r.leave_type_id`).bind(today,today,today,today,input.companyId,today).all<{company_id:string;engagement_id:string;leave_type_id:string;assignment_id:string;policy_version_id:string;grant_kind:string;grant_units:number;accrual_units:number;accrual_period:string|null;carryover_limit:number|null;first_day:string;last_day:string}>()).results,
 ledger=(await database.prepare("SELECT id,engagement_id,leave_type_id,kind,units,effective_date,entitlement_year,command_key FROM leave_balance_entries WHERE company_id=? ORDER BY id").bind(input.companyId).all<{id:string;engagement_id:string;leave_type_id:string;kind:string;units:number;effective_date:string;entitlement_year:number;command_key:string}>()).results;
 for(const item of due){
  if(item.first_day>item.last_day)continue;
  const expected=(kind:string,units:number,date:string)=>{if(!units)return;const key=`leave:${item.grant_kind==='period'?item.assignment_id:item.engagement_id}:${kind}:${kind==='grant'||kind==='carryover'?date.slice(0,4):date}`,actual=ledger.find(row=>row.command_key===key&&row.kind===kind&&row.leave_type_id===item.leave_type_id),pending=!actual||actual.engagement_id!==item.engagement_id||actual.effective_date!==date||actual.entitlement_year!==Number(date.slice(0,4));facts.push({kind:'leave-accrual-due',id:`${key}:${item.leave_type_id}`,version:1,state:actual?'issued':'due',image:{source:item,key,kind,units,date,actual},pending});};
  if(item.grant_kind==='period')expected('grant',item.grant_units,item.first_day);
  for(let year=Number(item.first_day.slice(0,4));year<=Number(item.last_day.slice(0,4));year++){
   const yearStart=`${year}-01-01`,first=yearStart<item.first_day?item.first_day:yearStart;
   if(item.grant_kind==='annual')expected('grant',item.grant_units,first);
   if(yearStart>=item.first_day&&item.carryover_limit!==null){const previous=ledger.filter(row=>row.engagement_id===item.engagement_id&&row.leave_type_id===item.leave_type_id&&row.entitlement_year===year-1).reduce((sum,row)=>sum+Number(row.units),0);expected('carryover',Math.min(item.carryover_limit,Math.max(0,previous)),yearStart);}
   if(item.grant_kind==='accrual')for(let month=1;month<=12;month++){const date=`${year}-${String(month).padStart(2,'0')}-01`;if(date>=item.first_day&&date<=item.last_day&&(item.accrual_period==='monthly'||item.accrual_period==='quarterly'&&[1,4,7,10].includes(month)||item.accrual_period==='annual'&&month===1))expected('accrual',item.accrual_units,date);}
  }
 }
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"people_payroll",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
