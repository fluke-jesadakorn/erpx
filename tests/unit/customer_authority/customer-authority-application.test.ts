import { expect, it } from "vitest";
import { createCustomerAccessApplication, type AccessRepository } from "../../../src/server/modules/customer_authority/application/access";
import { createCustomerDuplicateApplication, type DuplicateRepository } from "../../../src/server/modules/customer_authority/application/duplicates";
import { createCustomerIdentityApplication, type IdentityRepository } from "../../../src/server/modules/customer_authority/application/identity";

function repository<T extends object>(operations: Partial<T>): T { return new Proxy(operations as T, { get(target, name) { if (name === "then") return undefined; if (name in target) return Reflect.get(target, name); return () => { throw new Error(`unexpected operation: ${String(name)}`); }; } }); }
const error = (_status: number, code: string) => new Error(code);
const common = { actorId: "reviewer", companyScope: true, require: () => {}, canApprove: () => true, canSales: () => true, canRead: () => true, clock: { now: () => 100 }, error };
const access = (source: AccessRepository, overrides: Partial<Parameters<typeof createCustomerAccessApplication>[0]> = {}) => createCustomerAccessApplication({ ...common, repository: async () => source, ...overrides });
const duplicates = (source: DuplicateRepository, overrides: Partial<Parameters<typeof createCustomerDuplicateApplication>[0]> = {}) => createCustomerDuplicateApplication({ ...common, repository: async () => source, ...overrides });
const identity = (source: IdentityRepository, overrides: Partial<Parameters<typeof createCustomerIdentityApplication>[0]> = {}) => createCustomerIdentityApplication({ ...common, repository: async () => source, ...overrides });
const grant = { id: "grant", created_by: "author", status: "pending", actor_eligible: 1 };
const duplicate = { id: "review", created_by: "author", status: "duplicate", canonical_customer_id: "left", left_customer_id: "left", right_customer_id: "right", left_status: "active", right_status: "active", left_merged_into_id: null, right_merged_into_id: null, actor_eligible: 1 };

it("an access replay bypasses a now-expired draft and avoids the source/clock", async () => {
  const source = repository<AccessRepository>({ replay: async () => ({ id: "grant", status: "pending" }) });
  const application = access(source, { clock: { now: () => { throw new Error("unexpected clock"); } } });
  expect(await application.requestCustomerAccess({ orderId: "order", sourceVersion: 3, email: "customer@example.test", expiresAt: 1, reason: "Verified recipient" }, "key")).toMatchObject({ id: "grant" });
});
it("a new customer grant expiry must be within ninety days before any source access", async () => {
  const source = repository<AccessRepository>({ replay: async () => null });
  for (const expiresAt of [100, 100 + 90 * 86400000 + 1]) await expect(access(source).requestCustomerAccess({ orderId: "order", sourceVersion: 3, email: "customer@example.test", expiresAt, reason: "Verified recipient" }, "key")).rejects.toThrow("CUSTOMER_ACCESS_EXPIRY");
});
it("customer access review remains independent even when the requester has approval permission", async () => {
  const source = repository<AccessRepository>({ replay: async () => null, binding: async () => ({ ...grant, created_by: "reviewer" }) });
  await expect(access(source).changeCustomerAccess("grant", { action: "approve", expectedVersion: 1, reason: "Self review" }, "key")).rejects.toThrow("INDEPENDENT_APPROVER_REQUIRED");
});
it("the original requester may revoke without approval permission and preserves prior review evidence", async () => {
  const source = repository<AccessRepository>({ replay: async () => null, binding: async () => ({ ...grant, created_by: "reviewer", approved_by: "independent", reviewed_at: 90 }), change: async (_command, row, facts) => { expect(row.approved_by).toBe("independent"); expect(facts).toEqual({ review: false, status: "revoked", now: 100 }); return { id: "grant", status: "revoked" }; } });
  expect(await access(source, { canApprove: () => false }).changeCustomerAccess("grant", { action: "revoke", expectedVersion: 2, reason: "Authority withdrawn" }, "key")).toMatchObject({ status: "revoked" });
});
it("Company scope is checked before duplicate schema parsing, receipt lookup or database acquisition", async () => {
  await expect(duplicates(repository<DuplicateRepository>({}), { companyScope: false, repository: async () => { throw new Error("unexpected database"); } }).changeCustomerDuplicate("review", {}, "key")).rejects.toThrow("CUSTOMER_COMPANY_REVIEW_REQUIRED");
});
it("a merge retains the first immutable source blocker before reading or changing Finance parties", async () => {
  const source = repository<DuplicateRepository>({ replay: async () => null, rows: async () => [duplicate], customers: async () => [], history: async () => [], blockers: async () => ({ notifications: true, grants: true, agreements: false, billing: false, orderEvidence: false }) });
  const application = duplicates(source, { error: (_status, code, message) => new Error(`${code}:${message}`) });
  await expect(application.changeCustomerDuplicate("review", { action: "merge", expectedVersion: 2, reason: "Verified canonical identity" }, "key")).rejects.toThrow("Customer notifications retain immutable recipient snapshots.");
});
it("a customer merge refuses a currency change before staging any foreign owner reference", async () => {
  const source = repository<DuplicateRepository>({ replay: async () => null, rows: async () => [duplicate], customers: async () => [], history: async () => [], blockers: async () => ({ notifications: false, grants: false, agreements: false, billing: false, orderEvidence: false }), parties: async () => ({ left_status: "active", right_status: "active", left_merged_into_id: null, right_merged_into_id: null, currency: "THB" }), currency: async () => "USD" });
  await expect(duplicates(source).changeCustomerDuplicate("review", { action: "merge", expectedVersion: 2, reason: "Verified canonical identity" }, "key")).rejects.toThrow("CUSTOMER_MERGE_SOURCE_CHANGED");
});
it("a verified merge carries canonical and merged identities through one repository commit", async () => {
  const source = repository<DuplicateRepository>({ replay: async () => null, rows: async () => [duplicate], customers: async () => [], history: async () => [], blockers: async () => ({ notifications: false, grants: false, agreements: false, billing: false, orderEvidence: false }), parties: async () => ({ left_status: "active", right_status: "active", left_merged_into_id: null, right_merged_into_id: null, currency: "THB" }), currency: async () => "THB", merge: async (command, facts) => { expect(command.operation).toBe("customer.duplicate.merge"); expect(facts).toEqual({ canonical: "left", loser: "right", now: 100 }); return { id: "review", status: "merged", canonicalCustomerId: facts.canonical, mergedCustomerId: facts.loser }; } });
  expect(await duplicates(source).changeCustomerDuplicate("review", { action: "merge", expectedVersion: 2, reason: "Verified canonical identity" }, "key")).toMatchObject({ canonicalCustomerId: "left", mergedCustomerId: "right" });
});
it("an unchanged identity proposal cannot create another review or consult the clock", async () => {
  const source = repository<IdentityRepository>({ replay: async () => null, customer: async () => ({ name: "Customer", tax_registration: null, phone: null, address_json: "{}", notification_email: null, notification_consent: "unknown" }) });
  await expect(identity(source).requestCustomerIdentityChange({ customerId: "customer", expectedCustomerVersion: 1, proposed: { name: "Customer", taxRegistration: null }, evidence: "Verified unchanged identity" }, "key")).rejects.toThrow("CUSTOMER_IDENTITY_UNCHANGED");
});
it("an independently reviewed identity applies its retained proposed snapshot and expected source version", async () => {
  const proposed = { name: "Reviewed customer", taxRegistration: null, phone: null, address: null, notificationEmail: null, notificationConsent: "granted" }, row = { id: "change", created_by: "author", status: "pending", actor_eligible: 1, customer_id: "customer", proposed_json: JSON.stringify(proposed), expected_customer_version: 4 };
  const source = repository<IdentityRepository>({ replay: async () => null, rows: async () => [row], customers: async () => [], history: async () => [], change: async (_command, current, facts) => { expect(current.customer_id).toBe("customer"); expect(facts).toEqual({ proposed, customerVersion: 4, status: "applied", now: 100 }); return { id: "change", status: "applied" }; } });
  expect(await identity(source).changeCustomerIdentity("change", { action: "apply", expectedVersion: 1, reason: "Independent evidence review" }, "key")).toMatchObject({ status: "applied" });
});
it("customer authority Work projections acquire no repository outside Sales/read access", async () => {
  const overrides = { canSales: () => false, repository: async () => { throw new Error("unexpected repository"); } };
  expect(await access(repository<AccessRepository>({}), overrides).customerAccessWork()).toEqual([]);
  expect(await duplicates(repository<DuplicateRepository>({}), overrides).customerDuplicateWork()).toEqual([]);
  expect(await identity(repository<IdentityRepository>({}), overrides).customerIdentityWork()).toEqual([]);
});
