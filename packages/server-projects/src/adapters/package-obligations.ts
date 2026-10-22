import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {projectTaskSettlementSources as operations,lockProjectTaskSettlementSources} from "@erpx/server-operations/public/project-package-obligation-sources";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockProjectPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await database.prepare(`LOCK TABLE staff_projects,staff_project_members,staff_project_milestones,staff_project_milestone_events,
 staff_project_milestone_dependencies,staff_project_task_packages,staff_project_work_logs,staff_project_work_log_events,
 staff_project_cost_allocations,staff_project_cost_allocation_events,staff_project_package_actuals,staff_project_package_events,
 staff_project_handoffs,command_receipts IN SHARE MODE`).run();
 await lockProjectTaskSettlementSources(database);sources.set(database,companyId);
}
export function assertProjectPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){
 owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("PROJECT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
}
export const projectPackageSettlementSources={
 projects:"SELECT * FROM staff_projects",members:"SELECT * FROM staff_project_members",milestones:"SELECT * FROM staff_project_milestones",
 workLogs:"SELECT * FROM staff_project_work_logs",costs:"SELECT * FROM staff_project_cost_allocations",actuals:"SELECT * FROM staff_project_package_actuals",handoffs:"SELECT * FROM staff_project_handoffs",
} as const;
export async function projectPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertProjectPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="projects"||!["projects","projects-costs"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("PROJECT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean is_base),
 projects AS MATERIALIZED(SELECT p.* FROM staff_projects p,scope WHERE p.company_id=scope.company_id),
 accepted AS MATERIALIZED(SELECT h.* FROM staff_project_handoffs h JOIN projects p ON p.id=h.project_id AND p.company_id=h.company_id
 WHERE h.status='accepted' AND h.prepared_by<>h.decided_by AND EXISTS
 (SELECT 1 FROM staff_project_package_events event JOIN command_receipts command ON event.id='project-package-event-'||command.command_id
 WHERE event.company_id=h.company_id AND event.project_id=h.project_id AND event.action='accept-handoff'
 AND event.snapshot_json->>'handoffId'=h.id AND event.snapshot_json->>'snapshotSha256'=h.snapshot_sha256
 AND event.actor_id=h.decided_by AND command.actor_id=h.decided_by AND command.operation='project.structure.accept-handoff'
 AND command.record_id=h.project_id AND command.authority_json::jsonb->>'companyId'=h.company_id
 AND command.result_json::jsonb->>'handoffId'=h.id AND command.result_json::jsonb->>'snapshotSha256'=h.snapshot_sha256
 AND command.result_json::jsonb->>'status'='completed' AND (command.result_json::jsonb->>'version')::bigint<=p.version))
 SELECT 'project' kind,p.id,p.version,p.lifecycle_status state,
 p.lifecycle_status<>'completed' OR NOT EXISTS(SELECT 1 FROM accepted h WHERE h.project_id=p.id) pending FROM projects p,scope WHERE scope.is_base
 UNION ALL SELECT 'milestone',m.id,m.version,m.state,
 p.id IS NULL OR m.state NOT IN ('completed','cancelled') OR NOT EXISTS
 (SELECT 1 FROM staff_project_milestone_events event WHERE event.milestone_id=m.id AND event.milestone_version=m.version AND event.action=m.state)
 FROM staff_project_milestones m CROSS JOIN scope LEFT JOIN projects p ON p.id=m.project_id WHERE scope.is_base AND m.company_id=scope.company_id
 UNION ALL SELECT 'task',t.id,t.version,t.state,p.id IS NULL OR t.state NOT IN ('completed','cancelled')
 OR (t.state='completed' AND NOT EXISTS(SELECT 1 FROM staff_project_task_packages binding JOIN staff_project_milestones m ON m.id=binding.milestone_id
 WHERE binding.task_id=t.id AND binding.company_id=t.company_id AND binding.project_id=t.project_id AND m.project_id=t.project_id AND m.company_id=t.company_id))
 FROM (${operations.tasks}) t CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id WHERE scope.is_base AND t.company_id=scope.company_id AND t.project_id IS NOT NULL
 UNION ALL SELECT 'routine-template',t.id,t.version,CASE WHEN t.published=1 THEN 'published' ELSE 'draft' END,p.id IS NULL OR t.published=1
 FROM (${operations.templates}) t CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id WHERE scope.is_base AND t.company_id=scope.company_id AND t.project_id IS NOT NULL
 UNION ALL SELECT 'routine-schedule',s.id,s.version,s.state,p.id IS NULL OR s.state='active' OR s.company_id IS DISTINCT FROM t.company_id
 FROM (${operations.schedules}) s JOIN (${operations.templates}) t ON t.id=s.template_id CROSS JOIN scope LEFT JOIN projects p ON p.id=t.project_id
 WHERE scope.is_base AND s.company_id=scope.company_id AND t.project_id IS NOT NULL
 UNION ALL SELECT 'work-log',l.id,l.version,l.state,p.id IS NULL OR l.state='submitted' OR NOT EXISTS
 (SELECT 1 FROM staff_project_work_log_events event JOIN command_receipts command ON command.record_id=l.id
 AND command.operation='project.actual-time.'||CASE WHEN l.state='approved' THEN 'approve' ELSE 'reject' END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=l.company_id
 AND command.result_json::jsonb->>'id'=l.id AND command.result_json::jsonb->>'version'=l.version::text
 AND command.result_json::jsonb->>'state'=l.state WHERE event.work_log_id=l.id AND event.work_log_version=l.version AND event.action=l.state)
 OR (l.state='approved' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='work_log' AND actual.source_id=l.id
 AND actual.source_version=l.version AND actual.company_id=l.company_id AND actual.project_id=l.project_id AND actual.minutes=l.minutes))
 FROM staff_project_work_logs l CROSS JOIN scope LEFT JOIN projects p ON p.id=l.project_id WHERE scope.is_base AND l.company_id=scope.company_id
 UNION ALL SELECT 'cost-allocation',c.id,c.version,c.state,p.id IS NULL OR c.state='draft' OR NOT EXISTS
 (SELECT 1 FROM staff_project_cost_allocation_events event JOIN command_receipts command ON command.record_id=c.id
 AND command.operation='project.cost-allocation.'||CASE WHEN c.state='approved' THEN 'approve' ELSE 'reject' END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=c.company_id
 AND command.result_json::jsonb->>'id'=c.id AND command.result_json::jsonb->>'version'=c.version::text
 AND command.result_json::jsonb->>'state'=c.state WHERE event.allocation_id=c.id AND event.allocation_version=c.version AND event.action=c.state)
 OR (scope.is_base AND c.state='approved' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals actual WHERE actual.source_kind='cost_allocation'
 AND actual.source_id=c.id AND actual.source_version=c.version AND actual.company_id=c.company_id AND actual.project_id=c.project_id AND actual.amount_minor=c.amount_minor))
 FROM staff_project_cost_allocations c CROSS JOIN scope LEFT JOIN projects p ON p.id=c.project_id WHERE c.company_id=scope.company_id
 UNION ALL SELECT 'package-actual',a.id,a.source_version,a.source_kind,p.id IS NULL OR m.id IS NULL OR m.project_id<>a.project_id OR m.company_id<>a.company_id
 OR (a.source_kind='work_log' AND NOT EXISTS(SELECT 1 FROM staff_project_work_logs l WHERE l.id=a.source_id AND l.project_id=a.project_id AND l.company_id=a.company_id AND l.version=a.source_version AND l.state='approved' AND l.minutes=a.minutes))
 OR (a.source_kind='cost_allocation' AND NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c WHERE c.id=a.source_id AND c.project_id=a.project_id AND c.company_id=a.company_id AND c.version=a.source_version AND c.state='approved' AND c.amount_minor=a.amount_minor))
 FROM staff_project_package_actuals a CROSS JOIN scope LEFT JOIN projects p ON p.id=a.project_id LEFT JOIN staff_project_milestones m ON m.id=a.milestone_id
 WHERE a.company_id=scope.company_id AND (scope.is_base OR a.source_kind='cost_allocation')
 UNION ALL SELECT 'handoff',h.id,h.version,h.status,p.id IS NULL OR h.status='pending' OR (h.status='accepted' AND NOT EXISTS(SELECT 1 FROM accepted proof WHERE proof.id=h.id))
 FROM staff_project_handoffs h CROSS JOIN scope LEFT JOIN projects p ON p.id=h.project_id WHERE scope.is_base AND h.company_id=scope.company_id
 ORDER BY kind,id`).bind(input.companyId,input.packageId==='projects').all<{kind:string;id:string;version:number;state:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"projects",observedAt:input.now,
 revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
