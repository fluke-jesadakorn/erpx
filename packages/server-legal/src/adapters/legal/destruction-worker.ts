import {fileDestructionAuthority} from "@erpx/server-legal/adapters/file-destruction-ports";
import {withLegalRetentionExecutionUnit,authorizeLegalRetentionDeletion} from "@erpx/server-legal/adapters/retention-execution";
import "server-only";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase,FileStore} from "@erpx/server-runtime/public/runtime/types";
import {lock} from "@erpx/server-legal/adapters/legal/access";
import {validateLegalDestructionSources,sealLegalDestructionSources} from "@erpx/server-legal/adapters/legal/destruction-sources";
import {createLegalDestructionWorker,type LegalDestructionWork} from "@erpx/server-legal/application/destruction-worker";
function work(db:SqlDatabase,files:FileStore):LegalDestructionWork{return {
beginJournal:async ()=>{await db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run();},
approvedDestruction:(facts:{id:string;})=>db.prepare("SELECT x.*,i.company_id FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='approved' FOR UPDATE OF x,i").bind(facts.id).first<{
          id: string;
          item_id: string;
          company_id: string;
          policy_id: string;
          policy_version: number;
        }>(),
stillEligible:(facts:{item_id:string;policy_id:string;policy_version:number;now:number;})=>db.prepare("SELECT 1 FROM legal_retention_assignments a JOIN legal_retention_policies p ON p.id=a.policy_id JOIN legal_items i ON i.id=a.item_id WHERE a.item_id=? AND a.policy_id=? AND a.policy_version=? AND p.version=a.policy_version AND p.state='approved' AND a.eligible_at<=? AND i.state IN ('archived','closed','completed','expired','terminated') AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=i.id AND h.state!='released')").bind(facts.item_id,facts.policy_id,facts.policy_version,facts.now).first(),
blockDestruction:async (facts:{id:string;})=>{await db.prepare("UPDATE legal_destructions SET state='blocked',error='HOLD_OR_RETENTION_POLICY_CHANGED' WHERE id=?").bind(facts.id).run();},
startExecution:async (facts:{now:number;id:string;})=>{await db.prepare("UPDATE legal_destructions SET state='executing',started_at=? WHERE id=?").bind(facts.now,facts.id).run();},
recordStarted:async (facts:{identifier:string;fact1:string;now:number;item_id:string;})=>{await db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,'retention.started','system:legal',?,version,? FROM legal_items WHERE id=?").bind(facts.identifier,facts.fact1,facts.now,facts.item_id).run();},
beginExecution:async ()=>{await db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run();},
executingDestruction:(facts:{id:string;})=>db.prepare("SELECT x.*,i.company_id,i.version item_version FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='executing' FOR UPDATE OF x,i").bind(facts.id).first<{
          id: string;
          item_id: string;
          company_id: string;
          item_version: number;
          policy_id: string;
          policy_version: number;
          manifest: string;
        }>(),
held:(facts:{item_id:string;})=>db.prepare("SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=? AND h.state!='released'").bind(facts.item_id).first(),
countAttempt:async (facts:{id:string;})=>{await db.prepare("UPDATE legal_destructions SET attempts=attempts+1 WHERE id=?").bind(facts.id).run();},
retainExecuting:async (facts:{id:string;})=>{await db.prepare("UPDATE legal_destructions SET state='executing' WHERE id=?").bind(facts.id).run();},
retainDisposal:async (facts:{id:string;id2:string;checksum:string;now:number;})=>{await db.prepare("INSERT INTO legal_disposals VALUES(?,?,?,?) ON CONFLICT DO NOTHING").bind(facts.id,facts.id2,facts.checksum,facts.now).run();},
eraseDocument:async (facts:{id:string;})=>{await db.prepare("UPDATE legal_documents SET body='',object_key=NULL WHERE id=?").bind(facts.id).run();},
retireProcessing:async (facts:{item_id:string;})=>{await db.prepare("UPDATE legal_processing_jobs SET state='failed',result_json=NULL,error='CONTENT_DESTROYED',lease_token=NULL,lease_until=NULL WHERE item_id=?").bind(facts.item_id).run();},
eraseInspections:async (facts:{item_id:string;})=>{await db.prepare("DELETE FROM legal_document_inspections WHERE document_id IN (SELECT id FROM legal_documents WHERE item_id=?)").bind(facts.item_id).run();},
retireAi:async (facts:{item_id:string;})=>{await db.prepare("UPDATE legal_ai_proposals SET proposal=NULL,question='[disposed under retention policy]',state='disposed' WHERE item_id=?").bind(facts.item_id).run();},
revokeShares:async (facts:{now:number;item_id:string;})=>{await db.prepare("UPDATE legal_shares SET revoked_at=? WHERE item_id=? AND revoked_at IS NULL").bind(facts.now,facts.item_id).run();},
completeDestruction:async (facts:{now:number;id:string;})=>{await db.prepare("UPDATE legal_destructions SET state='completed',completed_at=?,error=NULL,version=version+1 WHERE id=?").bind(facts.now,facts.id).run();},
recordCompleted:async (facts:{identifier:string;item_id:string;fact2:string;item_version:number;now:number;})=>{await db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,'retention.destroyed','system:legal',?,?,?)").bind(facts.identifier,facts.item_id,facts.fact2,facts.item_version,facts.now).run();},
lockCompany:async id=>{await lock(db,id).run();},canDestroy:()=>Boolean(files.destroyPreserved),validate:(id,company,manifest)=>validateLegalDestructionSources(db,id,company,manifest),seal:(company,id,manifest)=>sealLegalDestructionSources(db,company,id,manifest),destroy:async(keys,id)=>{if(!files.destroyPreserved)throw new Error("LEGAL_DESTRUCTION_STORE_UNAVAILABLE");await authorizeLegalRetentionDeletion(db,id);return files.destroyPreserved(keys,{database:db,destructionId:id,owner:fileDestructionAuthority(db,id)});}
};}
export function drainLegalDestruction(db:PostgresDatabase,files:FileStore){let candidate:{id:string;company_id:string}|null=null;return createLegalDestructionWorker({clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},repository:{
candidate:async()=>{candidate=await db.prepare("SELECT x.id,i.company_id FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.state IN ('approved','executing') AND x.attempts<6 ORDER BY x.created_at LIMIT 1").first<{id:string;company_id:string}>();return candidate;},
requireReview:async (facts:{id:string;})=>{await db.prepare("UPDATE legal_destructions SET state=CASE WHEN started_at IS NULL THEN 'failed' ELSE 'executing' END,error='DESTRUCTION_REQUIRES_REVIEW',attempts=attempts+1 WHERE id=? AND state!='completed'").bind(facts.id).run();},transaction:task=>{if(!candidate)throw Error("LEGAL_DESTRUCTION_CANDIDATE_REQUIRED");return withLegalRetentionExecutionUnit(db,candidate.company_id,candidate.id,tx=>task(work(tx,files)));}}})();}
