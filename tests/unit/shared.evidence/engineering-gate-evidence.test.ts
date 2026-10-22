import { expect,it } from "vitest";
import { attachmentReconciliationEvidenceValid,businessWorkerEvidenceValid,businessWorkerRequiredProofs,financialContinuityEvidenceValid,stockContinuityEvidenceValid } from "@erpx/feature-runtime/lib/engineering-gate-evidence";
import {reconcileStockSnapshots,type StockSnapshot} from "../../../fixtures/inventory/builders/engineering-stock-continuity";

const release={sourceFingerprint:"a".repeat(64),softwareVersion:`release-${"a".repeat(64)}`,fileManifestSha256:"b".repeat(64)};
const evidence=()=>({passed:true,compiledCandidate:true,...release,closedMonths:["2026-01","2026-02"],comparativeYear:2025,currentYear:2026,sourceJournals:["j1","j2"],sourceInvoices:["i1","i2"],postedYearCloseJournalIds:["close1","close2"],issuedStatementIds:["original","amendment"],originalSha256:"c".repeat(64),amendmentSha256:"d".repeat(64),notes:true,comparatives:true,equityMovementVerified:true,closingProfitMinor:11000,beforeTransferProfitMinor:11000,subledgerDifferenceMinor:"0",trialBalanceDifferenceMinor:"0",immutableIssuedCopies:true,staleSourceRejected:true,companyScopeVerified:true,currentAssetsMinor:67500,currentLiabilitiesMinor:1500,currentEquityMinor:66000,subledgerAssertions:[{kind:"ar",ledgerMinor:2000,sourceMinor:2000,differenceMinor:"0"},{kind:"ap",ledgerMinor:1500,sourceMinor:1500,differenceMinor:"0"}]});
it('rejects missing stock movements, altered originals and offset Company differences despite green summary flags',()=>{
 const before:StockSnapshot={stocks:[{companyId:'c1',warehouseId:'w1',productId:'p1',quantityMilli:'3000',valueMinor:'1000'}],movements:[],companies:[{companyId:'c1',functionalCurrencies:['THB'],inventoryAccountIds:['inventory'],invalidMappings:0,physicalMinor:'1000',inTransitMinor:'0',ledgerMinor:'1000',stockRows:1}],wip:[],invalidRows:0,invalidTrackedBalances:0,unmappedWipMovements:0};
 const after=structuredClone(before);after.stocks[0].quantityMilli='2000';after.stocks[0].valueMinor='667';after.companies[0].physicalMinor='667';after.companies[0].inTransitMinor='333';after.movements.push({...after.stocks[0],quantityMilli:'-1000',valueMinor:'-333',id:'dispatch',sourceType:'transit',sourceId:'transfer',movementDate:'2026-10-03',createdAt:'1'});
 const proof=reconcileStockSnapshots(before,after);expect(proof.passed).toBe(true);
 const empty={...structuredClone(before),stocks:[],companies:[]},receipt={schemaVersion:1,scope:'isolated-engineering',run:'abc123def456',project:'desktop',...release,verificationFingerprint:'f'.repeat(64),passed:true,baselineArtifact:'stock-desktop-baseline.json',baselineArtifactSha256:'c'.repeat(64),finalSnapshotArtifact:'stock-desktop-final-snapshot.json',finalSnapshotArtifactSha256:'d'.repeat(64),results:[{holdingId:'h001',...proof},...['h002','h003'].map(holdingId=>({holdingId,...reconcileStockSnapshots(empty,empty)}))]};
 expect(stockContinuityEvidenceValid(receipt,release,'desktop')).toBe(true);
 expect(stockContinuityEvidenceValid(receipt,release,'mobile')).toBe(false);
 const missing=structuredClone(after);missing.movements=[];expect(reconcileStockSnapshots(before,missing).passed).toBe(false);
 const retained=structuredClone(after),changed=structuredClone(after);changed.movements[0].valueMinor='-332';expect(reconcileStockSnapshots(retained,changed).retainedMovements).toBe(false);
 const offset=structuredClone(after);offset.companies[0].ledgerMinor='999';offset.companies.push({...offset.companies[0],companyId:'c2',physicalMinor:'1000',inTransitMinor:'0',ledgerMinor:'1001'});expect(reconcileStockSnapshots(before,offset).passed).toBe(false);
 const concealed=structuredClone(receipt);concealed.results[0].companies[0].ledgerMinor='999';expect(stockContinuityEvidenceValid(concealed,release,'desktop')).toBe(false);
 const changedSku=structuredClone(receipt);changedSku.results[0].stockAssertions[0].expectedQuantityMilli='1999';expect(stockContinuityEvidenceValid(changedSku,release,'desktop')).toBe(false);
 for(const field of ['invalidRows','invalidTrackedBalances','unmappedWipMovements'] as const){const bad=structuredClone(after);bad[field]=1;expect(reconcileStockSnapshots(before,bad).passed).toBe(false);}
});
it("requires actual consecutive closes, comparative editions and reconciled minor-unit values on the compiled candidate",()=>{
  expect(financialContinuityEvidenceValid(evidence(),release)).toBe(true);
  for(const change of [{compiledCandidate:false},{sourceFingerprint:"e".repeat(64)},{closedMonths:["2026-01","2026-03"]},{postedYearCloseJournalIds:["close1","close1"]},{originalSha256:"d".repeat(64)},{closingProfitMinor:11001},{currentAssetsMinor:67500.5},{trialBalanceDifferenceMinor:"1"},{subledgerAssertions:[{kind:"ar",ledgerMinor:2000,sourceMinor:1999,differenceMinor:"0"}]}])
    expect(financialContinuityEvidenceValid({...evidence(),...change},release)).toBe(false);
});
it("rejects a cached ready attachment flag concealing unknown ownership, unavailable bytes or quota drift",()=>{
  const reconciliation={unknownOwnership:0,legacySanitizedOnly:0,unknownOwnershipSources:[],unavailableSources:[],unavailable:0,sealedMalware:0,quotaDeltaBytes:0,duplicateLedgerObjects:0,orphanObjects:[],checksumMismatches:[],missingObjects:[],sizeMismatches:[],missingLedger:[],crossCompanyLinks:[],crossCompanyIntents:[],crossCompanyStorage:[],crossCompanyDerivatives:[],crossCompanyPublications:[]};
  const value={sourceFingerprint:release.sourceFingerprint,reconciledAt:1000,gateReady:true,apply:true,final:true,scan:true,verifyChecksums:true,freezeConfirmed:true,scanner:{ready:true,enabled:true,engine:"ClamAV 1.5.4/28141",definitionUpdatedAt:1},preview:{ready:true},billing:{ready:true,scanReady:true,reconciliationReady:true,pending:0,sealedMalware:0,reconciledAt:999},billingReconciliation:{ready:true,uploadFrozen:true,checksumsVerified:true,report:reconciliation},results:["h001","h002","h003"].map(holdingId=>({holdingId,backfill:{unknown:[],legacySanitizedOnly:[]},specialized:{unknown:[]},reconciliation}))};
  expect(attachmentReconciliationEvidenceValid(value,release.sourceFingerprint)).toBe(true);
  expect(attachmentReconciliationEvidenceValid({...value,sourceFingerprint:"f".repeat(64)},release.sourceFingerprint)).toBe(false);
  expect(attachmentReconciliationEvidenceValid({...value,sourceFingerprint:undefined},release.sourceFingerprint)).toBe(false);
  for(const change of [{unknownOwnership:1},{quotaDeltaBytes:1},{unavailable:1},{orphanObjects:["unknown"]},{missingObjects:["missing"]}])
    expect(attachmentReconciliationEvidenceValid({...value,results:[{...value.results[0],reconciliation:{...reconciliation,...change}},...value.results.slice(1)]},release.sourceFingerprint)).toBe(false);
  for(const field of ['crossCompanyLinks','crossCompanyIntents','crossCompanyStorage','crossCompanyDerivatives','crossCompanyPublications']){
    const changed=structuredClone(value);Object.assign(changed.results[0].reconciliation,{[field]:['foreign-company']});expect(attachmentReconciliationEvidenceValid(changed,release.sourceFingerprint)).toBe(false);
  }
  for(const report of [{...reconciliation,sealedMalware:1},{...reconciliation,missingObjects:['billing-source']},{...reconciliation,quotaDeltaBytes:1}])
    expect(attachmentReconciliationEvidenceValid({...value,billingReconciliation:{...value.billingReconciliation,report}},release.sourceFingerprint)).toBe(false);
  for(const change of [{scan:false},{results:value.results.slice(0,2)},{results:[value.results[0],value.results[1],value.results[1]]},{results:value.results.map(row=>({...row,holdingId:'foreign'}))},{scanner:{...value.scanner,definitionUpdatedAt:1001}},{reconciledAt:24*60*60_000+2},{billing:{...value.billing,pending:1}},{billing:{...value.billing,scanReady:false}},{billingReconciliation:{...value.billingReconciliation,checksumsVerified:false}},{billingReconciliation:{...value.billingReconciliation,uploadFrozen:false}}])
    expect(attachmentReconciliationEvidenceValid({...value,...change},release.sourceFingerprint)).toBe(false);
});

it("refuses green worker flags that conceal disabled delivery, changed sources, stale recipients or a repeated worker process",()=>{
  const names=["enabled","disabled","unenrolled","withdrawn","expired"] as const,compiledJobsSha256="e".repeat(64);
  const identity={schemaVersion:1,scope:"isolated-engineering",run:"abc123def456",sourceFingerprint:release.sourceFingerprint,softwareVersion:release.softwareVersion,releaseManifestSha256:release.fileManifestSha256};
  const companies=Object.fromEntries(names.map(name=>[name,{companyId:name,holds:[`${name}-due`,`${name}-late`],salesId:`${name}-sale`,fieldId:`${name}-field`,productionId:`${name}-production`,messageId:`${name}-message`,evidence:{sourceId:`${name}-file`,sha256:"c".repeat(64),engine:"ClamAV 1.5.4/28141"}}]));
  const control={...identity,companies,currentRecipients:{owner:"reviewer",revoked:"quality-reviewer",revokedAssignment:"old-grant"}};
  const mail={status:"sent",attempts:1,provider_message_id:"sandbox-enabled-message",recipient_email:"fixture@engineering.invalid",subject:"Declared fixture",body_text:"Retained source message",adapter:"sandbox"};
  const first={alerts:[{id:"due",hold_id:"enabled-due",phase:"due_soon"},{id:"late",hold_id:"enabled-late",phase:"overdue"}],recipients:[{record_id:"due",user_id:"reviewer"},{record_id:"late",user_id:"reviewer"}],reservations:[{record_type:"commerce_order",record_id:"enabled-sale",user_id:"maker"},{record_type:"field_service_order",record_id:"enabled-field",user_id:"maker"},{record_type:"field_service_order",record_id:"enabled-field",user_id:"technician"},{record_type:"manufacturing_order",record_id:"enabled-production",user_id:"reviewer"},{record_type:"manufacturing_order",record_id:"enabled-production",user_id:"maker"}],mail};
  const blocked=Object.fromEntries(names.filter(name=>name!=="enabled").map(name=>[name,{alerts:[],recipients:[],reservations:[],mail:{...mail,status:"pending",attempts:0,provider_message_id:null}}]));
  const value={...identity,compiledJobsSha256,passed:[...businessWorkerRequiredProofs],companies:Object.fromEntries(names.map(name=>[name,name])),nativeSourceSnapshots:Object.fromEntries(names.map(name=>[name,{beforeSha256:"c".repeat(64),afterSha256:"c".repeat(64)}])),observations:{first,afterRestart:{...first,recipients:first.alerts.flatMap(alert=>[{record_id:alert.id,user_id:"reviewer"},{record_id:alert.id,user_id:"quality-reviewer"}])},blocked},currentRecipients:{revokedAssignment:"old-grant",replacementAssignment:"new-grant",reviewerId:"reviewer",recoveredReviewerId:"quality-reviewer"},restart:{cohortJobsPaused:true,workerPids:[71,72],firstWorkerPid:71,restartedWorkerPid:72,preStartHeartbeat:1,firstHeartbeat:2,secondHeartbeat:3},environment:{port:3104,databases:8,node:"26.11.1",bun:"1.4.3",provider:"sandbox",scanner:"actual local ClamAV"}};
  expect(businessWorkerEvidenceValid(value,control,release,compiledJobsSha256)).toBe(true);
  expect(businessWorkerEvidenceValid({passed:value.passed},control,release,compiledJobsSha256)).toBe(false);
  expect(businessWorkerEvidenceValid(value,control,release,"f".repeat(64))).toBe(false);
  const delivery=structuredClone(value);delivery.observations.blocked.disabled.mail.status="sent";
  expect(businessWorkerEvidenceValid(delivery,control,release,compiledJobsSha256)).toBe(false);
  const changed=structuredClone(value);changed.nativeSourceSnapshots.enabled.afterSha256="d".repeat(64);
  expect(businessWorkerEvidenceValid(changed,control,release,compiledJobsSha256)).toBe(false);
  const recipients=structuredClone(value);recipients.currentRecipients.recoveredReviewerId="unknown-person";
  expect(businessWorkerEvidenceValid(recipients,control,release,compiledJobsSha256)).toBe(false);
  const restart=structuredClone(value);restart.restart.restartedWorkerPid=71;
  expect(businessWorkerEvidenceValid(restart,control,release,compiledJobsSha256)).toBe(false);
});
