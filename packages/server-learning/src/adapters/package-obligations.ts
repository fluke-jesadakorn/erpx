import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockLearningPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await database.prepare(`LOCK TABLE learning_courses,learning_course_events,learning_assignments,learning_assignment_events,
 learning_eligibility_handoffs,learning_certificates,learning_certificate_revocations,command_receipts IN SHARE MODE`).run();
 sources.set(database,companyId);
}
export function assertLearningPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){
 owner.assertActiveTransaction(database);
 if(sources.get(database)!==companyId)throw Error("LEARNING_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
}
/** Raw lineage remains visible even when the source is broken, inactive or
 * absent from the current user's Work. These are owner facts, never a UI list. */
export const learningPackageSettlementSources={
 assignments:`SELECT a.*,c.company_id course_company_id,c.required_for,
 event.id event_id,event.version event_version,event.action event_action,event.status event_status,
 COALESCE(c.id IS NOT NULL AND c.company_id=a.company_id AND a.course_version<=c.version,false) source_valid,
 COALESCE(event.status=a.status AND event.version<=a.version AND command.command_id IS NOT NULL
 AND command.operation='learning.assignment.'||event.action AND command.record_id=a.id
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=a.company_id
 AND command.result_json::jsonb->>'id'=a.id AND command.result_json::jsonb->>'version'=event.version::text
 AND command.result_json::jsonb->>'status'=a.status,false) terminal_proven
 FROM learning_assignments a LEFT JOIN learning_courses c ON c.id=a.course_id
 LEFT JOIN LATERAL(SELECT e.* FROM learning_assignment_events e WHERE e.assignment_id=a.id ORDER BY e.version DESC LIMIT 1) event ON TRUE
 LEFT JOIN command_receipts command ON event.id='learning-assignment-event-'||command.command_id`,
 handoffs:"SELECT * FROM learning_eligibility_handoffs",
 certificates:"SELECT * FROM learning_certificates",
} as const;

export async function learningPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertLearningPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="learning"||input.packageId!=="learn"||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)
  throw Error("LEARNING_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id),
 assignments AS MATERIALIZED(SELECT a.* FROM (${learningPackageSettlementSources.assignments}) a,scope WHERE a.company_id=scope.company_id),
 handoffs AS MATERIALIZED(SELECT h.* FROM learning_eligibility_handoffs h,scope WHERE h.company_id=scope.company_id),
 certificates AS MATERIALIZED(SELECT c.* FROM learning_certificates c,scope WHERE c.company_id=scope.company_id)
 SELECT 'course' kind,c.id,c.version,c.status state,
 c.status='draft' OR event.id IS NULL OR event.status<>c.status OR event.version>c.version
 OR c.reviewed_by IS NULL OR c.reviewed_by=c.created_by pending
 FROM learning_courses c CROSS JOIN scope LEFT JOIN LATERAL
 (SELECT e.id,e.version,e.status FROM learning_course_events e WHERE e.course_id=c.id ORDER BY e.version DESC LIMIT 1) event ON TRUE
 WHERE c.company_id=scope.company_id
 UNION ALL SELECT 'assignment',a.id,a.version,a.status,
 NOT a.source_valid OR a.status NOT IN ('completed','waived','cancelled') OR NOT a.terminal_proven
 OR (a.status IN ('completed','waived') AND a.required_for IS NOT NULL AND NOT EXISTS
 (SELECT 1 FROM handoffs h JOIN certificates c ON c.handoff_id=h.id AND c.company_id=h.company_id
 WHERE h.assignment_id=a.id AND h.source_assignment_version=a.event_version AND h.eligibility_code=a.required_for
 AND c.assignment_id=a.id AND c.source_assignment_version=h.source_assignment_version
 AND c.eligibility_code=h.eligibility_code AND c.person_id=h.person_id AND c.completion_kind=a.status))
 FROM assignments a
 UNION ALL SELECT 'handoff',h.id,h.source_assignment_version,h.status,
 a.id IS NULL OR NOT a.source_valid OR NOT a.terminal_proven OR a.status NOT IN ('completed','waived')
 OR h.source_assignment_version<>a.event_version OR h.eligibility_code IS DISTINCT FROM a.required_for
 OR NOT EXISTS(SELECT 1 FROM certificates c WHERE c.handoff_id=h.id AND c.assignment_id=a.id
 AND c.source_assignment_version=h.source_assignment_version AND c.person_id=h.person_id
 AND c.eligibility_code=h.eligibility_code AND c.completion_kind=a.status)
 FROM handoffs h LEFT JOIN assignments a ON a.id=h.assignment_id
 UNION ALL SELECT 'certificate',c.id,c.source_assignment_version,c.completion_kind,
 h.id IS NULL OR a.id IS NULL OR NOT a.source_valid OR NOT a.terminal_proven
 OR c.assignment_id IS DISTINCT FROM h.assignment_id OR c.person_id IS DISTINCT FROM h.person_id
 OR c.source_assignment_version IS DISTINCT FROM h.source_assignment_version
 OR c.eligibility_code IS DISTINCT FROM h.eligibility_code OR c.completion_kind IS DISTINCT FROM a.status
 FROM certificates c LEFT JOIN handoffs h ON h.id=c.handoff_id LEFT JOIN assignments a ON a.id=c.assignment_id
 UNION ALL SELECT 'revocation',r.id,0::bigint,'revoked',
 c.id IS NULL OR command.command_id IS NULL OR command.operation<>'learning.certificate.revoke'
 OR command.record_id IS DISTINCT FROM r.certificate_id OR command.actor_id IS DISTINCT FROM r.actor_id
 OR command.authority_json::jsonb->>'companyId' IS DISTINCT FROM r.company_id
 FROM learning_certificate_revocations r CROSS JOIN scope LEFT JOIN certificates c ON c.id=r.certificate_id
 LEFT JOIN command_receipts command ON command.command_id=r.command_id WHERE r.company_id=scope.company_id
 ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;version:number;state:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"learning",observedAt:input.now,
  revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),
  outstanding:facts.filter(fact=>fact.pending).length};
}
