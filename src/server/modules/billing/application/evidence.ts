import {billingEvidenceReconciliationReady,type BillingEvidenceSource,type BillingScanJob,type BillingScanResult,type BillingEvidenceReconciliation} from '../domain/evidence';
export interface BillingEvidenceRepository {
 readinessCounts():Promise<{pending:string|number;sealed_malware:string|number}>;
 latestFinalReconciliation():Promise<{ready:boolean;created_at:number}|undefined>;
 sources():Promise<BillingEvidenceSource[]>;
 sourceLedger(source:BillingEvidenceSource):Promise<{bytes:number}|undefined>;
 storedTotals():Promise<{stored_bytes:string|number;duplicate_objects:string|number}>;
 pendingScans():Promise<{pending:string|number}>;
 recordReconciliation(facts:{id:string;input:ReconciliationInput;uploadFrozen:boolean;ready:boolean;report:BillingEvidenceReconciliation;pending:number;now:number}):Promise<unknown>;
 claimScan(now:number,lease:string):Promise<BillingScanJob|undefined>;
 scanSource(id:string):Promise<BillingEvidenceSource|undefined>;
 markScanning(id:string):Promise<unknown>;
 completeScan(facts:{job:BillingScanJob;source:BillingEvidenceSource;lease:string;result:BillingScanResult;detected:string;completed:number;eventId:string}):Promise<unknown>;
 retryScan(facts:{job:BillingScanJob;source:BillingEvidenceSource;lease:string;code:string;completed:number;eventId:string;retryId:string|null;retryAt:number}):Promise<unknown>;
}
export type ReconciliationInput={mode:'audit'|'final';verifyChecksums:boolean;scanner:{ready:boolean;enabled:boolean;engine:string|null;definitionUpdatedAt:number|null}};
export interface BillingEvidencePorts {
 repository:BillingEvidenceRepository;uploadsFrozen():boolean;now():number;id():string;
 objectHead(key:string):Promise<{size:number}|null>;objectBytes(key:string):Promise<Uint8Array|null>;
 listObjects(cursor:string|undefined):Promise<{objects:{key:string}[];truncated:boolean;cursor?:string}>;
 hash(bytes:Uint8Array):string;detect(bytes:Uint8Array,mime:string):string|null;scan(bytes:Uint8Array):Promise<BillingScanResult>;
}
export function createBillingEvidence(ports:BillingEvidencePorts){const repo=ports.repository;
 async function billingEvidenceReadiness(){const row=await repo.readinessCounts(),latest=await repo.latestFinalReconciliation(),scanReady=Number(row.pending)===0,reconciliationReady=latest?.ready===true;return {ready:scanReady&&reconciliationReady,scanReady,reconciliationReady,pending:Number(row.pending),sealedMalware:Number(row.sealed_malware),reconciledAt:latest?Number(latest.created_at):null};}
 async function reconcileBillingEvidenceStorage({verifyChecksums=false}:{verifyChecksums?:boolean}={}){const rows=await repo.sources(),report:BillingEvidenceReconciliation={sources:rows.length,unavailable:0,sealedMalware:0,destroyed:0,missingObjects:[],sizeMismatches:[],checksumMismatches:[],missingLedger:[],orphanObjects:[],ledgerStoredBytes:0,physicalStoredBytes:0,quotaDeltaBytes:0,duplicateLedgerObjects:0},known=new Set<string>();
  for(const row of rows){if(['legacy_unscanned','quarantined','scanning','scan_error'].includes(row.source_state))report.unavailable++;if(row.source_state==='malware_rejected')report.sealedMalware++;if(row.source_state==='destroyed'){report.destroyed++;continue;}known.add(row.object_key);const head=await ports.objectHead(row.object_key);if(!head){report.missingObjects.push(row.id);continue;}report.physicalStoredBytes+=head.size;if(head.size!==Number(row.size))report.sizeMismatches.push(row.id);if(verifyChecksums){const bytes=await ports.objectBytes(row.object_key);if(!bytes||ports.hash(bytes)!==row.sha256)report.checksumMismatches.push(row.id);}const ledger=await repo.sourceLedger(row);if(!ledger||Number(ledger.bytes)!==Number(row.size))report.missingLedger.push(row.id);}
  const totals=await repo.storedTotals();report.ledgerStoredBytes=Number(totals.stored_bytes);report.quotaDeltaBytes=report.ledgerStoredBytes-report.physicalStoredBytes;report.duplicateLedgerObjects=Number(totals.duplicate_objects);let cursor:string|undefined;do{const page=await ports.listObjects(cursor);for(const object of page.objects)if(!known.has(object.key))report.orphanObjects.push(object.key);cursor=page.truncated?page.cursor:undefined;}while(cursor);return report;
 }
 async function recordBillingEvidenceReconciliation(input:ReconciliationInput){const uploadFrozen=ports.uploadsFrozen();if(input.mode==='final'&&(!uploadFrozen||!input.verifyChecksums))throw new Error('FINAL_BILLING_RECONCILIATION_REQUIRES_CHECKSUM_AND_UPLOAD_FREEZE');const report=await reconcileBillingEvidenceStorage({verifyChecksums:input.verifyChecksums}),scan=await repo.pendingScans(),ready=Number(scan.pending)===0&&billingEvidenceReconciliationReady(report)&&input.scanner.ready&&input.scanner.enabled&&(input.mode!=='final'||uploadFrozen&&input.verifyChecksums),now=ports.now(),id=`billing-reconciliation-${ports.id()}`;await repo.recordReconciliation({id,input,uploadFrozen,ready,report,pending:Number(scan.pending),now});return {id,ready,uploadFrozen,checksumsVerified:input.verifyChecksums,report};}
 async function drainBillingEvidenceScans(limit=25,options:{offlineMigration?:boolean}={}){if(options.offlineMigration&&!ports.uploadsFrozen())throw new Error('OFFLINE_BILLING_SCAN_REQUIRES_UPLOAD_FREEZE');let processed=0;while(processed<limit){const now=ports.now(),lease=`billing-evidence-lease-${ports.id()}`,job=await repo.claimScan(now,lease);if(!job)break;const source=await repo.scanSource(job.evidence_id);if(!source)throw new Error('BILLING_EVIDENCE_SOURCE_MISSING');await repo.markScanning(source.id);
  try{const bytes=await ports.objectBytes(source.object_key);if(!bytes)throw new Error('BILLING_EVIDENCE_OBJECT_MISSING');if(bytes.byteLength<1||bytes.byteLength>10*1024*1024)throw new Error('BILLING_EVIDENCE_SIZE');if(ports.hash(bytes)!==source.sha256)throw new Error('BILLING_EVIDENCE_CHECKSUM');const detected=ports.detect(bytes,source.mime);if(detected!==source.mime)throw new Error('BILLING_EVIDENCE_MIME');const result=await ports.scan(bytes),completed=ports.now();await repo.completeScan({job,source,lease,result,detected,completed,eventId:`billing-event-${ports.id()}`});
  }catch(error){const completed=ports.now(),code=(error instanceof Error?error.message:'BILLING_SCAN_ERROR').slice(0,160),attempt=Number(job.attempt);await repo.retryScan({job,source,lease,code,completed,eventId:`billing-event-${ports.id()}`,retryId:attempt<6?`billing-scan-${ports.id()}`:null,retryAt:completed+Math.min(60_000,1000*2**attempt)});}processed++;}return processed;
 }
 return {billingEvidenceReadiness,reconcileBillingEvidenceStorage,recordBillingEvidenceReconciliation,drainBillingEvidenceScans};
}
