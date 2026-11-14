import { expect, it } from "vitest";
import { createCustomerAccessApplication, type AccessRepository } from "@erpx/server-customer_authority/application/access";
import { createCustomerDuplicateApplication, type DuplicateRepository } from "@erpx/server-customer_authority/application/duplicates";
import { createCustomerIdentityApplication, type IdentityRepository } from "@erpx/server-customer_authority/application/identity";

function repository<T extends object>(operations: Partial<T>): T { return new Proxy(operations as T, { get(target, name) { if (name === "then") return undefined; if (name in target) return Reflect.get(target, name); return () => { throw new Error(`unexpected operation: ${String(name)}`); }; } }); }
const error = (_status: number, code: string) => new Error(code);
const common = { actorId: "reviewer", companyScope: true, require: () => {}, canApprove: () => true, canSales: () => true, canRead: () => true, clock: { now: () => 100 }, error };
const access = (source: AccessRepository, overrides: Partial<Parameters<typeof createCustomerAccessApplication>[0]> = {}) => createCustomerAccessApplication({ ...common, repository: async () => source, ...overrides });
const duplicates = (source: DuplicateRepository, overrides: Partial<Parameters<typeof createCustomerDuplicateApplication>[0]> = {}) => createCustomerDuplicateApplication({ ...common, repository: async () => source, ...overrides });
const identity = (source: IdentityRepository, overrides: Partial<Parameters<typeof createCustomerIdentityApplication>[0]> = {}) => createCustomerIdentityApplication({ ...common, repository: async () => source, ...overrides });

it("does not advertise Portal invitation for an approved binding whose source order is canceled", async () => {
  const source = repository<AccessRepository>({ binding: async () => ({ id: "binding", number: "Q-1", email: "customer@example.test", status: "approved", expires_at: 500, version: 2, created_by: "reviewer", approved_by: "manager", reviewed_at: 90, order_id: "order", order_state: "cancelled", branch_id: "branch", department_id: null, updated_at: 90, actor_eligible: 1 }) });
  expect(await access(source).customerAccessBinding("binding")).toMatchObject({ canInvite: false });
});
it("customer authority Work projections acquire no repository outside Sales/read access", async () => {
  const overrides = { canSales: () => false, repository: async () => { throw new Error("unexpected repository"); } };
  expect(await access(repository<AccessRepository>({}), overrides).customerAccessWork()).toEqual([]);
  expect(await duplicates(repository<DuplicateRepository>({}), overrides).customerDuplicateWork()).toEqual([]);
  expect(await identity(repository<IdentityRepository>({}), overrides).customerIdentityWork()).toEqual([]);
});

it("preserves the customer access schema's Unicode, control and email input rules", async () => {
  const { customerAccessRequestSchema, customerAccessChangeSchema } = await import("@erpx/server-customer_authority/domain/access");
  const base = { orderId: "order", sourceVersion: 1, email: "Customer@Example.Test", expiresAt: 500, reason: "Verified" };
  const emoji = "😀".repeat(700);
  expect(Buffer.byteLength(emoji, "utf8")).toBeGreaterThan(2000);
  expect(customerAccessRequestSchema.parse({ ...base, reason: `  ${emoji}  ` })).toMatchObject({ reason: emoji, email: "customer@example.test" });
  expect(customerAccessChangeSchema.parse({ action: "approve", expectedVersion: 1, reason: "ab\ncd" }).reason).toBe("ab\ncd");
  expect(customerAccessRequestSchema.safeParse({ ...base, email: " customer@example.test " }).success).toBe(false);
  expect(customerAccessChangeSchema.safeParse({ action: "approve", expectedVersion: 1, reason: "x".repeat(2001) }).success).toBe(false);
  const change = { action: "approve" as const, expectedVersion: 1 };
  expect(customerAccessChangeSchema.parse({ ...change, reason: "😀".repeat(1500) }).reason).toBe("😀".repeat(1500));
  expect(customerAccessChangeSchema.safeParse({ ...change, reason: "😀".repeat(2001) }).success).toBe(false);
  expect(customerAccessChangeSchema.parse({ ...change, reason: "\uFEFFabc\uFEFF" }).reason).toBe("abc");
  expect(customerAccessChangeSchema.parse({ ...change, reason: "\u0085abc\u0085" }).reason).toBe("\u0085abc\u0085");
});
