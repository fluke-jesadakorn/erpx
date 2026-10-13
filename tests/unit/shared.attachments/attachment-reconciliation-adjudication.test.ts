import { describe,expect,it } from "vitest";
import { readFile } from "node:fs/promises";
import { buildAttachmentAdjudicationManifest,holdingAttachmentReconciliationReady,type AttachmentReconciliationSnapshot } from "../../../src/server/modules/files/public/attachments/reconciliation-adjudication";

function cleanStorage(){
  return {unknownOwnership:0,unavailable:0,sealedMalware:0,unknownOwnershipSources:[],unavailableSources:[],crossCompanyLinks:[],crossCompanyIntents:[],crossCompanyStorage:[],crossCompanyDerivatives:[],crossCompanyPublications:[],missingObjects:[],sizeMismatches:[],checksumMismatches:[],missingLedger:[],orphanObjects:[],quotaDeltaBytes:0,duplicateLedgerObjects:0};
}

function cleanSnapshot():AttachmentReconciliationSnapshot{
  return {
    apply:true,verifyChecksums:true,final:true,freezeConfirmed:true,gateReady:true,
    scanner:{enabled:true,ready:true,error:null},preview:{ready:true,error:null},billing:{ready:true},
    billingReconciliation:{ready:true,report:cleanStorage()},
    results:[{holdingId:"h001",backfill:{unknown:[]},specialized:{unknown:[]},reconciliation:cleanStorage()}],
  };
}

describe("attachment reconciliation adjudication manifest",()=>{
  it("merges one immutable source's blockers without guessing a replacement purpose",()=>{
    const snapshot=cleanSnapshot(),sourceId="attachment-legacy-leave-1",storage=snapshot.results[0]!.reconciliation;
    snapshot.gateReady=false;
    snapshot.results[0]!.backfill.unknown.push({id:sourceId,ownerType:"leave_request",ownerId:"leave-1",reason:"POLICY_UNKNOWN",targetCompanyId:"acme-co"});
    storage.unknownOwnership=1;storage.unknownOwnershipSources=[sourceId];storage.unavailable=1;
    storage.unavailableSources=[{id:sourceId,companyId:null,ownerType:"leave_request",ownerId:"leave-1",purpose:"leave evidence",mediaType:"application/pdf",bytes:1024,sourceState:"quarantined",scanAttempt:null,scanState:null,scanResult:null,scanError:null}];
    storage.missingLedger=[sourceId];storage.quotaDeltaBytes=-1024;
    snapshot.billing.ready=false;snapshot.billingReconciliation.ready=false;snapshot.billingReconciliation.report.orphanObjects=["billing/unknown-object"];

    const manifest=buildAttachmentAdjudicationManifest(snapshot),source=manifest.items.find(item=>item.subject.id===sourceId);
    expect(source?.blockers).toEqual(["company_scope_unassigned","missing_ledger","policy_unknown","unavailable_source"]);
    expect(source?.allowedResolutions).toContain("confirm canonical owner and an existing policy from source-record evidence");
    expect(source?.findings).toContainEqual(expect.objectContaining({blocker:"policy_unknown",details:expect.objectContaining({targetCompanyId:"acme-co"})}));
    expect(source?.decision).toEqual({status:"pending",resolution:null,evidenceReferences:[],decidedBy:null,decidedAt:null,notes:null});
    expect(manifest.items).toEqual(expect.arrayContaining([
      expect.objectContaining({scope:{kind:"holding",id:"h001"},subject:{kind:"scope",id:"quota"},blockers:["quota_delta"]}),
      expect.objectContaining({scope:{kind:"billing",id:"control"},subject:{kind:"storage_object",id:"billing/unknown-object"},blockers:["orphan_object"]}),
    ]));
    expect(manifest.computedGateReady).toBe(false);
    expect(JSON.stringify(manifest)).not.toContain("medical_certificate");
    expect(JSON.stringify(manifest)).not.toContain("leave_supporting_photo");
    expect(buildAttachmentAdjudicationManifest(snapshot)).toEqual(manifest);
  });

  it("requires a complete source inventory and flags security-scope corruption as critical",()=>{
    const snapshot=cleanSnapshot(),storage=snapshot.results[0]!.reconciliation;
    snapshot.gateReady=false;storage.unavailable=2;storage.crossCompanyLinks=["link-cross-company"];
    const manifest=buildAttachmentAdjudicationManifest(snapshot);
    expect(manifest.items).toEqual(expect.arrayContaining([
      expect.objectContaining({subject:{kind:"scope",id:"unavailable-source-inventory"},blockers:["unavailable_inventory_incomplete"],blocksGate:true}),
      expect.objectContaining({subject:{kind:"legacy_record",id:"link-cross-company"},severity:"critical",blockers:["cross_company_link"],blocksGate:true}),
    ]));
    expect(manifest.prohibitedShortcuts).toContain("copy or link evidence across Companies");
    expect(holdingAttachmentReconciliationReady(snapshot.results[0]!)).toBe(false);
  });

  it("produces an empty, gate-ready worklist only for a frozen verified clean snapshot",()=>{
    const snapshot=cleanSnapshot(),manifest=buildAttachmentAdjudicationManifest(snapshot);
    expect(holdingAttachmentReconciliationReady(snapshot.results[0]!)).toBe(true);
    expect(manifest).toMatchObject({schemaVersion:1,readOnly:true,reportedGateReady:true,computedGateReady:true,summary:{items:0,blockingItems:0,reviewOnlyItems:0,blockerCounts:{}},items:[]});

    snapshot.final=false;snapshot.gateReady=false;
    const nonFinal=buildAttachmentAdjudicationManifest(snapshot);
    expect(nonFinal.items).toContainEqual(expect.objectContaining({subject:{kind:"scope",id:"final-reconciliation"},blockers:["final_reconciliation_required"]}));
    expect(nonFinal.computedGateReady).toBe(false);
  });

  it("streams large reconciliation JSON instead of using Bun's truncated console output",async()=>{
    const reconciler=await readFile("scripts/local/reconcile-attachments.ts","utf8");
    expect(reconciler).toContain("process.stdout.write");
    expect(reconciler).not.toContain("console.log(JSON.stringify");
  });
});
