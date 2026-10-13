import { getRuntime, closeRuntime } from "../../src/server/modules/runtime/adapters/runtime/env";
import { resourceSlots, registeredResources } from "../../src/lib/runtime/holding-registry";
import { backfillLegacyAttachments,backfillSpecializedAttachments, reconcileAttachmentStorage } from "../../src/server/modules/files/public/attachments/migration";
import { buildAttachmentAdjudicationManifest,holdingAttachmentReconciliationReady } from "../../src/server/modules/files/public/attachments/reconciliation-adjudication";
import { attachmentUploadsFrozen,attachmentsV2Enabled } from "../../src/server/modules/files/public/attachments/feature";
import { attachmentRuntimeReadiness } from "../../src/server/modules/files/public/attachments/readiness";
import { drainAttachmentScans } from "../../src/server/modules/files/public/attachments/service";
import { billingEvidenceReadiness,billingEvidenceReconciliationReady,drainBillingEvidenceScans,reconcileBillingEvidenceStorage,recordBillingEvidenceReconciliation } from "../../src/server/modules/billing/public/billing-attachments";
import { randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { runtimeSourceFingerprint } from "./source-fingerprint";

type ReconciliationResult={holdingId:string;scanned:number;backfill:Awaited<ReturnType<typeof backfillLegacyAttachments>>;specialized:Awaited<ReturnType<typeof backfillSpecializedAttachments>>;reconciliation:Awaited<ReturnType<typeof reconcileAttachmentStorage>>};
const apply=process.argv.includes("--apply"),verifyChecksums=process.argv.includes("--checksum"),final=process.argv.includes("--final"),scan=process.argv.includes("--scan"),env=getRuntime(),results:ReconciliationResult[]=[];
try{
  const releasePath=join(process.cwd(),'release.json');
  const sourceFingerprint=existsSync(releasePath)
    ?JSON.parse(await readFile(releasePath,'utf8')).sourceFingerprint as string
    :await runtimeSourceFingerprint(process.cwd());
  if(!/^[a-f0-9]{64}$/.test(sourceFingerprint)||process.env.ERPX_ENGINEERING_SOURCE_FINGERPRINT&&process.env.ERPX_ENGINEERING_SOURCE_FINGERPRINT!==sourceFingerprint)throw new Error('RECONCILIATION_SOURCE_IDENTITY_MISMATCH');
  if(final&&attachmentsV2Enabled())throw new Error("FINAL_RECONCILIATION_REQUIRES_GATE_DISABLED");
  if(final&&(!apply||!verifyChecksums||!attachmentUploadsFrozen()))throw new Error("FINAL_RECONCILIATION_REQUIRES_APPLY_CHECKSUM_AND_UPLOAD_FREEZE");
  if(scan&&(!apply||!attachmentUploadsFrozen()))throw new Error("OFFLINE_SCAN_REQUIRES_APPLY_AND_UPLOAD_FREEZE");
  for(const slot of resourceSlots(env)){
    const {write,files}=registeredResources(env,slot.id),backfill=await backfillLegacyAttachments(write,{apply}),specialized=await backfillSpecializedAttachments(write,{apply,files});let scanned=0;
    if(scan)while(await drainAttachmentScans(write,files,undefined,{offlineMigration:true}))scanned++;
    const reconciliation=await reconcileAttachmentStorage(write,files,{verifyChecksums});results.push({holdingId:slot.id,scanned,backfill,specialized,reconciliation});
  }
  let billingScanned=0;if(scan){let batch=0;do{batch=await drainBillingEvidenceScans(env,250,{offlineMigration:true});billingScanned+=batch;}while(batch>0);}
  const runtime=await attachmentRuntimeReadiness(),scanner=runtime.scanner,preview=runtime.preview,freezeConfirmed=!final||attachmentUploadsFrozen();
  const billingReconciliation=final
    ?await recordBillingEvidenceReconciliation(env,{mode:"final",verifyChecksums,scanner:{ready:scanner.ready,enabled:scanner.enabled,engine:scanner.engine,definitionUpdatedAt:scanner.definitionUpdatedAt}})
    :{id:null,ready:false,uploadFrozen:attachmentUploadsFrozen(),checksumsVerified:verifyChecksums,report:await reconcileBillingEvidenceStorage(env,{verifyChecksums})};
  if(!final)billingReconciliation.ready=billingEvidenceReconciliationReady(billingReconciliation.report);
  const billing=await billingEvidenceReadiness(env);
  const prerequisites=freezeConfirmed&&scanner.ready&&scanner.enabled&&preview.ready&&billing.ready;
  const gateReady=final&&prerequisites&&billingReconciliation.ready&&results.every(holdingAttachmentReconciliationReady);
  const snapshot={sourceFingerprint,reconciledAt:Date.now(),apply,verifyChecksums,final,scan,freezeConfirmed,scanner,preview,billing,billingReconciliation,billingScanned,gateReady,results};
  const adjudication=buildAttachmentAdjudicationManifest(snapshot);
  if(final)for(const result of results){const {write}=registeredResources(env,result.holdingId);await write.prepare(`INSERT INTO attachment_reconciliation_runs(id,mode,upload_frozen,checksums_verified,scanner_engine,scanner_definition_at,ready,report_json,created_at)
    VALUES(?,'final',true,true,?,?,?,?,?)`).bind(`attachment-reconciliation-${randomUUID()}`,scanner.engine,scanner.definitionUpdatedAt,prerequisites&&billingReconciliation.ready&&holdingAttachmentReconciliationReady(result),JSON.stringify({sourceFingerprint,scanned:result.scanned,backfill:result.backfill,specialized:result.specialized,reconciliation:result.reconciliation,billing,billingReconciliation,billingScanned,scanner,preview,adjudication}),Date.now()).run();}
  process.stdout.write(`${JSON.stringify({...snapshot,adjudication},null,2)}\n`);
}finally{await closeRuntime();}
