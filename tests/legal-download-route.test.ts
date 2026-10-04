import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import { PDFDocument } from "pdf-lib";
import type { WorkContext } from "../src/server/authorization";
import type { PortalActor } from "../src/server/legal/portal";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { access } from "../src/server/legal/access";
import { detail, legalCommand } from "../src/server/legal/service";
import { uploadDocument } from "../src/server/legal/documents";

// Only the authenticated request boundary and unavailable external scanner are
// fixtures. The routes, owner commands, ACLs and canonical private files are real.
const session = vi.hoisted(() => ({
  actor: null as WorkContext | null,
  portal: null as PortalActor | null,
}));
vi.mock("../src/server/authorization", async original => ({
  ...await original<typeof import("../src/server/authorization")>(),
  requireWorkContext: async () => session.actor!,
}));
vi.mock("../src/server/runtime", async original => ({
  ...await original<typeof import("../src/server/runtime")>(),
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("../src/server/legal/portal", async original => ({
  ...await original<typeof import("../src/server/legal/portal")>(),
  portalContext: async () => ({ write: env.H001_DB, files: env.H001_FILES, actor: session.portal! }),
}));
vi.mock("../src/server/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "isolated scanner fixture", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: true, signature: null, engine: "isolated scanner fixture", definitionUpdatedAt: Date.now() }),
}));
vi.mock("../src/server/attachments/preview", () => ({
  attachmentPreviewReadiness: async () => ({ ready: true, error: null }),
  detectedAttachmentMediaType: (bytes: Uint8Array, type: string) => type === "application/pdf" && Buffer.from(bytes.subarray(0, 5)).toString() === "%PDF-" ? type : null,
  sanitizedAttachmentPreview: async () => null,
}));

import { GET as ownerGet } from "../src/app/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route";
import { GET as portalGet } from "../src/app/api/legal-portal/[holdingSlug]/[[...path]]/route";

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
