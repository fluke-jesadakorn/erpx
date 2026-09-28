import "server-only";

import { createHash,randomUUID } from "node:crypto";
import { join } from "node:path";
import type { RuntimeEnv } from "./runtime/env";
import type { FileStore } from "./runtime/types";
import type { PostgresDatabase } from "./runtime/postgres";
import { LocalFileStore } from "./runtime/filesystem";
import { MinioFileStore } from "./runtime/minio";
import { attachmentUploadsFrozen } from "./attachments/feature";
import { detectedAttachmentMediaType } from "./attachments/preview";
import { scanAttachmentBytes } from "./attachments/scanner";

export function billingEvidenceStore(env: RuntimeEnv): FileStore {
  const local = new LocalFileStore(join(env.ERPX_DATA_DIR, "files", "platform-billing"));
  return process.env.ERPX_MINIO_ENDPOINT && process.env.ERPX_MINIO_BUCKET
    ? new MinioFileStore(process.env.ERPX_MINIO_BUCKET, `environments/${env.ERPX_ENVIRONMENT}/platform-billing`, process.env.ERPX_MINIO_ENDPOINT, process.env.ERPX_MINIO_ACCESS_KEY ?? "", process.env.ERPX_MINIO_SECRET_KEY ?? "", process.env.ERPX_MINIO_REGION ?? "us-east-1", undefined, local)
    : local;
}

export async function billingEvidenceReadiness(env:RuntimeEnv){
  const db=env.CONTROL_DB as PostgresDatabase,row=(await db.pool.query(`SELECT
    count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint pending,
    count(*) FILTER (WHERE source_state='malware_rejected')::bigint sealed_malware
    FROM billing_evidence`)).rows[0] as {pending:string|number;sealed_malware:string|number},latest=(await db.pool.query(`SELECT ready,created_at FROM billing_evidence_reconciliation_runs
      WHERE mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1`)).rows[0] as {ready:boolean;created_at:number}|undefined;
  const scanReady=Number(row.pending)===0,reconciliationReady=latest?.ready===true;
  return {ready:scanReady&&reconciliationReady,scanReady,reconciliationReady,pending:Number(row.pending),sealedMalware:Number(row.sealed_malware),reconciledAt:latest?Number(latest.created_at):null};
}

export async function reconcileBillingEvidenceStorage(env:RuntimeEnv,{verifyChecksums=false}:{verifyChecksums?:boolean}={}){
  const db=env.CONTROL_DB as PostgresDatabase,store=billingEvidenceStore(env),rows=(await db.pool.query(`SELECT id,holding_id,object_key,size,sha256,source_state FROM billing_evidence ORDER BY id`)).rows as {id:string;holding_id:string;object_key:string;size:number;sha256:string;source_state:string}[];
  const report={sources:rows.length,unavailable:0,sealedMalware:0,destroyed:0,missingObjects:[] as string[],sizeMismatches:[] as string[],checksumMismatches:[] as string[],missingLedger:[] as string[],orphanObjects:[] as string[],ledgerStoredBytes:0,physicalStoredBytes:0,quotaDeltaBytes:0,duplicateLedgerObjects:0};
  const known=new Set<string>();
  for(const row of rows){
    if(["legacy_unscanned","quarantined","scanning","scan_error"].includes(row.source_state))report.unavailable++;
    if(row.source_state==="malware_rejected")report.sealedMalware++;
    if(row.source_state==="destroyed"){report.destroyed++;continue;}
    known.add(row.object_key);
    const head=await store.head(row.object_key);
    if(!head){report.missingObjects.push(row.id);continue;}
    report.physicalStoredBytes+=head.size;
    if(head.size!==Number(row.size))report.sizeMismatches.push(row.id);
    if(verifyChecksums){const object=await store.get(row.object_key);if(!object||createHash("sha256").update(new Uint8Array(await object.arrayBuffer())).digest("hex")!==row.sha256)report.checksumMismatches.push(row.id);}
    const ledger=(await db.pool.query("SELECT bytes FROM billing_evidence_storage_ledger WHERE evidence_id=$1 AND holding_id=$2 AND object_key=$3 AND state='stored'",[row.id,row.holding_id,row.object_key])).rows[0] as {bytes:number}|undefined;
    if(!ledger||Number(ledger.bytes)!==Number(row.size))report.missingLedger.push(row.id);
  }
  const totals=(await db.pool.query(`SELECT COALESCE(sum(bytes),0)::bigint stored_bytes,
    (SELECT count(*)::bigint FROM (SELECT object_key FROM billing_evidence_storage_ledger WHERE state='stored' GROUP BY object_key HAVING count(*)>1) duplicate_objects) AS duplicate_objects
    FROM billing_evidence_storage_ledger WHERE state='stored'`)).rows[0] as {stored_bytes:string|number;duplicate_objects:string|number};
  report.ledgerStoredBytes=Number(totals.stored_bytes);report.quotaDeltaBytes=report.ledgerStoredBytes-report.physicalStoredBytes;report.duplicateLedgerObjects=Number(totals.duplicate_objects);
  let cursor:string|undefined;do{const page=await store.list({cursor,limit:1000});for(const object of page.objects)if(!known.has(object.key))report.orphanObjects.push(object.key);cursor=page.truncated?page.cursor:undefined;}while(cursor);
  return report;
}

export function billingEvidenceReconciliationReady(report:Awaited<ReturnType<typeof reconcileBillingEvidenceStorage>>){
  return report.unavailable===0&&report.missingObjects.length===0&&report.sizeMismatches.length===0&&report.checksumMismatches.length===0&&report.missingLedger.length===0&&report.orphanObjects.length===0&&report.quotaDeltaBytes===0&&report.duplicateLedgerObjects===0;
}

export async function recordBillingEvidenceReconciliation(env:RuntimeEnv,input:{mode:"audit"|"final";verifyChecksums:boolean;scanner:{ready:boolean;enabled:boolean;engine:string|null;definitionUpdatedAt:number|null}}){
  const uploadFrozen=attachmentUploadsFrozen();
  if(input.mode==="final"&&(!uploadFrozen||!input.verifyChecksums))throw new Error("FINAL_BILLING_RECONCILIATION_REQUIRES_CHECKSUM_AND_UPLOAD_FREEZE");
  const report=await reconcileBillingEvidenceStorage(env,{verifyChecksums:input.verifyChecksums}),db=env.CONTROL_DB as PostgresDatabase;
  const scan=(await db.pool.query(`SELECT count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint pending FROM billing_evidence`)).rows[0] as {pending:string|number};
  const ready=Number(scan.pending)===0&&billingEvidenceReconciliationReady(report)&&input.scanner.ready&&input.scanner.enabled&&(input.mode!=="final"||uploadFrozen&&input.verifyChecksums),now=Date.now(),id=`billing-reconciliation-${randomUUID()}`;
  await db.pool.query(`INSERT INTO billing_evidence_reconciliation_runs(id,mode,upload_frozen,checksums_verified,scanner_engine,scanner_definition_at,ready,report_json,created_at)
    VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)`,[id,input.mode,uploadFrozen,input.verifyChecksums,input.scanner.engine,input.scanner.definitionUpdatedAt,ready,JSON.stringify({scanPending:Number(scan.pending),...report}),now]);
  return {id,ready,uploadFrozen,checksumsVerified:input.verifyChecksums,report};
}

export async function drainBillingEvidenceScans(env: RuntimeEnv, limit = 25, options:{offlineMigration?:boolean}={}) {
  if(options.offlineMigration&&!attachmentUploadsFrozen())throw new Error("OFFLINE_BILLING_SCAN_REQUIRES_UPLOAD_FREEZE");
  const db=env.CONTROL_DB as PostgresDatabase,store=billingEvidenceStore(env);
  const scannerEnvironment={...process.env,ERPX_ATTACHMENTS_V2:"true"};
  let processed=0;
  while(processed<limit){
    const now=Date.now(),lease=`billing-evidence-lease-${randomUUID()}`;
    const job=(await db.pool.query(`UPDATE billing_evidence_scan_attempts SET state='running',claimed_at=$1,claim_until=$2,lease_token=$3
      WHERE id=(SELECT id FROM billing_evidence_scan_attempts WHERE state IN ('pending','running') AND created_at<=$1 AND (claim_until IS NULL OR claim_until<$1)
      ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *`,[now,now+60_000,lease])).rows[0];
    if(!job)break;
    const source=(await db.pool.query("SELECT * FROM billing_evidence WHERE id=$1",[job.evidence_id])).rows[0];
    if(!source)throw new Error("BILLING_EVIDENCE_SOURCE_MISSING");
    await db.pool.query("UPDATE billing_evidence SET source_state='scanning' WHERE id=$1 AND source_state IN ('legacy_unscanned','quarantined','scan_error')",[source.id]);
    try{
      const object=await store.get(source.object_key);if(!object)throw new Error("BILLING_EVIDENCE_OBJECT_MISSING");
      const bytes=new Uint8Array(await object.arrayBuffer());if(bytes.byteLength<1||bytes.byteLength>10*1024*1024)throw new Error("BILLING_EVIDENCE_SIZE");
      if(createHash("sha256").update(bytes).digest("hex")!==source.sha256)throw new Error("BILLING_EVIDENCE_CHECKSUM");
      const detected=detectedAttachmentMediaType(bytes,source.mime);if(detected!==source.mime)throw new Error("BILLING_EVIDENCE_MIME");
      const result=await scanAttachmentBytes(bytes,scannerEnvironment),completed=Date.now();
      if(!result.clean){
        await db.transaction(tx=>tx.batch([
          tx.prepare("UPDATE billing_evidence_scan_attempts SET state='infected',scanner_engine=?,scanner_signature=?,scanner_definition_at=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?").bind(result.engine,result.signature,result.definitionUpdatedAt,completed,job.id,lease),
          tx.prepare("UPDATE billing_evidence SET source_state='malware_rejected',detected_mime=?,scanned_at=?,scanner_definition_at=? WHERE id=?").bind(detected,completed,result.definitionUpdatedAt,source.id),
          tx.prepare("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.malware_rejected',?,?)").bind(`billing-event-${randomUUID()}`,source.id,source.holding_id,JSON.stringify({signature:result.signature,engine:result.engine}),completed),
        ]));
      }else{
        await db.transaction(tx=>tx.batch([
          tx.prepare("UPDATE billing_evidence_scan_attempts SET state='clean',scanner_engine=?,scanner_signature=NULL,scanner_definition_at=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?").bind(result.engine,result.definitionUpdatedAt,completed,job.id,lease),
          tx.prepare("UPDATE billing_evidence SET source_state='available',detected_mime=?,scanned_at=?,scanner_definition_at=? WHERE id=?").bind(detected,completed,result.definitionUpdatedAt,source.id),
          tx.prepare("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.available',?,?)").bind(`billing-event-${randomUUID()}`,source.id,source.holding_id,JSON.stringify({engine:result.engine,definitionUpdatedAt:result.definitionUpdatedAt}),completed),
        ]));
      }
    }catch(error){
      const completed=Date.now(),code=(error instanceof Error?error.message:"BILLING_SCAN_ERROR").slice(0,160),attempt=Number(job.attempt);
      await db.transaction(async tx=>{const statements=[
        tx.prepare("UPDATE billing_evidence_scan_attempts SET state='error',error_code=?,completed_at=?,claim_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?").bind(code,completed,job.id,lease),
        tx.prepare("UPDATE billing_evidence SET source_state='scan_error' WHERE id=?").bind(source.id),
        tx.prepare("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES(?,?,?,'system:jobs','scan.error',?,?)").bind(`billing-event-${randomUUID()}`,source.id,source.holding_id,JSON.stringify({attempt,code}),completed),
      ];if(attempt<6)statements.push(tx.prepare("INSERT INTO billing_evidence_scan_attempts(id,evidence_id,attempt,state,created_at) VALUES(?,?,?,'pending',?) ON CONFLICT(evidence_id,attempt) DO NOTHING").bind(`billing-scan-${randomUUID()}`,source.id,attempt+1,completed+Math.min(60_000,1000*2**attempt)));await tx.batch(statements);});
    }
    processed++;
  }
  return processed;
}
