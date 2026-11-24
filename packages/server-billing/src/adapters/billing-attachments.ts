import "server-only";

import { createHash,randomUUID } from "node:crypto";
import { join } from "node:path";
import type { RuntimeEnv } from "@erpx/server-runtime/public/runtime/env";
import type { FileStore } from "@erpx/server-runtime/public/runtime/types";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { LocalFileStore } from "@erpx/server-runtime/public/runtime/filesystem";
import { MinioFileStore } from "@erpx/server-runtime/public/runtime/minio";
import { attachmentUploadsFrozen } from "@erpx/server-files/public/attachments/feature";
import { detectedAttachmentMediaType } from "@erpx/server-files/public/attachments/preview";
import { scanAttachmentBytes } from "@erpx/server-files/public/attachments/scanner";

export function billingEvidenceStore(env: RuntimeEnv): FileStore {
  const local = new LocalFileStore(join(env.ERPX_DATA_DIR, "files", "platform-billing"));
  return process.env.ERPX_MINIO_ENDPOINT && process.env.ERPX_MINIO_BUCKET
    ? new MinioFileStore(process.env.ERPX_MINIO_BUCKET, `environments/${env.ERPX_ENVIRONMENT}/platform-billing`, process.env.ERPX_MINIO_ENDPOINT, process.env.ERPX_MINIO_ACCESS_KEY ?? "", process.env.ERPX_MINIO_SECRET_KEY ?? "", process.env.ERPX_MINIO_REGION ?? "us-east-1", undefined, local)
    : local;
}

import {createBillingEvidence,type BillingEvidenceRepository} from '@erpx/server-billing/application/evidence';
import {nativeBillingOwnerQuery,nativeBillingOwnerBind} from '@erpx/server-billing/adapters/native-billing-owner-write';
import type {BillingEvidenceSource,BillingScanJob} from '@erpx/server-billing/domain/evidence';
import {billingStoredLedgerSql} from './billing-read-tail-r28-sql';
import {billingPointReadSql} from './billing-read-tail-r29-sql';
export {billingEvidenceReconciliationReady} from '@erpx/server-billing/domain/evidence';
/** r17 read tail: byte-identical pending-scan count prefix shared by the readiness
 * totals and the scan-queue count. Each caller keeps its own suffix and result type. */
const BILLING_PENDING_SCAN_COUNT_SQL = "SELECT count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint pending";
function nativeEvidence(env:RuntimeEnv){const db=env.CONTROL_DB as PostgresDatabase,store=billingEvidenceStore(env),scannerEnvironment={...process.env,ERPX_ATTACHMENTS_V2:'true'};const repository:BillingEvidenceRepository={
readinessCounts:async()=>(await db.pool.query(`${BILLING_PENDING_SCAN_COUNT_SQL},count(*) FILTER (WHERE source_state='malware_rejected')::bigint sealed_malware FROM billing_evidence`)).rows[0],
latestFinalReconciliation:async()=>(await db.pool.query(billingPointReadSql('ready,created_at','billing_evidence_reconciliation_runs',"mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1"))).rows[0],
sources:async()=>(await db.pool.query<BillingEvidenceSource>('SELECT id,holding_id,object_key,size,sha256,source_state FROM billing_evidence ORDER BY id')).rows,
sourceLedger:async row=>(await db.pool.query(billingStoredLedgerSql('bytes',"evidence_id=$1 AND holding_id=$2 AND object_key=$3 AND state='stored'"),[row.id,row.holding_id,row.object_key])).rows[0],
storedTotals:async()=>(await db.pool.query(billingStoredLedgerSql(`COALESCE(sum(bytes),0)::bigint stored_bytes,(SELECT count(*)::bigint FROM (${billingStoredLedgerSql('object_key',"state='stored' GROUP BY object_key HAVING count(*)>1")}) duplicate_objects) AS duplicate_objects`,"state='stored'"))).rows[0],
pendingScans:async()=>(await db.pool.query(`${BILLING_PENDING_SCAN_COUNT_SQL} FROM billing_evidence`)).rows[0],
recordReconciliation:({id,input,uploadFrozen,ready,report,pending,now})=>nativeBillingOwnerQuery(db.pool, 'reconciliation_run', [id,input.mode,uploadFrozen,input.verifyChecksums,input.scanner.engine,input.scanner.definitionUpdatedAt,ready,JSON.stringify({scanPending:pending,...report}),now]),
claimScan:async(now,lease)=>(await nativeBillingOwnerQuery(db.pool, 'scan_claim', [now,now+60_000,lease])).rows[0],
scanSource:async id=>(await nativeBillingOwnerQuery(db.pool, 'scan_source', [id])).rows[0] as BillingEvidenceSource | undefined,
markScanning:id=>nativeBillingOwnerQuery(db.pool, 'scan_mark_scanning', [id]),
completeScan:({job,source,lease,result,detected,completed,eventId})=>db.transaction(async tx=>tx.batch(await Promise.all(result.clean?[
  nativeBillingOwnerBind(tx,'scan_clean_attempt',[result.engine,result.definitionUpdatedAt,completed,job.id,lease]),
  nativeBillingOwnerBind(tx,'scan_clean_evidence',[detected,completed,result.definitionUpdatedAt,source.id]),
  nativeBillingOwnerBind(tx,'scan_clean_event',[eventId,source.id,source.holding_id,JSON.stringify({engine:result.engine,definitionUpdatedAt:result.definitionUpdatedAt}),completed])
]:[
  nativeBillingOwnerBind(tx,'scan_infected_attempt',[result.engine,result.signature,result.definitionUpdatedAt,completed,job.id,lease]),
  nativeBillingOwnerBind(tx,'scan_infected_evidence',[detected,completed,result.definitionUpdatedAt,source.id]),
  nativeBillingOwnerBind(tx,'scan_infected_event',[eventId,source.id,source.holding_id,JSON.stringify({signature:result.signature,engine:result.engine}),completed])
]))),
retryScan:({job,source,lease,code,completed,eventId,retryId,retryAt})=>db.transaction(async tx=>{const statements=await Promise.all([
  nativeBillingOwnerBind(tx,'scan_error_attempt',[code,completed,job.id,lease]),
  nativeBillingOwnerBind(tx,'scan_error_evidence',[source.id]),
  nativeBillingOwnerBind(tx,'scan_error_event',[eventId,source.id,source.holding_id,JSON.stringify({attempt:Number(job.attempt),code}),completed])
]);if(retryId)statements.push(await nativeBillingOwnerBind(tx,'scan_retry_attempt',[retryId,source.id,Number(job.attempt)+1,retryAt]));return tx.batch(statements);})
};return createBillingEvidence({repository,uploadsFrozen:attachmentUploadsFrozen,now:Date.now,id:randomUUID,objectHead:key=>store.head(key),objectBytes:async key=>{const object=await store.get(key);return object?new Uint8Array(await object.arrayBuffer()):null},listObjects:cursor=>store.list({cursor,limit:1000}),hash:bytes=>createHash('sha256').update(bytes).digest('hex'),detect:detectedAttachmentMediaType,scan:bytes=>scanAttachmentBytes(bytes,scannerEnvironment)})}
export const billingEvidenceReadiness=(env:RuntimeEnv)=>nativeEvidence(env).billingEvidenceReadiness();
export const reconcileBillingEvidenceStorage=(env:RuntimeEnv,options:{verifyChecksums?:boolean}={})=>nativeEvidence(env).reconcileBillingEvidenceStorage(options);
export const recordBillingEvidenceReconciliation=(env:RuntimeEnv,input:Parameters<ReturnType<typeof createBillingEvidence>['recordBillingEvidenceReconciliation']>[0])=>nativeEvidence(env).recordBillingEvidenceReconciliation(input);
export const drainBillingEvidenceScans=(env:RuntimeEnv,limit=25,options:{offlineMigration?:boolean}={})=>nativeEvidence(env).drainBillingEvidenceScans(limit,options);
