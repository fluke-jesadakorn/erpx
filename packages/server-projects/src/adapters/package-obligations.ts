import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {projectTaskSettlementSources as operations,lockProjectTaskSettlementSources} from "@erpx/server-operations/public/project-package-obligation-sources";
import {notExistsRow,projectFactUnionArms} from './read-tail-r37';
import {scopeBindProjection} from './read-tail-r39';
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
/** Byte-identical fixed row-source prefix shared by the settlement table reads;
 * one definition keeps every source relation identical. */
const tableRowsSql=`SELECT * FROM `;
/** G4 / retire.projects.read-tail-r42: the Projects row source is the same closed
 * head with the Projects relation appended. Delegating keeps one byte literal for
 * the shared prefix; the rendered source stays byte-identical. No locks, native
 * statements or cross-owner text are emitted here. */
export const projectRowsSql=`${tableRowsSql}staff_projects`;
/** Byte-identical assignment-scope subquery delegate; the closed bind projection
 * lives in read-tail-r39 so the purchase and metric authority reads keep the
 * exact same emitted SQL and only the scope alias varies per caller. */
export const projectAssignmentScopeSql=(alias:string)=>scopeBindProjection('company-branch-department',alias);
/** Byte-identical package-actual proof guard shared by the work-log and
 * cost-allocation arms of projectPackageObligations. Only the source kind,
 * source alias, measure column and the two whitespace gaps vary; every source
 * relation, equality and bind order stays exactly as in the former inline
 * copies. The two gap parameters preserve the original newline position. */
export const projectPackageActualProof=(kind:string,alias:string,measure:string,kindGap:string,idGap:string)=>
 notExistsRow('staff_project_package_actuals','actual',
 `actual.source_kind='${kind}'${kindGap}AND actual.source_id=${alias}.id${idGap}AND actual.source_version=${alias}.version AND actual.company_id=${alias}.company_id AND actual.project_id=${alias}.project_id AND actual.${measure}=${alias}.${measure}`);
/** Byte-identical receipt-event proof arm shared by the work-log and
 * cost-allocation arms of projectPackageObligations. Only the event table, row
 * alias, operation prefix and the two FK columns vary; newlines, indentation
 * and bind order stay exactly as in the former inline copies. */
export const projectReceiptEventProof=(table:string,alias:string,op:string,fkId:string,fkVer:string)=>
  `(SELECT 1 FROM ${table} event JOIN command_receipts command ON command.record_id=${alias}.id\n AND command.operation='${op}'||CASE WHEN ${alias}.state='approved' THEN 'approve' ELSE 'reject' END\n AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=${alias}.company_id\n AND command.result_json::jsonb->>'id'=${alias}.id AND command.result_json::jsonb->>'version'=${alias}.version::text\n AND command.result_json::jsonb->>'state'=${alias}.state WHERE event.${fkId}=${alias}.id AND event.${fkVer}=${alias}.version AND event.action=${alias}.state)`;
/** Byte-identical package-actual source witness shared by the work-log and
 * cost-allocation arms of the package-actual fact read. Only the source
 * relation, row alias and measure column vary; no binds and no whitespace gaps. */
export const packageActualSourceProof=(relation:string,alias:string,measure:string)=>
  notExistsRow(relation,alias,
    `${alias}.id=a.source_id AND ${alias}.project_id=a.project_id AND ${alias}.company_id=a.company_id AND ${alias}.version=a.source_version AND ${alias}.state='approved' AND ${alias}.${measure}=a.${measure}`);
type EventFactArmArgs={kind:string;alias:string;state:string;table:string;op:string;fkId:string;fkVer:string;
  actualKind:string;measure:string;kindGap:string;idGap:string;scopeGuard:string;relation:string;whereScope:string};
/** Byte-identical closed work-log/cost-allocation UNION arm. The first emitted
 * line carries no leading space (the call site supplies it); every other gap,
 * alias, literal and whitespace position stays exactly as in the former pairs. */
export const eventFactArm=(a:EventFactArmArgs)=>
  `UNION ALL SELECT '${a.kind}',${a.alias}.id,${a.alias}.version,${a.alias}.state,p.id IS NULL OR ${a.alias}.state='${a.state}' OR NOT EXISTS
 ${projectReceiptEventProof(a.table,a.alias,a.op,a.fkId,a.fkVer)}
 OR (${a.scopeGuard}${a.alias}.state='approved' AND ${projectPackageActualProof(a.actualKind,a.alias,a.measure,a.kindGap,a.idGap)})
 FROM ${a.relation} ${a.alias} CROSS JOIN scope LEFT JOIN projects p ON p.id=${a.alias}.project_id WHERE ${a.whereScope}${a.alias}.company_id=scope.company_id`;
export const projectPackageSettlementSources={
 projects:projectRowsSql,members:`${tableRowsSql}staff_project_members`,milestones:`${tableRowsSql}staff_project_milestones`,
 workLogs:`${tableRowsSql}staff_project_work_logs`,costs:`${tableRowsSql}staff_project_cost_allocations`,actuals:`${tableRowsSql}staff_project_package_actuals`,handoffs:`${tableRowsSql}staff_project_handoffs`,
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
 ${projectFactUnionArms(operations)}
 ${eventFactArm({kind:'work-log',alias:'l',state:'submitted',table:'staff_project_work_log_events',op:'project.actual-time.',fkId:'work_log_id',fkVer:'work_log_version',actualKind:'work_log',measure:'minutes',kindGap:' ',idGap:'\n ',scopeGuard:'',relation:'staff_project_work_logs',whereScope:'scope.is_base AND '})}
 ${eventFactArm({kind:'cost-allocation',alias:'c',state:'draft',table:'staff_project_cost_allocation_events',op:'project.cost-allocation.',fkId:'allocation_id',fkVer:'allocation_version',actualKind:'cost_allocation',measure:'amount_minor',kindGap:'\n ',idGap:' ',scopeGuard:'scope.is_base AND ',relation:'staff_project_cost_allocations',whereScope:''})}
 UNION ALL SELECT 'package-actual',a.id,a.source_version,a.source_kind,p.id IS NULL OR m.id IS NULL OR m.project_id<>a.project_id OR m.company_id<>a.company_id
 OR (a.source_kind='work_log' AND ${packageActualSourceProof('staff_project_work_logs','l','minutes')})
 OR (a.source_kind='cost_allocation' AND ${packageActualSourceProof('staff_project_cost_allocations','c','amount_minor')})
 FROM staff_project_package_actuals a CROSS JOIN scope LEFT JOIN projects p ON p.id=a.project_id LEFT JOIN staff_project_milestones m ON m.id=a.milestone_id
 WHERE a.company_id=scope.company_id AND (scope.is_base OR a.source_kind='cost_allocation')
 UNION ALL SELECT 'handoff',h.id,h.version,h.status,p.id IS NULL OR h.status='pending' OR (h.status='accepted' AND NOT EXISTS(SELECT 1 FROM accepted proof WHERE proof.id=h.id))
 FROM staff_project_handoffs h CROSS JOIN scope LEFT JOIN projects p ON p.id=h.project_id WHERE scope.is_base AND h.company_id=scope.company_id
 ORDER BY kind,id`).bind(input.companyId,input.packageId==='projects').all<{kind:string;id:string;version:number;state:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"projects",observedAt:input.now,
 revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
