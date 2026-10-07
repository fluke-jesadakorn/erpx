import { expect, it } from "vitest";
import { createCustomerPortalApplication, type PortalRepository } from "../../src/server/modules/customer_360/application/portal";
import { createCustomerPortalAttachmentApplication, type PortalAttachmentRepository } from "../../src/server/modules/customer_360/application/portal-attachments";
import { createCustomerPortalSessionApplication } from "../../src/server/modules/customer_360/application/portal-session";
import type { PortalBinding, PortalActionRow } from "../../src/server/modules/customer_360/domain/portal";

function repository<T extends object>(operations: Partial<T>): T { return new Proxy(operations as T, { get(target, property) { if (property === "then") return undefined; if (property in target) return Reflect.get(target, property); return () => { throw new Error(`unexpected operation: ${String(property)}`); }; } }); }
const error = (_status: number, code: string) => new Error(code);
const actor = { userId: "customer", email: "customer@example.test" }, input = { bindingId: "grant", expectedBindingVersion: 2, action: "request_support", note: "Help with delivery" }, key = "portal-key-1";
const binding: PortalBinding = { binding_id: "grant", company_id: "company", order_id: "order", customer_id: "party", email: actor.email, status: "approved", expires_at: 200, binding_version: 2, order_version: 3, number: "SO-1", state: "confirmed" };
const prior: PortalActionRow = { id: "original", binding_id: "grant", order_id: "order", action: "request_support", status: "recorded", created_at: 90, request_hash: "hash", user_id: actor.userId };
function portal(source: PortalRepository) { return createCustomerPortalApplication({ repository: source, clock: { now: () => 100 }, identifiers: { next: () => "identifier" }, hash: () => "hash", error }); }
function transaction(operations: Partial<PortalRepository>): PortalRepository { const source: PortalRepository = repository<PortalRepository>({ transaction: callback => callback(source), uniqueConflict: () => false, replay: async () => null, ...operations }); return source; }

it("a lost customer action response replays before accessing a now-revoked binding", async () => {
  const source = transaction({ replay: async () => prior });
  expect(await portal(source).customerPortalCommand(actor, input, key)).toMatchObject({ id: "original", createdAt: 90 });
});
it("another customer cannot reuse an existing command receipt", async () => {
  const source = transaction({ replay: async () => ({ ...prior, user_id: "another-customer" }) });
  await expect(portal(source).customerPortalCommand(actor, input, key)).rejects.toThrow("IDEMPOTENCY_CONFLICT");
});
it("revoked and stale customer grants stop before identity lookup or any write", async () => {
  await expect(portal(transaction({ binding: async () => ({ ...binding, status: "revoked" }) })).customerPortalCommand(actor, input, key)).rejects.toThrow("CUSTOMER_PORTAL_BINDING_EXPIRED");
  await expect(portal(transaction({ binding: async () => ({ ...binding, binding_version: 3 }) })).customerPortalCommand(actor, input, key)).rejects.toThrow("CUSTOMER_PORTAL_BINDING_CHANGED");
});
it("opening a document and verifying its identity is required before a portal command", async () => {
  const source = transaction({ binding: async () => binding, identity: async () => false });
  await expect(portal(source).customerPortalCommand(actor, input, key)).rejects.toThrow("CUSTOMER_PORTAL_IDENTITY_REQUIRED");
});
it("support evidence, customer receipt, staff notification, audit and outbox use the same transaction repository", async () => {
  const calls: string[] = [], transactionSource = repository<PortalRepository>({ replay: async () => null, binding: async () => binding, identity: async () => true, existing: async () => null,
    evidence: async (command, attachmentActor) => { expect(command.binding).toBe(binding); expect(attachmentActor).toMatchObject({ role: "external_customer", companyId: "company", assignmentId: "customer-portal:grant" }); calls.push("evidence"); return { manifest: [{ file: "clean" }], commit: async () => { calls.push("binding-version-and-files"); } }; },
    record: async (command, evidence) => { expect(command.input.attachmentSetIds).toEqual(["evidence-set"]); calls.push("receipt"); await evidence?.commit(); },
    notify: async (_command, message) => { expect(message).toContain("Customer requested support"); calls.push("notification"); },
    audit: async () => { calls.push("audit"); }, outbox: async (_command, _id, manifest) => { expect(manifest).toEqual([{ file: "clean" }]); calls.push("outbox"); },
  });
  const source = repository<PortalRepository>({ transaction: async callback => { calls.push("begin"); const result = await callback(transactionSource); calls.push("commit"); return result; }, uniqueConflict: () => false });
  expect(await portal(source).customerPortalCommand({ ...actor, holdingId: "H001", holdingSlug: "acme", holdingName: "Acme" }, { ...input, attachmentSetIds: ["evidence-set"] }, key)).toMatchObject({ status: "recorded", orderId: "order" });
  expect(calls).toEqual(["begin", "evidence", "receipt", "binding-version-and-files", "notification", "audit", "outbox", "commit"]);
});
it("a failure after customer persistence leaves the unit of work uncommitted", async () => {
  let committed = false;
  const tx = transaction({ binding: async () => binding, identity: async () => true, existing: async () => null, record: async () => {}, notify: async () => {}, audit: async () => {}, outbox: async () => { throw new Error("late outbox fault"); } });
  const source = repository<PortalRepository>({ transaction: async callback => { const result = await callback(tx); committed = true; return result; }, uniqueConflict: () => false });
  await expect(portal(source).customerPortalCommand(actor, input, key)).rejects.toThrow("late outbox fault");
  expect(committed).toBe(false);
});
it("a concurrent unique-key collision returns only a receipt with the same customer and request hash", async () => {
  const source = repository<PortalRepository>({ transaction: async () => { throw new Error("unique conflict"); }, uniqueConflict: () => true, replay: async () => prior });
  expect(await portal(source).customerPortalCommand(actor, input, key)).toMatchObject({ id: "original" });
});
it("projection decodes only allowlisted order facts and uses the injected read identity", async () => {
  const source = repository<PortalRepository>({ projection: async (_actor, now, id) => { expect([now, id]).toEqual([100, "customer-read-identifier"]); return [{ binding_id: "grant", fulfillments: "[]", actions: "[]" }]; }, sharedFiles: async (_actor, ids) => { expect(ids).toEqual(["grant"]); return new Map([["grant", [{ linkId: "file", label: "Customer document", previewAvailable: true as const }]]]); } });
  expect(await portal(source).customerPortalProjection(actor)).toMatchObject({ projectionVersion: "customer-order-v1", items: [{ binding_id: "grant", fulfillments: [], actions: [], sharedFiles: [{ linkId: "file" }] }] });
});
function attachments(source: PortalAttachmentRepository) { return createCustomerPortalAttachmentApplication({ repository: source, clock: { now: () => 100 }, identifiers: { next: () => "identifier" }, files: { get: async () => { throw new Error("unexpected file read"); } }, error }); }
it("an inaccessible preview never reads a file or records an allowed access", async () => {
  await expect(attachments(repository<PortalAttachmentRepository>({ preview: async () => null })).customerPortalSharedPreview(actor, "file")).rejects.toThrow("CUSTOMER_SHARED_FILE_NOT_FOUND");
});
it("an empty set of customer grants queries no database and reads no clock", async () => {
  expect(await attachments(repository<PortalAttachmentRepository>({})).customerPortalSharedFiles(actor, [])).toEqual(new Map());
});
it("an unverified account cannot obtain customer document context", () => {
  const application = createCustomerPortalSessionApplication({ error });
  expect(() => application.verify({ id: "customer", email: actor.email, emailVerified: false })).toThrow("CUSTOMER_EMAIL_UNVERIFIED");
  expect(application.verify({ id: "customer", email: "CUSTOMER@example.test", emailVerified: true })).toEqual(actor);
});
