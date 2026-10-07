import "server-only";

import { createHash,randomUUID } from "node:crypto";
import { join } from "node:path";
import type { RuntimeEnv } from "@/server/modules/runtime/public/runtime/env";
import type { FileStore } from "@/server/modules/runtime/public/runtime/types";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { LocalFileStore } from "@/server/modules/runtime/public/runtime/filesystem";
import { MinioFileStore } from "@/server/modules/runtime/public/runtime/minio";
import { attachmentUploadsFrozen } from "@/server/modules/files/public/attachments/feature";
import { detectedAttachmentMediaType } from "@/server/modules/files/public/attachments/preview";
import { scanAttachmentBytes } from "@/server/modules/files/public/attachments/scanner";

export function billingEvidenceStore(env: RuntimeEnv): FileStore {
  const local = new LocalFileStore(join(env.ERPX_DATA_DIR, "files", "platform-billing"));
  return process.env.ERPX_MINIO_ENDPOINT && process.env.ERPX_MINIO_BUCKET
    ? new MinioFileStore(process.env.ERPX_MINIO_BUCKET, `environments/${env.ERPX_ENVIRONMENT}/platform-billing`, process.env.ERPX_MINIO_ENDPOINT, process.env.ERPX_MINIO_ACCESS_KEY ?? "", process.env.ERPX_MINIO_SECRET_KEY ?? "", process.env.ERPX_MINIO_REGION ?? "us-east-1", undefined, local)
    : local;
}

import {createBillingEvidence,type BillingEvidenceRepository} from '../application/evidence';
import type {BillingEvidenceSource,BillingScanJob} from '../domain/evidence';
export {billingEvidenceReconciliationReady} from '../domain/evidence';
function nativeEvidence(env:RuntimeEnv){const db=env.CONTROL_DB as PostgresDatabase,store=billingEvidenceStore(env),scannerEnvironment={...process.env,ERPX_ATTACHMENTS_V2:'true'};const repository:BillingEvidenceRepository={
readinessCounts:async()=>(await db.pool.query(`SELECT count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint pending,count(*) FILTER (WHERE source_state='malware_rejected')::bigint sealed_malware FROM billing_evidence`)).rows[0],
latestFinalReconciliation:async()=>(await db.pool.query(`SELECT ready,created_at FROM billing_evidence_reconciliation_runs WHERE mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1`)).rows[0],
sources:async()=>(await db.pool.query<BillingEvidenceSource>('SELECT id,holding_id,object_key,size,sha256,source_state FROM billing_evidence ORDER BY id')).rows,
sourceLedger:async row=>(await db.pool.query('SELECT bytes FROM billing_evidence_storage_ledger WHERE evidence_id=$1 AND holding_id=$2 AND object_key=$3 AND state=\'stored\'',[row.id,row.holding_id,row.object_key])).rows[0],
storedTotals:async()=>(await db.pool.query(`SELECT COALESCE(sum(bytes),0)::bigint stored_bytes,(SELECT count(*)::bigint FROM (SELECT object_key FROM billing_evidence_storage_ledger WHERE state='stored' GROUP BY object_key HAVING count(*)>1) duplicate_objects) AS duplicate_objects FROM billing_evidence_storage_ledger WHERE state='stored'`)).rows[0],
pendingScans:async()=>(await db.pool.query(`SELECT count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint pending FROM billing_evidence`)).rows[0],
recordReconciliation:({id,input,uploadFrozen,ready,report,pending,now})=>db.pool.query(`INSERT INTO billing_evidence_reconciliation_runs(id,mode,upload_frozen,checksums_verified,scanner_engine,scanner_definition_at,ready,report_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)`,[id,input.mode,uploadFrozen,input.verifyChecksums,input.scanner.engine,input.scanner.definitionUpdatedAt,ready,JSON.stringify({scanPending:pending,...report}),now]),
claimScan:async(now,lease)=>(await db.pool.query<BillingScanJob>(`UPDATE billing_evidence_scan_attempts SET state='running',claimed_at=$1,claim_until=$2,lease_token=$3 WHERE id=(SELECT id FROM billing_evidence_scan_attempts WHERE state IN ('pending','running') AND created_at<=$1 AND (claim_until IS NULL OR claim_until<$1) ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *`,[now,now+60_000,lease])).rows[0],
scanSource:async id=>(await db.pool.query<BillingEvidenceSource>('SELECT * FROM billing_evidence WHERE id=$1',[id])).rows[0],
markScanning:id=>db.pool.query("UPDATE billing_evidence SET source_state='scanning' WHERE id=$1 AND source_state IN ('legacy_unscanned','quarantined','scan_error')",[id]),
completeScan:({job,source,lease,result,detected,completed,eventId})=>db.transaction(tx=>tx.batch(result.clean?[
 tx.prepare("UPDATE billing_evidence_scan_attempts SET state='clean',scanner_engine=?,scanner_signature=NULL,scanner_definition_at=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?").bind(result.engine,result.definitionUpdatedAt,completed,job.id,lease),
 tx.prepare("UPDATE billing_evidence SET source_state='available',detected_mime=?,scanned_at=?,scanner_definition_at=? WHERE id=?").bind(detected,completed,result.definitionUpdatedAt,source.id),
 tx.prepare("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.available',?,?)").bind(eventId,source.id,source.holding_id,JSON.stringify({engine:result.engine,definitionUpdatedAt:result.definitionUpdatedAt}),completed)
]:[
 tx.prepare("UPDATE billing_evidence_scan_attempts SET state='infected',scanner_engine=?,scanner_signature=?,scanner_definition_at=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?").bind(result.engine,result.signature,result.definitionUpdatedAt,completed,job.id,lease),
 tx.prepare("UPDATE billing_evidence SET source_state='malware_rejected',detected_mime=?,scanned_at=?,scanner_definition_at=? WHERE id=?").bind(detected,completed,result.definitionUpdatedAt,source.id),
 tx.prepare("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.malware_rejected',?,?)").bind(eventId,source.id,source.holding_id,JSON.stringify({signature:result.signature,engine:result.engine}),completed)
])),
retryScan:({job,source,lease,code,completed,eventId,retryId,retryAt})=>db.transaction(async tx=>{const statements=[
 tx.prepare("UPDATE billing_evidence_scan_attempts SET state='error',error_code=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?").bind(code,completed,job.id,lease),
 tx.prepare("UPDATE billing_evidence SET source_state='scan_error' WHERE id=?").bind(source.id),
 tx.prepare("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.error',?,?)").bind(eventId,source.id,source.holding_id,JSON.stringify({attempt:Number(job.attempt),code}),completed)
];if(retryId)statements.push(tx.prepare("INSERT INTO billing_evidence_scan_attempts(id,evidence_id,attempt,state,created_at) VALUES(?,?,?,'pending',?) ON CONFLICT(evidence_id,attempt) DO NOTHING").bind(retryId,source.id,Number(job.attempt)+1,retryAt));return tx.batch(statements);})
};return createBillingEvidence({repository,uploadsFrozen:attachmentUploadsFrozen,now:Date.now,id:randomUUID,objectHead:key=>store.head(key),objectBytes:async key=>{const object=await store.get(key);return object?new Uint8Array(await object.arrayBuffer()):null},listObjects:cursor=>store.list({cursor,limit:1000}),hash:bytes=>createHash('sha256').update(bytes).digest('hex'),detect:detectedAttachmentMediaType,scan:bytes=>scanAttachmentBytes(bytes,scannerEnvironment)})}
export const billingEvidenceReadiness=(env:RuntimeEnv)=>nativeEvidence(env).billingEvidenceReadiness();
export const reconcileBillingEvidenceStorage=(env:RuntimeEnv,options:{verifyChecksums?:boolean}={})=>nativeEvidence(env).reconcileBillingEvidenceStorage(options);
export const recordBillingEvidenceReconciliation=(env:RuntimeEnv,input:Parameters<ReturnType<typeof createBillingEvidence>['recordBillingEvidenceReconciliation']>[0])=>nativeEvidence(env).recordBillingEvidenceReconciliation(input);
export const drainBillingEvidenceScans=(env:RuntimeEnv,limit=25,options:{offlineMigration?:boolean}={})=>nativeEvidence(env).drainBillingEvidenceScans(limit,options);
