import { afterAll, beforeAll, it, expect, vi } from "vitest";
import { readFile, rm } from "node:fs/promises";
import { createHash } from "node:crypto";
import { env, applyMigrations } from "../../support/runtime-env";
const session = vi.hoisted(() => ({
  role: "holding_owner",
  holdingId: "billing_api_holding",
  platformRole: "platform_admin",
  dataDir: `.runtime/tests/billing-api-${crypto.randomUUID()}`,
  scannerReady: true,
  scannerGate: null as string|null,
}));
const minioEnvironment={
  endpoint:process.env.ERPX_MINIO_ENDPOINT,
  bucket:process.env.ERPX_MINIO_BUCKET,
  attachmentsV2:process.env.ERPX_ATTACHMENTS_V2,
};
const validPngFixture=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=","base64");
vi.mock("@erpx/server-identity_access/public/authorization", () => ({
  requireWorkContext: async (slug: string) => {
    if (slug !== "billing-api") throw new Error("FORBIDDEN");
    return {
      role: session.role,
      holdingId: session.holdingId,
      userId: "test-owner",
      email: "owner@example.test",
    };
  },
  requirePlatformContext: async () => ({
    role: session.platformRole,
    userId: "test-admin",
    email: "admin@example.test",
  }),
  requireManageErpxContext: async () => ({
    platform: {
      role: session.platformRole,
      userId: "test-admin",
      email: "admin@example.test",
    },
  }),
}));
vi.mock("@erpx/server-operations/public/staff-policy", () => ({
  requireCompanyAuthority: async () => {},
}));
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({
  attachmentScannerReadiness:async(environment:NodeJS.ProcessEnv=process.env)=>{session.scannerGate=environment.ERPX_ATTACHMENTS_V2??null;return {enabled:true,ready:session.scannerReady,engine:"ClamAV billing test",definitionUpdatedAt:Date.now(),error:session.scannerReady?null:"CLAMD_UNAVAILABLE"};},
  scanAttachmentBytes:async()=>({clean:true,signature:null,engine:"ClamAV billing test",definitionUpdatedAt:Date.now()}),
}));
vi.mock("@erpx/server-files/public/attachments/preview",()=>({
  detectedAttachmentMediaType:(_bytes:Uint8Array,declared:string)=>declared,
}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  runtimeEnv: async () => ({
    ...env,
    ERPX_DATA_DIR: session.dataDir,
    ERPX_PUBLIC_ORIGIN: "http://localhost:3100",
  }),
  holdingResources: async () => ({ write: env.H001_DB }),
}));
import { billingHandler } from "@erpx/server-billing/public/billing-api";
import {
  billingEvidenceReadiness,
  billingEvidenceStore,
  drainBillingEvidenceScans,
  reconcileBillingEvidenceStorage,
  recordBillingEvidenceReconciliation,
} from "@erpx/server-billing/public/billing-attachments";
const customer = billingHandler(false),
  platform = billingHandler(true);
const ctx = (path: string[] = []) => ({
  params: Promise.resolve({ holdingSlug: "billing-api", path }),
});
beforeAll(async () => {
  delete process.env.ERPX_MINIO_ENDPOINT;
  delete process.env.ERPX_MINIO_BUCKET;
  delete process.env.ERPX_ATTACHMENTS_V2;
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await env.CONTROL_DB.pool.query(
    "INSERT INTO holding_bindings VALUES('billing_api_holding','billing-api','API test','W_BILL_API','R_BILL_API','F_BILL_API','active')",
  );
});
afterAll(async () => {
  await rm(session.dataDir, { recursive: true, force: true });
  if(minioEnvironment.endpoint)process.env.ERPX_MINIO_ENDPOINT=minioEnvironment.endpoint;
  else delete process.env.ERPX_MINIO_ENDPOINT;
  if(minioEnvironment.bucket)process.env.ERPX_MINIO_BUCKET=minioEnvironment.bucket;
  else delete process.env.ERPX_MINIO_BUCKET;
  if(minioEnvironment.attachmentsV2)process.env.ERPX_ATTACHMENTS_V2=minioEnvironment.attachmentsV2;
  else delete process.env.ERPX_ATTACHMENTS_V2;
});
it("allows owner reads, denies ordinary users and cross-Holding URL access", async () => {
  expect(
    (
      await customer(
        new Request(
          "http://localhost:3100/api/v1/holdings/billing-api/billing",
        ),
        ctx(),
      )
    ).status,
  ).toBe(200);
  session.role = "employee";
  expect(
    (
      await customer(
        new Request(
          "http://localhost:3100/api/v1/holdings/billing-api/billing",
        ),
        ctx(),
      )
    ).status,
  ).toBe(403);
  session.role = "holding_owner";
  expect(
    (
      await customer(
        new Request("http://localhost:3100/api/v1/holdings/other/billing"),
        { params: Promise.resolve({ holdingSlug: "other" }) },
      )
    ).status,
  ).toBe(403);
  expect(
    (
      await customer(
        new Request(
          "http://localhost:3100/api/v1/holdings/billing-api/billing/evidence/guessed",
        ),
        ctx(["evidence", "guessed"]),
      )
    ).status,
  ).toBe(403);
});
it("protects Platform mutations from auditors and cross-origin requests", async () => {
  session.platformRole = "platform_auditor";
  const request = () =>
    new Request(
      "http://localhost:3100/api/v1/platform/billing?holdingId=billing_api_holding",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          action: "complete-costs",
          version: 0,
          reason: "Test review",
        }),
      },
    );
  expect((await platform(request(), ctx())).status).toBe(403);
  session.platformRole = "platform_admin";
  const evil = new Request(request(), {
    headers: {
      origin: "https://evil.example",
      "Content-Type": "application/json",
    },
  });
  expect((await platform(evil, ctx())).status).toBe(403);
  expect((await platform(request(), ctx())).status).toBe(422);
  expect(
    (
      await platform(
        new Request(
          "http://localhost:3100/api/v1/platform/billing/evidence/other?holdingId=billing_api_holding",
        ),
        ctx(["evidence", "other"]),
      )
    ).status,
  ).toBe(404);
});
it("stores evidence privately, retries uploads safely and prevents evidence access under another Holding", async () => {
  await env.CONTROL_DB.pool.query(
    "INSERT INTO billing_accounts(holding_id,created_at) VALUES($1,1) ON CONFLICT DO NOTHING",
    [session.holdingId],
  );
  const key = crypto.randomUUID(),
    bytes = validPngFixture;
  const upload = () =>
    platform(
      new Request(
        "http://localhost:3100/api/v1/platform/billing/evidence?holdingId=billing_api_holding",
        {
          method: "POST",
          headers: {
            "Content-Type": "image/png",
            "X-Filename": "fixture.png",
            "Idempotency-Key": key,
          },
          body: bytes,
        },
      ),
      ctx(["evidence"]),
    );
  const first = await upload();
  expect(first.status).toBe(202);
  const firstPayload=(await first.json()).data as {id:string;state:string},id=firstPayload.id;
  expect(firstPayload.state).toBe("quarantined");
  const replay=await upload();
  expect(replay.status).toBe(202);
  expect((await replay.json()).data).toEqual({id,state:"quarantined"});
  expect((await platform(new Request(`http://localhost:3100/api/v1/platform/billing/evidence/${id}?holdingId=billing_api_holding`),ctx(["evidence",id]))).status).toBe(423);
  const runtime={...env,ERPX_DATA_DIR:session.dataDir,ERPX_PUBLIC_ORIGIN:"http://localhost:3100"};
  expect(await drainBillingEvidenceScans(runtime,10)).toBe(1);
  const file = await platform(
    new Request(
      `http://localhost:3100/api/v1/platform/billing/evidence/${id}?holdingId=billing_api_holding`,
    ),
    ctx(["evidence", id]),
  );
  expect(file.status).toBe(200);
  expect(file.headers.get("Cache-Control")).toBe("private, no-store");
  expect(file.headers.get("Content-Disposition")).toContain("attachment");
  expect(Buffer.from(await file.arrayBuffer())).toEqual(bytes);
  await env.CONTROL_DB.pool.query(
    "INSERT INTO holding_bindings VALUES('billing_other','billing-other','Other','W_B_OTHER','R_B_OTHER','F_B_OTHER','active')",
  );
  expect(
    (
      await platform(
        new Request(
          `http://localhost:3100/api/v1/platform/billing/evidence/${id}?holdingId=billing_other`,
        ),
        ctx(["evidence", id]),
      )
    ).status,
  ).toBe(404);
  expect(
    (
      await customer(
        new Request(
          `http://localhost:3100/api/v1/holdings/billing-api/billing/evidence/${id}`,
        ),
        ctx(["evidence", id]),
      )
    ).status,
  ).toBe(403);
});

it("replays legacy Billing evidence across the global gate without creating a quarantine orphan",async()=>{
  const runtime={...env,ERPX_DATA_DIR:session.dataDir,ERPX_PUBLIC_ORIGIN:"http://localhost:3100"},store=billingEvidenceStore(runtime),
    key=crypto.randomUUID(),bytes=validPngFixture,
    id=createHash("sha256").update(`${session.holdingId}:test-admin:${key}`).digest("hex"),
    objectKey=`${session.holdingId}/${id}`,now=Date.now();
  const sha=createHash("sha256").update(bytes).digest("hex");
  await store.put(objectKey,bytes,{customMetadata:{sha256:sha,state:"available"}});
  await env.CONTROL_DB.pool.query(`INSERT INTO billing_evidence(id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state,scanned_at,scanner_definition_at)
    VALUES($1,$2,'legacy.png','image/png',$3,$4,'test-admin',$5,$6,'image/png','available',$5,$5)`,[id,session.holdingId,bytes.length,sha,now,objectKey]);
  await env.CONTROL_DB.pool.query("INSERT INTO billing_evidence_storage_ledger(id,evidence_id,holding_id,object_key,bytes,state,created_at,updated_at) VALUES($1,$2,$3,$4,$5,'stored',$6,$6)",[`billing-storage-${id}`,id,session.holdingId,objectKey,bytes.length,now]);
  process.env.ERPX_ATTACHMENTS_V2="true";
  try{
    const response=await platform(new Request("http://localhost:3100/api/v1/platform/billing/evidence?holdingId=billing_api_holding",{method:"POST",headers:{"Content-Type":"image/png","X-Filename":"legacy.png","Idempotency-Key":key},body:bytes}),ctx(["evidence"]));
    expect(response.status).toBe(200);
    expect((await response.json()).data).toEqual({id,state:"available"});
    expect(await store.head(objectKey)).not.toBeNull();
    expect(await store.head(`${session.holdingId}/quarantine/${id}/original`)).toBeNull();
    expect((await env.CONTROL_DB.pool.query("SELECT count(*)::int count FROM billing_evidence_storage_ledger WHERE evidence_id=$1",[id])).rows[0].count).toBe(1);
  }finally{delete process.env.ERPX_ATTACHMENTS_V2;}
});

it("quarantines Billing evidence under V2 and serves it only after the shared scan lifecycle",async()=>{
  process.env.ERPX_ATTACHMENTS_V2="true";
  try{
    const bytes=validPngFixture,key=crypto.randomUUID();
    const uploaded=await platform(new Request("http://localhost:3100/api/v1/platform/billing/evidence?holdingId=billing_api_holding",{method:"POST",headers:{"Content-Type":"image/png","X-Filename":"quarantined.png","Idempotency-Key":key},body:bytes}),ctx(["evidence"]));
    if(uploaded.status!==202)throw new Error(`Billing V2 upload failed: ${uploaded.status} ${await uploaded.text()}`);
    const payload=await uploaded.json() as {data:{id:string;state:string}};
    expect(payload.data.state).toBe("quarantined");
    const before=await platform(new Request(`http://localhost:3100/api/v1/platform/billing/evidence/${payload.data.id}?holdingId=billing_api_holding`),ctx(["evidence",payload.data.id]));
    expect(before.status).toBe(423);
    const runtime={...env,ERPX_DATA_DIR:session.dataDir,ERPX_PUBLIC_ORIGIN:"http://localhost:3100"};
    expect(await drainBillingEvidenceScans(runtime,10)).toBe(1);
    const after=await platform(new Request(`http://localhost:3100/api/v1/platform/billing/evidence/${payload.data.id}?holdingId=billing_api_holding`),ctx(["evidence",payload.data.id]));
    expect(after.status).toBe(200);
    expect(await env.CONTROL_DB.pool.query("SELECT count(*)::int count FROM billing_evidence_access_events WHERE evidence_id=$1 AND outcome='allowed'",[payload.data.id]).then(result=>result.rows[0].count)).toBe(1);
    await expect(env.CONTROL_DB.pool.query("DELETE FROM billing_evidence_events WHERE evidence_id=$1",[payload.data.id])).rejects.toThrow(/BILLING_EVIDENCE_AUDIT_APPEND_ONLY/);
  }finally{delete process.env.ERPX_ATTACHMENTS_V2;}
});

it("blocks Billing evidence intake during rollback read-only and the final upload freeze",async()=>{
  const request=()=>new Request("http://localhost:3100/api/v1/platform/billing/evidence?holdingId=billing_api_holding",{
    method:"POST",headers:{"Content-Type":"image/png","X-Filename":"blocked.png","Idempotency-Key":crypto.randomUUID()},
    body:Buffer.from("blocked"),
  });
  process.env.ERPX_ATTACHMENTS_READ_ONLY="true";
  try{
    const response=await platform(request(),ctx(["evidence"]));
    expect(response.status).toBe(503);
    expect((await response.json()).error.code).toBe("ATTACHMENTS_READ_ONLY");
  }finally{delete process.env.ERPX_ATTACHMENTS_READ_ONLY;}
  process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE="true";
  try{
    const response=await platform(request(),ctx(["evidence"]));
    expect(response.status).toBe(503);
    expect((await response.json()).error.code).toBe("ATTACHMENT_UPLOAD_FREEZE");
  }finally{delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;}
});

it("fails Billing intake closed against scanner readiness even while the global attachment gate is off",async()=>{
  const key=crypto.randomUUID(),bytes=validPngFixture,
    id=createHash("sha256").update(`${session.holdingId}:test-admin:${key}`).digest("hex");
  delete process.env.ERPX_ATTACHMENTS_V2;
  session.scannerReady=false;
  session.scannerGate=null;
  try{
    const response=await platform(new Request("http://localhost:3100/api/v1/platform/billing/evidence?holdingId=billing_api_holding",{method:"POST",headers:{"Content-Type":"image/png","X-Filename":"scanner-down.png","Idempotency-Key":key},body:bytes}),ctx(["evidence"]));
    expect(response.status).toBe(503);
    expect((await response.json()).error.code).toBe("ATTACHMENT_SCANNER_UNAVAILABLE");
    expect(session.scannerGate).toBe("true");
    expect((await env.CONTROL_DB.pool.query("SELECT count(*)::int count FROM billing_evidence WHERE id=$1",[id])).rows[0].count).toBe(0);
  }finally{session.scannerReady=true;}
});

it("reconciles Billing objects, checksums, orphans and quota before recording the final gate",async()=>{
  const runtime={...env,ERPX_DATA_DIR:session.dataDir,ERPX_PUBLIC_ORIGIN:"http://localhost:3100"},
    store=billingEvidenceStore(runtime),
    bytes=validPngFixture,
    key=crypto.randomUUID();
  const uploaded=await platform(new Request("http://localhost:3100/api/v1/platform/billing/evidence?holdingId=billing_api_holding",{
    method:"POST",headers:{"Content-Type":"image/png","X-Filename":"reconcile.png","Idempotency-Key":key},body:bytes,
  }),ctx(["evidence"]));
  expect(uploaded.status).toBe(202);
  const evidenceId=(await uploaded.json()).data.id as string,
    evidence=(await env.CONTROL_DB.pool.query("SELECT object_key,size FROM billing_evidence WHERE id=$1",[evidenceId])).rows[0] as {object_key:string;size:number};
  expect(await drainBillingEvidenceScans(runtime,10)).toBe(1);
  expect((await reconcileBillingEvidenceStorage(runtime,{verifyChecksums:true})).missingObjects).not.toContain(evidenceId);

  const orphanKey=`orphan/${crypto.randomUUID()}`;
  await store.put(orphanKey,Buffer.from("orphan"));
  expect((await reconcileBillingEvidenceStorage(runtime,{verifyChecksums:true})).orphanObjects).toContain(orphanKey);
  await store.delete(orphanKey);

  await env.CONTROL_DB.pool.query("UPDATE billing_evidence_storage_ledger SET bytes=bytes+1 WHERE evidence_id=$1",[evidenceId]);
  const quotaMismatch=await reconcileBillingEvidenceStorage(runtime,{verifyChecksums:true});
  expect(quotaMismatch.missingLedger).toContain(evidenceId);
  expect(quotaMismatch.quotaDeltaBytes).toBe(1);
  await env.CONTROL_DB.pool.query("UPDATE billing_evidence_storage_ledger SET bytes=$2 WHERE evidence_id=$1",[evidenceId,evidence.size]);

  await store.put(evidence.object_key,Buffer.alloc(bytes.length,0));
  expect((await reconcileBillingEvidenceStorage(runtime,{verifyChecksums:true})).checksumMismatches).toContain(evidenceId);
  await store.put(evidence.object_key,bytes);
  expect((await reconcileBillingEvidenceStorage(runtime,{verifyChecksums:true})).checksumMismatches).not.toContain(evidenceId);

  process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE="true";
  try{
    const receipt=await recordBillingEvidenceReconciliation(runtime,{mode:"final",verifyChecksums:true,scanner:{ready:true,enabled:true,engine:"ClamAV billing test",definitionUpdatedAt:Date.now()}});
    expect(receipt.ready,JSON.stringify(receipt.report)).toBe(true);
    expect((await billingEvidenceReadiness(runtime)).reconciliationReady).toBe(true);
    await expect(env.CONTROL_DB.pool.query("UPDATE billing_evidence_reconciliation_runs SET ready=false WHERE id=$1",[receipt.id])).rejects.toThrow(/BILLING_EVIDENCE_AUDIT_APPEND_ONLY/);
  }finally{delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;}
});

it("upgrades a pre-0020 Billing evidence row without assuming it was already scanned",async()=>{
  const schema=`billing_pre0020_${crypto.randomUUID().replaceAll("-","")}`,client=await env.CONTROL_DB.pool.connect();
  try{
    await client.query(`CREATE SCHEMA ${schema}`);
    await client.query(`SET search_path TO ${schema},public`);
    await client.query("CREATE TABLE billing_accounts(holding_id TEXT PRIMARY KEY)");
    await client.query("CREATE TABLE billing_evidence(id TEXT PRIMARY KEY,holding_id TEXT NOT NULL REFERENCES billing_accounts,filename TEXT NOT NULL,mime TEXT NOT NULL,size BIGINT NOT NULL,sha256 TEXT NOT NULL,actor TEXT NOT NULL,created_at BIGINT NOT NULL)");
    await client.query("INSERT INTO billing_accounts VALUES('legacy-holding')");
    await client.query("INSERT INTO billing_evidence VALUES('legacy-evidence','legacy-holding','legacy.pdf','application/pdf',42,$1,'legacy-actor',123),('legacy-malware','legacy-holding','malware.pdf','application/pdf',42,$1,'legacy-actor',124),('legacy-error','legacy-holding','error.pdf','application/pdf',42,$1,'legacy-actor',125)",["b".repeat(64)]);
    await client.query(await readFile("migrations/postgres/control/0020_billing_attachment_lifecycle.sql","utf8"));
    await client.query(await readFile("migrations/postgres/control/0021_billing_attachment_reconciliation.sql","utf8"));
    expect((await client.query("SELECT object_key,source_state FROM billing_evidence WHERE id='legacy-evidence'")).rows[0]).toEqual({object_key:"legacy-holding/legacy-evidence",source_state:"legacy_unscanned"});
    expect((await client.query("SELECT attempt,state FROM billing_evidence_scan_attempts WHERE evidence_id='legacy-evidence'")).rows[0]).toEqual({attempt:1,state:"pending"});
    expect((await client.query("SELECT count(*)::int count FROM billing_evidence_storage_ledger")).rows[0].count).toBe(3);
    await client.query("UPDATE billing_evidence SET source_state='scanning' WHERE id IN ('legacy-malware','legacy-error')");
    await client.query("UPDATE billing_evidence SET source_state='malware_rejected',detected_mime='application/pdf',scanned_at=200,scanner_definition_at=199 WHERE id='legacy-malware'");
    await client.query("UPDATE billing_evidence SET source_state='scan_error' WHERE id='legacy-error'");
    await expect(client.query("UPDATE billing_evidence SET source_state='destroyed',destroyed_at=300 WHERE id='legacy-malware'")).rejects.toThrow(/BILLING_EVIDENCE_SECURITY_DESTRUCTION_POLICY_REQUIRED/);
    await expect(client.query("UPDATE billing_evidence SET source_state='destroyed',destroyed_at=300 WHERE id='legacy-error'")).rejects.toThrow(/BILLING_EVIDENCE_SECURITY_DESTRUCTION_POLICY_REQUIRED/);
    await expect(client.query("DELETE FROM billing_evidence WHERE id='legacy-evidence'")).rejects.toThrow(/BILLING_EVIDENCE_SOURCE_IMMUTABLE/);
  }finally{
    await client.query("RESET search_path").catch(()=>undefined);
    await client.query(`DROP SCHEMA IF EXISTS ${schema} CASCADE`).catch(()=>undefined);
    client.release();
  }
});

it("returns the native measured target quote through the Console route with current Holding scope and validated month",async()=>{
  session.role="holding_owner";
  const response=await customer(new Request("http://localhost:3100/api/v1/holdings/billing-api/billing/target?month=2026-10"),ctx(["target"]));
  expect(response.status).toBe(200);expect(response.headers.get("cache-control")).toBe("private, no-store");
  const target=(await response.json()).data;expect(target).toMatchObject({holdingId:session.holdingId,month:"2026-10",state:"unassigned",assignment:null,invoice:null,totalSatang:null});expect(target.measuredEvidence.snapshot).toBeNull();expect(target.completeness.ready).toBe(false);
  session.platformRole="platform_auditor";
  const audit=await platform(new Request(`http://localhost:3100/api/v1/platform/billing/target?holdingId=${session.holdingId}&month=2026-10`),ctx(["target"]));expect(audit.status).toBe(200);expect((await audit.json()).data).toEqual(target);
  expect((await platform(new Request(`http://localhost:3100/api/v1/platform/billing/target?holdingId=${session.holdingId}&month=2026-99`),ctx(["target"]))).status).toBe(422);
  expect((await platform(new Request("http://localhost:3100/api/v1/platform/billing/target?holdingId=h003&month=2026-10"),ctx(["target"]))).status).toBe(403);
  expect((await platform(new Request("http://localhost:3100/api/v1/platform/billing/target?holdingId=unknown&month=2026-10"),ctx(["target"]))).status).toBe(404);
  session.role="employee";expect((await customer(new Request("http://localhost:3100/api/v1/holdings/billing-api/billing/target?month=2026-10"),ctx(["target"]))).status).toBe(403);session.role="holding_owner";session.platformRole="platform_admin";
  expect(await env.CONTROL_DB.prepare("SELECT count(*) n FROM billing_target_rating_runs WHERE holding_id=?").bind(session.holdingId).first("n")).toBe(0);
});
