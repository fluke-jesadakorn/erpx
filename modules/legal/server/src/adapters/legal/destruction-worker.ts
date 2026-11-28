import {fileDestructionAuthority} from "@erpx/legal-server/adapters/file-destruction-ports";
import {withLegalRetentionExecutionUnit,authorizeLegalRetentionDeletion} from "@erpx/legal-server/adapters/retention-execution";
import "server-only";
import type {PostgresDatabase} from "@erpx/platform-runtime-server/public/runtime/postgres";
import type {SqlDatabase,FileStore} from "@erpx/platform-runtime-server/public/runtime/types";
import {lock} from "@erpx/legal-server/adapters/legal/access";
import {validateLegalDestructionSources,sealLegalDestructionSources} from "@erpx/legal-server/adapters/legal/destruction-sources";
import {createLegalDestructionWorker,type LegalDestructionWork} from "@erpx/legal-server/application/destruction-worker";
import { legalDestructionWorker } from '@erpx/collaboration-server/adapters/native-legal-destruction-worker';
import {legalHoldGuardPrefix,legalHoldGuardSuffix} from '../legal-hold-guard-sources';
import {legalHoldActiveSql} from '../legal-read-tail-r23-sources';
import {legalFilesAdvisoryLockSql} from '../legal-read-tail-r9-sources';
/** Byte-identical Legal destruction-by-id row-lock head (continued read-tail
 * family). One renderer for the two state-specific point reads; each caller
 * appends its fixed row-lock tail and keeps its own single bind. The state is a
 * trusted union, no request text reaches the head, and no SQL fallback is
 * introduced. */
const legalDestructionByIdLockSql = (projection: string, state: 'approved' | 'executing') =>
  `SELECT ${projection} FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='${state}'`;
function work(db:SqlDatabase,files:FileStore,companyId:string):LegalDestructionWork{const nw=legalDestructionWorker(db,companyId);return {
beginJournal:async ()=>{await db.prepare(legalFilesAdvisoryLockSql).run();},
approvedDestruction:(facts:{id:string;})=>db.prepare(legalDestructionByIdLockSql('x.*,i.company_id','approved')+' FOR UPDATE OF x,i').bind(facts.id).first<{
          id: string;
          item_id: string;
          company_id: string;
          policy_id: string;
          policy_version: number;
        }>(),
stillEligible:(facts:{item_id:string;policy_id:string;policy_version:number;now:number;})=>db.prepare("SELECT 1 FROM legal_retention_assignments a JOIN legal_retention_policies p ON p.id=a.policy_id JOIN legal_items i ON i.id=a.item_id WHERE a.item_id=? AND a.policy_id=? AND a.policy_version=? AND p.version=a.policy_version AND p.state='approved' AND a.eligible_at<=? AND i.state IN ('archived','closed','completed','expired','terminated') AND "+legalHoldGuardPrefix+"i.id"+legalHoldGuardSuffix).bind(facts.item_id,facts.policy_id,facts.policy_version,facts.now).first(),
blockDestruction:async (facts:{id:string;})=>{await (await nw.block(facts)).run();},
startExecution:async (facts:{now:number;id:string;})=>{await (await nw.start(facts)).run();},
recordStarted:async (facts:{identifier:string;fact1:string;now:number;item_id:string;})=>{await (await nw.startedEvent(facts)).run();},
beginExecution:async ()=>{await db.prepare(legalFilesAdvisoryLockSql).run();},
executingDestruction:(facts:{id:string;})=>db.prepare(legalDestructionByIdLockSql('x.*,i.company_id,i.version item_version','executing')+' FOR UPDATE OF x,i').bind(facts.id).first<{
          id: string;
          item_id: string;
          company_id: string;
          item_version: number;
          policy_id: string;
          policy_version: number;
          manifest: string;
        }>(),
held:(facts:{item_id:string;})=>db.prepare(legalHoldActiveSql('!=')).bind(facts.item_id).first(),
countAttempt:async (facts:{id:string;})=>{await (await nw.countAttempt(facts)).run();},
retainExecuting:async (facts:{id:string;})=>{await (await nw.retainExecuting(facts)).run();},
retainDisposal:async (facts:{id:string;id2:string;checksum:string;now:number;})=>{await (await nw.retainDisposal(facts)).run();},
eraseDocument:async (facts:{id:string;})=>{await (await nw.eraseDocument(facts)).run();},
retireProcessing:async (facts:{item_id:string;})=>{await (await nw.retireProcessing(facts)).run();},
eraseInspections:async (facts:{item_id:string;})=>{await (await nw.eraseInspections(facts)).run();},
retireAi:async (facts:{item_id:string;})=>{await (await nw.retireAi(facts)).run();},
revokeShares:async (facts:{now:number;item_id:string;})=>{await (await nw.revokeShares(facts)).run();},
completeDestruction:async (facts:{now:number;id:string;})=>{await (await nw.complete(facts)).run();},
recordCompleted:async (facts:{identifier:string;item_id:string;fact2:string;item_version:number;now:number;})=>{await (await nw.completedEvent(facts)).run();},
lockCompany:async id=>{await lock(db,id).run();},canDestroy:()=>Boolean(files.destroyPreserved),validate:(id,company,manifest)=>validateLegalDestructionSources(db,id,company,manifest),seal:(company,id,manifest)=>sealLegalDestructionSources(db,company,id,manifest),destroy:async(keys,id)=>{if(!files.destroyPreserved)throw new Error("LEGAL_DESTRUCTION_STORE_UNAVAILABLE");await authorizeLegalRetentionDeletion(db,id);return files.destroyPreserved(keys,{database:db,destructionId:id,owner:fileDestructionAuthority(db,id)});}
};}
export function drainLegalDestruction(db:PostgresDatabase,files:FileStore){let candidate:{id:string;company_id:string}|null=null;return createLegalDestructionWorker({clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},repository:{
candidate:async()=>{candidate=await db.prepare("SELECT x.id,i.company_id FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.state IN ('approved','executing') AND x.attempts<6 ORDER BY x.created_at LIMIT 1").first<{id:string;company_id:string}>();return candidate;},
requireReview:async (facts:{id:string;})=>{await (await legalDestructionWorker(db,candidate?.company_id??"").requireReview(facts)).run();},transaction:task=>{if(!candidate)throw Error("LEGAL_DESTRUCTION_CANDIDATE_REQUIRED");return withLegalRetentionExecutionUnit(db,candidate.company_id,candidate.id,tx=>task(work(tx,files,candidate!.company_id)));}}})();}
