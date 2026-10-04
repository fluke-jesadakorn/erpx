import { z } from "zod";

const digest=z.string().regex(/^[a-f0-9]{64}$/),ids=z.array(z.string().min(1)).min(2);
const financial=z.object({
  passed:z.literal(true),compiledCandidate:z.literal(true),sourceFingerprint:digest,softwareVersion:z.string(),fileManifestSha256:digest,
  closedMonths:z.array(z.string().regex(/^\d{4}-(?:0[1-9]|1[0-2])$/)).min(2),comparativeYear:z.number().int(),currentYear:z.number().int(),
  sourceJournals:ids,sourceInvoices:ids,postedYearCloseJournalIds:ids,issuedStatementIds:ids,
  originalSha256:digest,amendmentSha256:digest,notes:z.literal(true),comparatives:z.literal(true),equityMovementVerified:z.literal(true),
  closingProfitMinor:z.number().int().safe(),beforeTransferProfitMinor:z.number().int().safe(),
  subledgerDifferenceMinor:z.literal("0"),trialBalanceDifferenceMinor:z.literal("0"),
  immutableIssuedCopies:z.literal(true),staleSourceRejected:z.literal(true),companyScopeVerified:z.literal(true),
  currentAssetsMinor:z.number().int().safe(),currentLiabilitiesMinor:z.number().int().safe(),currentEquityMinor:z.number().int().safe(),
  subledgerAssertions:z.array(z.object({kind:z.string(),ledgerMinor:z.union([z.string(),z.number().int().safe()]),sourceMinor:z.union([z.string(),z.number().int().safe()]),differenceMinor:z.union([z.literal("0"),z.literal(0)])})).min(2),
});
export type CompiledEvidenceIdentity={sourceFingerprint:string;softwareVersion:string;fileManifestSha256:string};
const integer=z.string().regex(/^-?\d+$/),nonnegativeInteger=z.string().regex(/^\d+$/);
/** Validate individual Company and SKU assertions; aggregate zero cannot hide offsets. */
export function stockContinuityEvidenceValid(value:unknown,release:CompiledEvidenceIdentity,project:'desktop'|'mobile'){
 const parsed=z.object({schemaVersion:z.literal(1),scope:z.literal('isolated-engineering'),run:z.string().regex(/^[a-f0-9]{12}$/),project:z.enum(['desktop','mobile']),sourceFingerprint:digest,verificationFingerprint:digest,softwareVersion:z.string(),fileManifestSha256:digest,passed:z.literal(true),baselineArtifact:z.string(),baselineArtifactSha256:digest,finalSnapshotArtifact:z.string(),finalSnapshotArtifactSha256:digest,
 results:z.array(z.object({holdingId:z.string(),passed:z.literal(true),baselineSha256:digest,finalSha256:digest,retainedMovements:z.literal(true),newMovements:z.number().int().nonnegative(),invalidRows:z.literal(0),invalidTrackedBalances:z.literal(0),unmappedWipMovements:z.literal(0),
 stockAssertions:z.array(z.object({companyId:z.string().min(1),warehouseId:z.string().min(1),productId:z.string().min(1),expectedQuantityMilli:nonnegativeInteger,quantityMilli:nonnegativeInteger,expectedValueMinor:nonnegativeInteger,valueMinor:nonnegativeInteger})),
 companies:z.array(z.object({companyId:z.string().min(1),functionalCurrencies:z.array(z.string().regex(/^[A-Z]{3}$/)).length(1),inventoryAccountIds:z.array(z.string().min(1)).min(1),invalidMappings:z.literal(0),physicalMinor:nonnegativeInteger,inTransitMinor:nonnegativeInteger,stockMinor:nonnegativeInteger,ledgerMinor:nonnegativeInteger,differenceMinor:z.literal('0'),stockRows:z.number().int().nonnegative()})),
 wip:z.array(z.object({companyId:z.string().min(1),accountId:z.string().min(1),currency:z.string().regex(/^[A-Z]{3}$/),sourceMinor:nonnegativeInteger,ledgerMinor:nonnegativeInteger,differenceMinor:z.literal('0'),movements:z.number().int().nonnegative()})),
 })).length(3)}).safeParse(value);
 if(!parsed.success)return false;const item=parsed.data;
 return item.sourceFingerprint===release.sourceFingerprint&&item.softwareVersion===release.softwareVersion&&item.fileManifestSha256===release.fileManifestSha256&&item.project===project
  &&item.baselineArtifact===`stock-${project}-baseline.json`&&item.finalSnapshotArtifact===`stock-${project}-final-snapshot.json`
  &&['h001','h002','h003'].every(id=>item.results.some(row=>row.holdingId===id))&&new Set(item.results.map(row=>row.holdingId)).size===3
  &&item.results.some(row=>row.newMovements>0)&&item.results.some(row=>row.stockAssertions.length>0)
  &&item.results.every(row=>new Set(row.companies.map(company=>company.companyId)).size===row.companies.length&&new Set(row.stockAssertions.map(stock=>`${stock.warehouseId}:${stock.productId}`)).size===row.stockAssertions.length
   &&row.stockAssertions.every(stock=>stock.quantityMilli===stock.expectedQuantityMilli&&stock.valueMinor===stock.expectedValueMinor&&row.companies.some(company=>company.companyId===stock.companyId))
   &&row.companies.every(company=>new Set(company.inventoryAccountIds).size===company.inventoryAccountIds.length&&BigInt(company.physicalMinor)+BigInt(company.inTransitMinor)===BigInt(company.stockMinor)&&company.stockMinor===company.ledgerMinor)
   &&row.wip.every(wip=>integer.safeParse(wip.sourceMinor).success&&wip.sourceMinor===wip.ledgerMinor));
}
export const businessWorkerRequiredProofs=["compiled_quality_due_soon_overdue","compiled_sales_field_manufacturing_reservations","compiled_sandbox_delivery_once","disabled_company","unenrolled_company","withdrawn_company","expired_company","current_recipient_revocation_recovery","restart_deduplication","native_sources_and_stock_unchanged","certification_unchanged"] as const;
const workerCompanyNames=["enabled","disabled","unenrolled","withdrawn","expired"] as const;
const workerObservation=z.object({
  alerts:z.array(z.object({id:z.string().min(1),hold_id:z.string().min(1),phase:z.enum(["due_soon","overdue"])})),
  recipients:z.array(z.object({record_id:z.string().min(1),user_id:z.string().min(1)})),
  reservations:z.array(z.object({record_type:z.string().min(1),record_id:z.string().min(1),user_id:z.string().min(1)})),
  mail:z.object({status:z.string(),attempts:z.number().int().nonnegative(),provider_message_id:z.string().nullable(),recipient_email:z.string(),subject:z.string(),body_text:z.string(),adapter:z.literal("sandbox")}),
});
/** Reconcile actual compiled worker observations, current recipients and retained source hashes. */
export function businessWorkerEvidenceValid(value:unknown,control:unknown,release:CompiledEvidenceIdentity,compiledJobsSha256:string) {
  const parsed=z.object({schemaVersion:z.literal(1),scope:z.literal("isolated-engineering"),run:z.string().regex(/^[a-f0-9]{12}$/),sourceFingerprint:digest,softwareVersion:z.string(),releaseManifestSha256:digest,compiledJobsSha256:digest,passed:z.array(z.string()),
    companies:z.record(z.string(),z.string().min(1)),nativeSourceSnapshots:z.record(z.string(),z.object({beforeSha256:digest,afterSha256:digest})),
    observations:z.object({first:workerObservation,afterRestart:workerObservation,blocked:z.record(z.string(),workerObservation)}),
    currentRecipients:z.object({revokedAssignment:z.string().min(1),replacementAssignment:z.string().min(1),reviewerId:z.string().min(1),recoveredReviewerId:z.string().min(1)}),
    restart:z.object({cohortJobsPaused:z.literal(true),workerPids:z.array(z.number().int().positive()).min(2),firstWorkerPid:z.number().int().positive(),restartedWorkerPid:z.number().int().positive(),preStartHeartbeat:z.number().int().nonnegative(),firstHeartbeat:z.number().int().positive(),secondHeartbeat:z.number().int().positive()}),
    environment:z.object({port:z.literal(3104),databases:z.literal(8),node:z.literal("24.20.0"),bun:z.literal("1.2.17"),provider:z.literal("sandbox"),scanner:z.literal("actual local ClamAV")}),
  }).safeParse(value);
  const source=z.object({schemaVersion:z.literal(1),scope:z.literal("isolated-engineering"),run:z.string(),sourceFingerprint:digest,softwareVersion:z.string(),releaseManifestSha256:digest,
    companies:z.record(z.string(),z.object({companyId:z.string(),holds:z.array(z.string()).length(2),salesId:z.string(),fieldId:z.string(),productionId:z.string(),messageId:z.string(),evidence:z.object({sourceId:z.string().min(1),sha256:digest,engine:z.string().regex(/^ClamAV 1\.5\.4\//)})})),
    currentRecipients:z.object({owner:z.string().min(1),revoked:z.string().min(1),revokedAssignment:z.string().min(1)}),
  }).safeParse(control);
  if(!parsed.success||!source.success)return false;
  const item=parsed.data,input=source.data,first=item.observations.first,after=item.observations.afterRestart;
  if(item.sourceFingerprint!==release.sourceFingerprint||item.softwareVersion!==release.softwareVersion||item.releaseManifestSha256!==release.fileManifestSha256||item.compiledJobsSha256!==compiledJobsSha256||input.run!==item.run||input.sourceFingerprint!==item.sourceFingerprint||input.softwareVersion!==item.softwareVersion||input.releaseManifestSha256!==item.releaseManifestSha256||!businessWorkerRequiredProofs.every(name=>item.passed.includes(name)))return false;
  if(Object.keys(item.companies).length!==5||new Set(Object.values(item.companies)).size!==5||!workerCompanyNames.every(name=>!!item.companies[name]&&item.companies[name]===input.companies[name]?.companyId&&item.nativeSourceSnapshots[name]?.beforeSha256===item.nativeSourceSnapshots[name]?.afterSha256&&!!item.nativeSourceSnapshots[name]))return false;
  const enabled=input.companies.enabled,recipients=item.currentRecipients,restart=item.restart;
  if(recipients.reviewerId!==input.currentRecipients.owner||recipients.recoveredReviewerId!==input.currentRecipients.revoked||recipients.revokedAssignment!==input.currentRecipients.revokedAssignment)return false;
  if(first.alerts.length!==2||new Set(first.alerts.map(row=>row.id)).size!==2||new Set(first.alerts.map(row=>row.hold_id)).size!==2||!first.alerts.every(row=>enabled.holds.includes(row.hold_id))||JSON.stringify(first.alerts.map(row=>row.phase).sort())!==JSON.stringify(["due_soon","overdue"])||JSON.stringify(after.alerts)!==JSON.stringify(first.alerts))return false;
  if(first.recipients.length!==2||after.recipients.length!==4||recipients.reviewerId===recipients.recoveredReviewerId||recipients.revokedAssignment===recipients.replacementAssignment)return false;
  for(const alert of first.alerts){const original=first.recipients.filter(row=>row.record_id===alert.id),current=after.recipients.filter(row=>row.record_id===alert.id);if(original.length!==1||original[0].user_id!==recipients.reviewerId||JSON.stringify(current.map(row=>row.user_id).sort())!==JSON.stringify([recipients.reviewerId,recipients.recoveredReviewerId].sort()))return false;}
  if(first.reservations.length!==5||new Set(first.reservations.map(row=>`${row.record_type}:${row.record_id}:${row.user_id}`)).size!==5||JSON.stringify(first.reservations)!==JSON.stringify(after.reservations))return false;
  for(const [type,id,count] of [["commerce_order",enabled.salesId,1],["field_service_order",enabled.fieldId,2],["manufacturing_order",enabled.productionId,2]] as const)if(first.reservations.filter(row=>row.record_type===type&&row.record_id===id).length!==count)return false;
  if(first.mail.status!=="sent"||first.mail.attempts!==1||first.mail.provider_message_id!==`sandbox-${enabled.messageId}`||JSON.stringify(first.mail)!==JSON.stringify(after.mail))return false;
  if(Object.keys(item.observations.blocked).length!==4||!workerCompanyNames.filter(name=>name!=="enabled").every(name=>{const row=item.observations.blocked[name];return row&&row.alerts.length===0&&row.recipients.length===0&&row.reservations.length===0&&row.mail.status==="pending"&&row.mail.attempts===0&&row.mail.provider_message_id===null;}))return false;
  return new Set(restart.workerPids).size>=2&&restart.firstWorkerPid!==restart.restartedWorkerPid&&restart.workerPids.includes(restart.firstWorkerPid)&&restart.workerPids.includes(restart.restartedWorkerPid)&&restart.firstHeartbeat>restart.preStartHeartbeat&&restart.secondHeartbeat>restart.firstHeartbeat;
}
export function financialContinuityEvidenceValid(value:unknown,release:CompiledEvidenceIdentity) {
  const parsed=financial.safeParse(value);if(!parsed.success)return false;const item=parsed.data;
  const month=(text:string)=>Number(text.slice(0,4))*12+Number(text.slice(5));
  return item.sourceFingerprint===release.sourceFingerprint&&item.softwareVersion===release.softwareVersion&&item.fileManifestSha256===release.fileManifestSha256
    &&item.currentYear===item.comparativeYear+1&&item.closedMonths.every((text,index)=>Number(text.slice(0,4))===item.currentYear&&(!index||month(text)===month(item.closedMonths[index-1])+1))
    &&[item.sourceJournals,item.sourceInvoices,item.postedYearCloseJournalIds,item.issuedStatementIds].every(values=>new Set(values).size===values.length)
    &&item.originalSha256!==item.amendmentSha256&&item.closingProfitMinor===item.beforeTransferProfitMinor
    &&item.currentAssetsMinor===item.currentLiabilitiesMinor+item.currentEquityMinor
    &&new Set(item.subledgerAssertions.map(row=>row.kind)).size>=2
    &&item.subledgerAssertions.every(row=>/^-?\d+$/.test(String(row.ledgerMinor))&&String(row.ledgerMinor)===String(row.sourceMinor));
}

/** A ready summary cannot conceal an unknown owner or mismatched physical file. */
export function attachmentReconciliationEvidenceValid(value:unknown,sourceFingerprint:string) {
  const empty=z.array(z.unknown()).length(0),reconciliation=z.object({unavailable:z.literal(0),sealedMalware:z.literal(0),quotaDeltaBytes:z.literal(0),duplicateLedgerObjects:z.literal(0),orphanObjects:empty,checksumMismatches:empty,missingObjects:empty,sizeMismatches:empty,missingLedger:empty});
  const parsed=z.object({sourceFingerprint:digest,reconciledAt:z.number().int().positive(),gateReady:z.literal(true),apply:z.literal(true),final:z.literal(true),scan:z.literal(true),verifyChecksums:z.literal(true),freezeConfirmed:z.literal(true),
    scanner:z.object({ready:z.literal(true),enabled:z.literal(true),engine:z.string().regex(/^ClamAV 1\.5\.4\//),definitionUpdatedAt:z.number().int().positive()}),
    preview:z.object({ready:z.literal(true)}),billing:z.object({ready:z.literal(true),scanReady:z.literal(true),reconciliationReady:z.literal(true),pending:z.literal(0),sealedMalware:z.literal(0),reconciledAt:z.number().int().positive()}),billingReconciliation:z.object({ready:z.literal(true),uploadFrozen:z.literal(true),checksumsVerified:z.literal(true),report:reconciliation}),
    results:z.array(z.object({holdingId:z.enum(['h001','h002','h003']),backfill:z.object({unknown:empty,legacySanitizedOnly:empty}),specialized:z.object({unknown:empty}),
      reconciliation:reconciliation.extend({unknownOwnership:z.literal(0),legacySanitizedOnly:z.literal(0),unknownOwnershipSources:empty,unavailableSources:empty,crossCompanyLinks:empty,crossCompanyIntents:empty,crossCompanyStorage:empty,crossCompanyDerivatives:empty,crossCompanyPublications:empty})})).length(3),
  }).safeParse(value);
  if(!parsed.success)return false;const item=parsed.data;
  return item.sourceFingerprint===sourceFingerprint&&new Set(item.results.map(row=>row.holdingId)).size===3
    &&item.scanner.definitionUpdatedAt<=item.reconciledAt&&item.reconciledAt-item.scanner.definitionUpdatedAt<=24*60*60_000
    &&item.billing.reconciledAt<=item.reconciledAt;
}
