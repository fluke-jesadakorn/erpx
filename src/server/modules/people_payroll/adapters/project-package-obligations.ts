import {projectWorkerSourceRows,projectWorkerMarkedRows,projectWorkerPlacementRows,projectWorkerPlacementMarkedRows,projectWorkerPlacementTerminalRows} from './project-worker-startup-source-sql';
import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {assertProjectPackageObligationSources,projectPackageSettlementSources as projects} from "../../projects/public/package-obligations";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockProjectPeoplePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE hr_people,hr_engagements,people_lifecycle_cases,command_receipts,audit_events,outbox_events,iam_audit IN SHARE MODE").run();sources.set(database,companyId);}
/** References supporting ongoing Project work need a real Company worker.
 * Closed delivery retains the original identifiers even after employment ends. */
export async function projectPeoplePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("PROJECT_PEOPLE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertProjectPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="people_payroll"||!["projects","projects-costs"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("PROJECT_PEOPLE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean is_base),
 projects AS MATERIALIZED(SELECT p.* FROM (${projects.projects}) p,scope WHERE p.company_id=scope.company_id),
 refs AS(SELECT 'project-owner' kind,p.id,p.owner_user_id user_id,p.company_id,p.lifecycle_status<>'completed' pending_reference FROM projects p,scope WHERE scope.is_base
 UNION ALL SELECT 'work-log',l.id,l.user_id,l.company_id,l.state='submitted' FROM (${projects.workLogs}) l,scope WHERE scope.is_base AND l.company_id=scope.company_id
 UNION ALL SELECT 'cost-allocation',c.id,c.created_by,c.company_id,c.state='draft' FROM (${projects.costs}) c,scope WHERE c.company_id=scope.company_id
 UNION ALL SELECT 'handoff',h.id,h.recipient_id,h.company_id,h.status='pending' FROM (${projects.handoffs}) h,scope WHERE scope.is_base AND h.company_id=scope.company_id)
 SELECT refs.kind,refs.id,refs.pending_reference,
 refs.pending_reference AND NOT EXISTS(SELECT 1 FROM hr_people person WHERE person.company_id=refs.company_id AND (person.user_id=refs.user_id OR person.id=refs.user_id)) pending
 FROM refs
 UNION ALL SELECT 'worker-starter',marked.person_id,false,NOT EXISTS(SELECT 1 FROM (${projectWorkerSourceRows}) source WHERE source.person_id=marked.person_id AND source.company_id=marked.company_id)
 FROM (${projectWorkerMarkedRows}) marked CROSS JOIN scope WHERE scope.is_base AND marked.company_id=scope.company_id
 UNION ALL SELECT 'worker-placement',c.id,c.status NOT IN ('completed','cancelled'),source.id IS NULL OR NOT EXISTS(SELECT 1 FROM (${projectWorkerPlacementTerminalRows}) terminal WHERE terminal.id=c.id AND terminal.company_id=c.company_id)
 FROM people_lifecycle_cases c CROSS JOIN scope JOIN (${projectWorkerPlacementMarkedRows}) marked ON marked.id=c.id AND marked.company_id=c.company_id LEFT JOIN (${projectWorkerPlacementRows}) source ON source.id=c.id AND source.company_id=c.company_id WHERE scope.is_base AND c.company_id=scope.company_id
 ORDER BY kind,id`).bind(input.companyId,input.packageId==='projects').all<{kind:string;id:string;pending_reference:boolean;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"people_payroll",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts,workerSources:(await database.prepare(`SELECT c.id,c.version,c.status,to_jsonb(c)::text image,source.proof FROM people_lifecycle_cases c JOIN (${projectWorkerPlacementMarkedRows}) marked ON marked.id=c.id AND marked.company_id=c.company_id LEFT JOIN (${projectWorkerPlacementRows}) source ON source.id=c.id AND source.company_id=c.company_id WHERE c.company_id=? ORDER BY c.id`).bind(input.companyId).all()).results})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
