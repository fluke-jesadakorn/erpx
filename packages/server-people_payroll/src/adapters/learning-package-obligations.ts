import {learningWorkerSourceRows,learningWorkerMarkedRows,learningWorkerPlacementRows,learningWorkerPlacementMarkedRows,projectWorkerPlacementTerminalRows} from '@erpx/server-people_payroll/adapters/project-worker-startup-source-sql';
import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {assertLearningPackageObligationSources,learningPackageSettlementSources as learning} from "@erpx/server-learning/public/package-obligations";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockLearningPeoplePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await database.prepare("LOCK TABLE hr_people,hr_engagements,people_lifecycle_cases,command_receipts,audit_events,outbox_events,iam_audit IN SHARE MODE").run();
 sources.set(database,companyId);
}
/** The issued certificate delivers the minimal learner reference. It does not
 * require buying People, starting an election or consuming a future benefit. */
export async function learningPeoplePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);
 if(sources.get(database)!==input.companyId)throw Error("LEARNING_PEOPLE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
 assertLearningPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="people_payroll"||input.packageId!=="learn"||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)
  throw Error("LEARNING_PEOPLE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id),
 assignments AS MATERIALIZED(SELECT a.* FROM (${learning.assignments}) a,scope WHERE a.company_id=scope.company_id),
 handoffs AS MATERIALIZED(SELECT h.* FROM (${learning.handoffs}) h,scope WHERE h.company_id=scope.company_id),
 certificates AS MATERIALIZED(SELECT c.* FROM (${learning.certificates}) c,scope WHERE c.company_id=scope.company_id)
 SELECT 'learner-handoff' kind,h.id,h.source_assignment_version version,
 a.id IS NULL OR NOT a.source_valid OR a.status NOT IN ('completed','waived') OR NOT a.terminal_proven
 OR person.id IS NULL OR person.user_id IS DISTINCT FROM a.learner_id
 OR h.source_assignment_version IS DISTINCT FROM a.event_version OR h.eligibility_code IS DISTINCT FROM a.required_for pending
 FROM handoffs h LEFT JOIN assignments a ON a.id=h.assignment_id
 LEFT JOIN hr_people person ON person.id=h.person_id AND person.company_id=h.company_id
 UNION ALL SELECT 'learner-certificate',c.id,c.source_assignment_version,
 h.id IS NULL OR a.id IS NULL OR person.id IS NULL OR person.user_id IS DISTINCT FROM a.learner_id
 OR c.person_id IS DISTINCT FROM h.person_id OR c.assignment_id IS DISTINCT FROM h.assignment_id
 OR c.source_assignment_version IS DISTINCT FROM h.source_assignment_version
 OR c.eligibility_code IS DISTINCT FROM h.eligibility_code OR c.completion_kind IS DISTINCT FROM a.status
 FROM certificates c LEFT JOIN handoffs h ON h.id=c.handoff_id LEFT JOIN assignments a ON a.id=c.assignment_id
 LEFT JOIN hr_people person ON person.id=c.person_id AND person.company_id=c.company_id

 UNION ALL SELECT 'worker-starter',marked.person_id,0,NOT EXISTS(SELECT 1 FROM (${learningWorkerSourceRows}) source WHERE source.person_id=marked.person_id AND source.company_id=marked.company_id)
 FROM (${learningWorkerMarkedRows}) marked CROSS JOIN scope WHERE marked.company_id=scope.company_id
 UNION ALL SELECT 'worker-placement',c.id,c.version,source.id IS NULL OR NOT EXISTS(SELECT 1 FROM (${projectWorkerPlacementTerminalRows}) terminal WHERE terminal.id=c.id AND terminal.company_id=c.company_id)
 FROM people_lifecycle_cases c CROSS JOIN scope JOIN (${learningWorkerPlacementMarkedRows}) marked ON marked.id=c.id AND marked.company_id=c.company_id LEFT JOIN (${learningWorkerPlacementRows}) source ON source.id=c.id AND source.company_id=c.company_id WHERE c.company_id=scope.company_id
 ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;version:number;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"people_payroll",observedAt:input.now,
  revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts,workerSources:(await database.prepare(`SELECT c.id,c.version,c.status,to_jsonb(c)::text image,source.proof FROM people_lifecycle_cases c JOIN (${learningWorkerPlacementMarkedRows}) marked ON marked.id=c.id AND marked.company_id=c.company_id LEFT JOIN (${learningWorkerPlacementRows}) source ON source.id=c.id AND source.company_id=c.company_id WHERE c.company_id=? ORDER BY c.id`).bind(input.companyId).all()).results})).digest("hex"),
  outstanding:facts.filter(fact=>fact.pending).length};
}
