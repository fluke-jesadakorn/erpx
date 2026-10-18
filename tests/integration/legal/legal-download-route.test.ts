import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import { PDFDocument } from "pdf-lib";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type { PortalActor } from "../../../src/server/modules/legal/public/legal/portal";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { access } from "../../../src/server/modules/legal/public/legal/access";
import { detail, legalCommand } from "../../../src/server/modules/legal/public/legal/service";
import { uploadDocument } from "../../../src/server/modules/legal/public/legal/documents";

// Only the authenticated request boundary and unavailable external scanner are
// fixtures. The routes, owner commands, ACLs and canonical private files are real.
const session = vi.hoisted(() => ({
  actor: null as WorkContext | null,
  portal: null as PortalActor | null,
}));
vi.mock("../../../src/server/modules/identity_access/public/authorization", async original => ({
  ...await original<typeof import("../../../src/server/modules/identity_access/public/authorization")>(),
  requireWorkContext: async () => session.actor!,
}));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", async original => ({
  ...await original<typeof import("../../../src/server/modules/runtime/adapters/runtime")>(),
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("../../../src/server/modules/legal/public/legal/portal", async original => ({
  ...await original<typeof import("../../../src/server/modules/legal/public/legal/portal")>(),
  portalContext: async () => ({ write: env.H001_DB, files: env.H001_FILES, actor: session.portal! }),
}));
// This owner suite keeps authentication explicit as a fixture boundary. The
// separate legal-portal-download-session suite uses actual credential/SSO APIs.
vi.mock("../../../src/server/modules/legal/public/portal-download-request", () => ({
  legalPortalDownloadRequest: async (_slug: string, shareId: string) => {
    const {portalDownload} = await import("../../../src/server/modules/legal/public/legal/portal");
    return portalDownload(env.H001_DB, env.H001_FILES, session.portal!, shareId);
  },
}));
vi.mock("../../../src/server/modules/files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "isolated scanner fixture", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: true, signature: null, engine: "isolated scanner fixture", definitionUpdatedAt: Date.now() }),
}));
vi.mock("../../../src/server/modules/files/public/attachments/preview", () => ({
  attachmentPreviewReadiness: async () => ({ ready: true, error: null }),
  detectedAttachmentMediaType: (bytes: Uint8Array, type: string) => type === "application/pdf" && Buffer.from(bytes.subarray(0, 5)).toString() === "%PDF-" ? type : null,
  sanitizedAttachmentPreview: async () => null,
}));

import { GET as ownerGet } from "../../../src/app/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route";
import { GET as portalGet } from "../../../src/app/api/legal-portal/[holdingSlug]/[[...path]]/route";

const db = env.H001_DB, key = () => crypto.randomUUID();
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
let itemId: string, documentId: string, bytes: Uint8Array;
beforeAll(() => applyMigrations(db, env.STAFF_TEST_MIGRATIONS));
beforeEach(async () => {
  f = await seedExpenseCompany(db, env.H001_FILES);
  f.manager = await f.member("document-owner", "legal_manager");
  f.tie = await f.member("independent-unassigned-reader", "legal_counsel");
  session.actor = f.manager;
  await legalCommand(db, f.manager, { action: "settings", enabled: true }, key());
  const created = await legalCommand(db, f.manager, {
    action: "create", item: { kind: "contract", classification: "confidential", title: "Fictional private agreement", ownerId: f.manager.userId, data: { contractType: "Synthetic", parties: "Company and fictional supplier" } },
  }, key());
  itemId = created.id;
  const pdf = await PDFDocument.create(); pdf.addPage([100, 100]); bytes = await pdf.save();
  await uploadDocument(db, env.H001_FILES, f.manager, itemId, 1, "fictional-original.pdf", "application/pdf", bytes, key());
  const document = (await detail(db, f.manager, itemId)).documents[0];
  documentId = document.id;
  expect(document.attachment_link_id).toBeTruthy();
  expect(document.checksum).toBe(createHash("sha256").update(bytes).digest("hex"));
});
async function command(action: string, extra: Record<string, unknown>) {
  return legalCommand(db, f.manager, { action, id: itemId, expectedVersion: (await access(db, f.manager, itemId)).version, ...extra }, key());
}
function ownerDownload() {
  return ownerGet(new Request(`http://localhost:3100/api/v1/holdings/test/legal/documents/${documentId}`), { params: Promise.resolve({ holdingSlug: "test", path: ["documents", documentId] }) });
}
function portalDownload(shareId: string) {
  return portalGet(new Request(`http://localhost:3100/api/legal-portal/test/${shareId}/download`), { params: Promise.resolve({ holdingSlug: "test", path: [shareId, "download"] }) });
}
async function denied(response: Response, code: string) {
  expect(response.status).toBe(404);
  expect(response.headers.get("content-disposition")).toBeNull();
  const body = await response.json(); expect(body).toEqual({ error: { code, message: expect.any(String) } });
  expect(body).not.toHaveProperty("data");
}
async function original(response: Response) {
  expect(response.status).toBe(200);
  expect(response.headers.get("content-type")).toBe("application/pdf");
  expect(response.headers.get("cache-control")).toBe("private, no-store");
  expect(response.headers.get("content-disposition")).toContain("attachment;");
  expect(Buffer.from(await response.arrayBuffer())).toEqual(Buffer.from(bytes));
}

it("returns native scoped 404 envelopes without bytes for confidential originals and honors current member withdrawal", async () => {
  session.actor = f.tie; await denied(await ownerDownload(), "LEGAL_NOT_FOUND");
  expect(await db.prepare("SELECT count(*) n FROM legal_access_events WHERE document_id=?").bind(documentId).first("n")).toBe(0);
  session.actor = f.manager; await original(await ownerDownload());
  await command("member", { userId: f.tie.userId, access: "read" });
  session.actor = f.tie; await original(await ownerDownload());
  await command("member", { userId: f.tie.userId, access: "remove" });
  await denied(await ownerDownload(), "LEGAL_NOT_FOUND");
  const foreign = await seedExpenseCompany(db, env.H001_FILES); session.actor = await foreign.member("other-company-legal", "legal_manager");
  await denied(await ownerDownload(), "LEGAL_NOT_FOUND");
  expect(await db.prepare("SELECT count(*) n FROM legal_access_events WHERE document_id=? AND action='document.export.original'").bind(documentId).first("n")).toBe(2);
  expect((await detail(db, f.manager, itemId)).documents[0]).toMatchObject({ id: documentId, checksum: createHash("sha256").update(bytes).digest("hex") });
});

it("returns native portal 404 envelopes without bytes for mismatched and revoked shares while preserving the exact authorized original", async () => {
  const email = `fictional-counsel-${key()}@example.test`;
  await command("share", { documentId, email, expiresAt: Date.now() + 86400000 });
  const shareId = (await detail(db, f.manager, itemId)).shares[0].id;
  session.portal = { userId: "isolated-external-counsel", email: "different@example.test", sessionId: "normal-boundary-fixture" };
  await denied(await portalDownload(shareId), "LEGAL_SHARE_UNAVAILABLE");
  session.portal = { ...session.portal, email }; await original(await portalDownload(shareId));
  await command("share.revoke", { shareId }); await denied(await portalDownload(shareId), "LEGAL_SHARE_UNAVAILABLE");
  expect(await db.prepare("SELECT count(*) n FROM legal_access_events WHERE document_id=? AND action='portal.download'").bind(documentId).first("n")).toBe(1);
  expect((await detail(db, f.manager, itemId)).shares[0].revoked_at).not.toBeNull();
  expect((await detail(db, f.manager, itemId)).documents[0].checksum).toBe(createHash("sha256").update(bytes).digest("hex"));
});


it("legalPortalRead orders a normal share withdrawal after buffered original bytes and the complete owned access audits", async () => {
  const email = `read-window-${key()}@example.test`;
  await command("share", { documentId, email, expiresAt: Date.now() + 86400000 });
  const shareId = (await detail(db, f.manager, itemId)).shares[0].id;
  session.portal = { userId: "external-read-window", email, sessionId: "normal-authenticated-request-fixture" };
  let entered!: () => void, release!: () => void;
  const enteredBytes = new Promise<void>(resolve => { entered = resolve; });
  const continueBytes = new Promise<void>(resolve => { release = resolve; });
  const originalGet = env.H001_FILES.get.bind(env.H001_FILES);
  const spy = vi.spyOn(env.H001_FILES, "get").mockImplementation(async objectKey => {
    const object = await originalGet(objectKey);
    if (!object) return object;
    return { ...object, arrayBuffer: async () => { entered(); await continueBytes; return object.arrayBuffer(); } };
  });
  const reading = portalDownload(shareId);
  let withdrawing: Promise<unknown> | undefined;
  try {
    await Promise.race([enteredBytes, reading.then(response => { throw Error(`Original bytes were not entered: ${response.status}`); })]);
    withdrawing = command("share.revoke", { shareId });
    let withdrawn = false;
    void withdrawing.then(() => { withdrawn = true; });
    let waiting = false;
    for (let attempt = 0; attempt < 100 && !withdrawn; attempt++) {
      waiting = !!await db.prepare("SELECT 1 FROM pg_locks WHERE relation='legal_items'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first();
      if (waiting) break;
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    expect({ waiting, withdrawn }).toEqual({ waiting: true, withdrawn: false });
  } finally {
    release(); await Promise.allSettled([reading, ...(withdrawing ? [withdrawing] : [])]); spy.mockRestore();
  }
  await original(await reading);
  await denied(await portalDownload(shareId), "LEGAL_SHARE_UNAVAILABLE");
  expect(await db.prepare("SELECT count(*)::int n FROM legal_access_events WHERE document_id=? AND action='portal.download'").bind(documentId).first("n")).toBe(1);
  expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE actor_id=? AND outcome='allowed'").bind(session.portal.userId).first("n")).toBe(1);
});


it("legalPortalRead rolls both original access audits and a deferred native owner configuration change back", async () => {
  const email = `deferred-read-${key()}@example.test`;
  await command("share", { documentId, email, expiresAt: Date.now() + 86400000 });
  const shareId = (await detail(db, f.manager, itemId)).shares[0].id;
  session.portal = { userId: "external-deferred-read", email, sessionId: "normal-authenticated-request-fixture" };
  const fault = `legal_portal_read_${key().replaceAll("-", "")}`;
  const enabled = await db.prepare("SELECT enabled FROM legal_settings WHERE company_id=?").bind(f.manager.companyId).first("enabled");
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE legal_settings SET enabled=NOT enabled WHERE company_id=(SELECT company_id FROM legal_items WHERE id=NEW.item_id);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON legal_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.action='portal.download') EXECUTE FUNCTION ${fault}()`);
  try {
    const response = await portalDownload(shareId);
    expect(response.status).toBe(409);
    expect(await response.json()).toMatchObject({error:{code:"LEGAL_SHARE_CHANGED"}});
  } finally { await db.exec(`DROP TRIGGER ${fault} ON legal_access_events;DROP FUNCTION ${fault}()`); }
  expect(await db.prepare("SELECT enabled FROM legal_settings WHERE company_id=?").bind(f.manager.companyId).first("enabled")).toBe(enabled);
  expect(await db.prepare("SELECT count(*)::int n FROM legal_access_events WHERE document_id=? AND action='portal.download'").bind(documentId).first("n")).toBe(0);
  expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE actor_id=?").bind(session.portal.userId).first("n")).toBe(0);
  await original(await portalDownload(shareId));
});

it("legalPortalRead keeps an expired share unavailable and rolls the buffered attachment audit back", async () => {
  const email = `expiring-read-${key()}@example.test`;
  await command("share", { documentId, email, expiresAt: Date.now() + 86400000 });
  const shareId = (await detail(db, f.manager, itemId)).shares[0].id;
  session.portal = { userId: "external-expiring-read", email, sessionId: "normal-authenticated-request-fixture" };
  // Represent the final half-second of a normally created retained share.
  await db.prepare("UPDATE legal_shares SET expires_at=?,version=version+1 WHERE id=?").bind(Date.now()+500,shareId).run();
  const originalGet = env.H001_FILES.get.bind(env.H001_FILES);
  const spy = vi.spyOn(env.H001_FILES, "get").mockImplementation(async objectKey => {
    const object = await originalGet(objectKey);
    if (!object) return object;
    return { ...object, arrayBuffer: async () => { await new Promise(resolve => setTimeout(resolve, 650)); return object.arrayBuffer(); } };
  });
  try { await denied(await portalDownload(shareId), "LEGAL_SHARE_UNAVAILABLE"); } finally { spy.mockRestore(); }
  expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE actor_id=?").bind(session.portal.userId).first("n")).toBe(0);
  expect(await db.prepare("SELECT count(*)::int n FROM legal_access_events WHERE document_id=? AND action='portal.download'").bind(documentId).first("n")).toBe(0);
});

it("legalPortalRead refuses a borrowed snapshot before private storage or audit access", async () => {
  const {portalDownload: read} = await import("../../../src/server/modules/legal/public/legal/portal");
  const actor = {userId:"borrowed-external",email:"borrowed@example.test",sessionId:"normal-authenticated-request-fixture"};
  const spy = vi.spyOn(env.H001_FILES,"get");
  try {
    await expect(db.transaction(tx => read(tx,env.H001_FILES,actor,"unknown"))).rejects.toMatchObject({status:503,code:"LEGAL_PORTAL_READ_TRANSACTION_REQUIRED"});
    expect(spy).not.toHaveBeenCalled();
  } finally { spy.mockRestore(); }
});


it.each(["unavailable", "integrity"] as const)("legalPortalDenial preserves the original current %s file denial audit without allowed bytes", async failure => {
  const email = `denied-read-${key()}@example.test`;
  await command("share", {documentId,email,expiresAt:Date.now()+86400000});
  const shareId=(await detail(db,f.manager,itemId)).shares[0].id;
  session.portal={userId:`external-denied-${failure}`,email,sessionId:"normal-authenticated-request-fixture"};
  const originalGet=env.H001_FILES.get.bind(env.H001_FILES),spy=vi.spyOn(env.H001_FILES,"get").mockImplementation(async objectKey=>{
    if(failure==="unavailable")return null;
    const object=await originalGet(objectKey);if(!object)return object;
    return {...object,arrayBuffer:async()=>new Uint8Array([1,2,3]).buffer};
  });
  const reason=failure==="unavailable"?"FILE_UNAVAILABLE":"LEGAL_INTEGRITY";
  try {
    const response=await portalDownload(shareId);
    expect(response.status).toBe(failure==="unavailable"?503:409);
    expect(response.headers.get("content-disposition")).toBeNull();
    expect(await response.json()).toMatchObject({error:{code:reason}});
  } finally {spy.mockRestore();}
  expect((await db.prepare("SELECT outcome,reason FROM attachment_access_events WHERE actor_id=? ORDER BY created_at").bind(session.portal.userId).all()).results).toEqual([{outcome:"denied",reason}]);
  expect(await db.prepare("SELECT count(*)::int n FROM legal_access_events WHERE document_id=? AND action='portal.download'").bind(documentId).first("n")).toBe(0);
  await original(await portalDownload(shareId));
});


it("legalPortalDetailRead keeps a normal share withdrawal after the original private detail and view audit", async()=>{
  const email=`detail-window-${key()}@example.test`;
  await command("share",{documentId,email,expiresAt:Date.now()+86400000});
  const shareId=(await detail(db,f.manager,itemId)).shares[0].id;
  const actor={userId:"external-detail-window",email,sessionId:"normal-authenticated-request-fixture"};
  const {portalDetail}=await import("../../../src/server/modules/legal/public/legal/portal");
  const fault=`legal_detail_window_${key().replaceAll("-","")}`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.7);RETURN NEW;END $$;CREATE TRIGGER ${fault} BEFORE INSERT ON legal_access_events FOR EACH ROW WHEN(NEW.action='portal.view') EXECUTE FUNCTION ${fault}()`);
  const reading=portalDetail(db,actor,shareId);let withdrawing:Promise<unknown>|undefined;
  try{
    let paused=false,finished=false;void reading.then(()=>{finished=true;});
    for(let attempt=0;attempt<100&&!finished;attempt++){paused=!!await db.prepare(`SELECT is_called FROM ${fault}`).first("is_called");if(paused)break;await new Promise(resolve=>setTimeout(resolve,5));}
    expect(paused).toBe(true);withdrawing=command("share.revoke",{shareId});let withdrawn=false;void withdrawing.then(()=>{withdrawn=true;});let waiting=false;
    for(let attempt=0;attempt<100&&!withdrawn;attempt++){waiting=!!await db.prepare("SELECT 1 FROM pg_locks WHERE relation='legal_items'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first();if(waiting)break;await new Promise(resolve=>setTimeout(resolve,5));}
    expect({waiting,withdrawn}).toEqual({waiting:true,withdrawn:false});
  }finally{await Promise.allSettled([reading,...(withdrawing?[withdrawing]:[])]);await db.exec(`DROP TRIGGER ${fault} ON legal_access_events;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  expect(await reading).toMatchObject({shareId,doc:{id:documentId,name:"fictional-original.pdf"}});
  await expect(portalDetail(db,actor,shareId)).rejects.toMatchObject({status:404,code:"LEGAL_SHARE_UNAVAILABLE"});
  expect(await db.prepare("SELECT count(*)::int n FROM legal_access_events WHERE document_id=? AND action='portal.view'").bind(documentId).first("n")).toBe(1);
});


it("legalPortalDetailRead rolls a deferred external comment and original view audit back before private detail escapes",async()=>{
  const email=`detail-comment-${key()}@example.test`;await command("share",{documentId,email,expiresAt:Date.now()+86400000});
  const shareId=(await detail(db,f.manager,itemId)).shares[0].id,actor={userId:"external-detail-comment",email,sessionId:"normal-authenticated-request-fixture"},fault=`legal_detail_comment_${key().replaceAll("-","")}`,commentId=key();
  const {portalDetail}=await import("../../../src/server/modules/legal/public/legal/portal");
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO legal_comments(id,item_id,document_id,body,visibility,created_by,created_at) VALUES('${commentId}',NEW.item_id,NEW.document_id,'New source after original detail snapshot','external',NEW.actor_id,NEW.created_at);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON legal_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.action='portal.view') EXECUTE FUNCTION ${fault}()`);
  try{await expect(portalDetail(db,actor,shareId)).rejects.toMatchObject({status:409,code:"LEGAL_SHARE_CHANGED"});}finally{await db.exec(`DROP TRIGGER ${fault} ON legal_access_events;DROP FUNCTION ${fault}()`);}
  expect(await db.prepare("SELECT 1 FROM legal_comments WHERE id=?").bind(commentId).first()).toBeNull();
  expect(await db.prepare("SELECT count(*)::int n FROM legal_access_events WHERE document_id=? AND action='portal.view'").bind(documentId).first("n")).toBe(0);
  expect(await portalDetail(db,actor,shareId)).toMatchObject({shareId,comments:[]});
});
