import { beforeEach,expect,it,vi } from "vitest";
import { readFile,readdir } from "node:fs/promises";

const state=vi.hoisted(()=>({
  enabled:false,
  appFrozen:false,
  scannerReady:true,
  scannerVersion:"1.5.4",
  scannerGate:null as string|null,
  previewReady:true,
  holdingFinal:false,
  heartbeatAt:0,
  jobsReady:true,
  jobsFrozen:false,
  billingScanReady:true,
  billingReconciliationReady:false,
}));

vi.mock("../src/server/modules/files/public/attachments/scanner",()=>({
  attachmentScannerReadiness:async(environment:NodeJS.ProcessEnv)=>{
    state.scannerGate=environment.ERPX_ATTACHMENTS_V2??null;
    return {enabled:true,ready:state.scannerReady,engine:state.scannerReady?`ClamAV ${state.scannerVersion}/test`:null,definitionUpdatedAt:state.scannerReady?Date.now():null,error:state.scannerReady?null:"CLAMD_UNAVAILABLE"};
  },
}));
vi.mock("../src/server/modules/files/public/attachments/preview",()=>({
  attachmentPreviewReadiness:async()=>({ready:state.previewReady,heifConvert:"/private/heif-convert",pdftoppm:"/private/pdftoppm",error:state.previewReady?null:"PREVIEW_UNAVAILABLE"}),
}));
vi.mock("../src/server/modules/files/public/attachments/feature",()=>({
  attachmentsV2Enabled:()=>state.enabled,
  attachmentUploadsFrozen:()=>state.appFrozen,
}));
vi.mock("../src/server/modules/billing/public/billing-attachments",()=>({
  billingEvidenceReadiness:async()=>({
    ready:state.billingScanReady&&state.billingReconciliationReady,
    scanReady:state.billingScanReady,
    reconciliationReady:state.billingReconciliationReady,
    pending:state.billingScanReady?0:1,
    sealedMalware:0,
    reconciledAt:state.billingReconciliationReady?Date.now():null,
  }),
}));
vi.mock("../src/server/modules/runtime/public/runtime",()=>({
  runtimeEnv:async()=>({
    CONTROL_DB:{prepare:()=>({first:async()=>({
      heartbeat_at:state.heartbeatAt,
      details_json:JSON.stringify({attachments:{ready:state.jobsReady,uploadFrozen:state.jobsFrozen}}),
    })})},
  }),
}));
vi.mock("../src/lib/holding-registry",()=>({
  resourceSlots:()=>[{id:"h001"}],
  registeredResources:()=>({write:{prepare:()=>({first:async()=>state.holdingFinal?{ready:true}:null})}}),
}));

vi.mock("../src/server/modules/files/public/attachments/readiness",async importActual=>{
  const actual=await importActual<typeof import("../src/server/modules/files/public/attachments/readiness")>();
  return {attachmentRuntimeReadiness:()=>actual.attachmentRuntimeReadiness({...process.env,ERPX_ATTACHMENT_UPLOAD_FREEZE:state.appFrozen?"true":"false"})};
});

import { GET } from "../src/app/api/health/route";

beforeEach(()=>{
  state.enabled=false;
  state.appFrozen=false;
  state.scannerReady=true;
  state.scannerVersion="1.5.4";
  state.scannerGate=null;
  state.previewReady=true;
  state.holdingFinal=false;
  state.heartbeatAt=Date.now();
  state.jobsReady=true;
  state.jobsFrozen=false;
  state.billingScanReady=true;
  state.billingReconciliationReady=false;
});

it("requires the canonical scanner for compatibility traffic while the UI gate is closed",async()=>{
  state.scannerReady=false;
  const response=await GET(),body=await response.json();
  expect(response.status).toBe(503);
  expect(state.scannerGate).toBe("true");
  expect(body.attachments.enabled).toBe(false);
  expect(body.attachments.scanner.ready).toBe(false);
  expect(body.attachments.preview).toEqual({ready:true,error:null});
  expect(JSON.stringify(body)).not.toContain("/private/");

  state.scannerReady=true;
  state.scannerVersion="1.5.3";
  const mismatched=await GET();
  expect(mismatched.status).toBe(503);
  expect((await mismatched.json()).attachments.scanner.error).toBe("CLAMAV_ENGINE_VERSION_MISMATCH");
});

it("keeps final reconciliation gate-specific but requires jobs and Billing scan compatibility",async()=>{
  const response=await GET(),body=await response.json();
  expect(response.status).toBe(200);
  expect(body.attachments.billing).toMatchObject({compatibilityReady:true,reconciliationReady:false});
  expect(body.attachments.migration).toEqual({ready:true,pendingHoldings:0});

  state.billingScanReady=false;
  expect((await GET()).status).toBe(503);
});

it("requires final Holding and Billing receipts before the global gate reports healthy",async()=>{
  state.enabled=true;
  expect((await GET()).status).toBe(503);
  state.holdingFinal=true;
  state.billingReconciliationReady=true;
  const response=await GET(),body=await response.json();
  expect(response.status).toBe(200);
  expect(body.attachments.migration).toEqual({ready:true,pendingHoldings:0});
  expect(body.attachments.billing.ready).toBe(true);
});

it("fails health when jobs is stale or did not restart with the same upload freeze",async()=>{
  state.heartbeatAt=Date.now()-45_001;
  expect((await GET()).status).toBe(503);
  state.heartbeatAt=Date.now();
  state.appFrozen=true;
  expect((await GET()).status).toBe(503);
  state.jobsFrozen=true;
  const response=await GET(),body=await response.json();
  expect(response.status).toBe(200);
  expect(body.attachments.jobs).toMatchObject({ready:true,attachmentReady:true,uploadFrozen:true});
});

it("pins the attachment migration order and local-only scanner manifest",async()=>{
  const holding=(await readdir("migrations/postgres/holding-write")).filter(name=>name.endsWith(".sql")).sort();
  const control=(await readdir("migrations/postgres/control")).filter(name=>name.endsWith(".sql")).sort();
  expect(holding.indexOf("0331_attachment_sources_links.sql")).toBeGreaterThan(-1);
  expect(holding.indexOf("0331_attachment_sources_links.sql")).toBeLessThan(holding.indexOf("0332_leave_evidence_policy.sql"));
  expect(holding.indexOf("0331_attachment_sources_links.sql")).toBeLessThan(holding.indexOf("0337_attachment_derivative_storage_ledger.sql"));
  expect(holding.indexOf("0339_legal_attachment_hold_race_guard.sql")).toBeLessThan(holding.indexOf("0340_attachment_jobs_storage_grant.sql"));
  const jobsStorageGrant=await readFile("migrations/postgres/holding-write/0340_attachment_jobs_storage_grant.sql","utf8");
  expect(jobsStorageGrant).toContain("GRANT INSERT ON TABLE attachment_storage_ledger");
  expect(control.indexOf("0020_billing_attachment_lifecycle.sql")).toBeLessThan(control.indexOf("0021_billing_attachment_reconciliation.sql"));
  const manifest=JSON.parse(await readFile("ops/macos/clamav/runtime-versions.json","utf8"));
  expect(manifest).toMatchObject({clamav:{version:"1.5.4"},libheif:{version:"1.23.5"},poppler:{version:"26.09.0"}});
  const config=await readFile("ops/macos/clamav/clamd.conf.example","utf8");
  expect(config).toContain("LocalSocket ");
  expect(config).not.toMatch(/^\s*(?:TCPSocket|TCPAddr)\s/m);
});

it("does not recreate ambiguous legacy Leave evidence in a fresh local seed",async()=>{
  const seed=await readFile("scripts/seed-local.ts","utf8");
  expect(seed).not.toContain('"leave evidence"');
  expect(seed).not.toContain('.bind(id, "leave_request", leaveId');
});
