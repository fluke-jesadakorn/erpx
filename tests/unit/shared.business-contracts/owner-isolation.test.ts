import { expect, it } from "vitest";
import { createAssetRegistration, type AssetRegistrationRepository } from "../../../src/server/modules/assets/application/registration";
import { createChannelApplication, type ChannelRepository } from "../../../src/server/modules/ecommerce/application/channel";
import { createEcommerceApplication, type EcommerceRepository } from "../../../src/server/modules/ecommerce/application/commerce";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { createCustomerTimeline, type CustomerTimelineRepository } from "../../../src/server/modules/customer_360/application/timeline";
import { materialAllocationViolation, materialLotViolation, outputAllocationViolation } from "../../../src/server/modules/manufacturing/domain/lot-policy";

const error = (_status: number, code: string) => new Error(code);
const ecommerceActor = { userId: "author", companyId: "company", branchId: null, departmentId: null } as WorkContext;
function ecommerceRepository(overrides: Partial<EcommerceRepository>): EcommerceRepository {
  return new Proxy(overrides as EcommerceRepository, { get(target, property) {
    if (property === "then") return undefined;
    if (property === "transaction") return target.transaction;
    if (property in target) return Reflect.get(target, property);
    return () => { throw new Error(`unexpected repository operation: ${String(property)}`); };
  } });
}
function ecommerce(repository: EcommerceRepository, overrides: Partial<Parameters<typeof createEcommerceApplication>[0]> = {}) {
  return createEcommerceApplication({ actor: ecommerceActor, authorize: () => {}, authorizeReview: () => {}, repository: async () => repository, clock: { now: () => 100 }, error, ...overrides });
}
it("E-commerce replays a listing before currency, money or clock work, and checks revocation first", async () => {
  const receipt = { id: "listing", version: 1, status: "draft" }, repository = ecommerceRepository({ replay: async () => receipt });
  const input = { channelId: "channel", productId: "product", title: "Online item", currency: "thb", price: "10.00" };
  expect(await ecommerce(repository, { clock: { now: () => { throw new Error("clock accessed during replay"); } } }).createListing(input, "key")).toEqual(receipt);
  await expect(ecommerce(repository, { authorize: () => { throw new Error("revoked"); } }).createListing({}, "key")).rejects.toThrow("revoked");
});
it("E-commerce rejects stale listing terms before money preparation or a cart write", async () => {
  const repository = ecommerceRepository({ replay: async () => null, cart: async () => ({ id: "cart", status: "open", version: 2, channel_id: "channel", currency: "THB" }), listing: async () => ({ id: "listing", status: "published", channel_status: "active", version: 3, channel_id: "channel", currency: "THB" }) });
  await expect(ecommerce(repository).addCartLine({ cartId: "cart", expectedVersion: 2, listingId: "listing", listingVersion: 2, quantity: "1" }, "key")).rejects.toThrow("ECOMMERCE_SOURCE_CHANGED");
});
it("E-commerce rejects functional-currency mismatch before Finance recognition", async () => {
  const repository = ecommerceRepository({ replay: async () => null, functionalCurrency: async () => ({ currency: "THB", base_currency: "USD" }) });
  await expect(ecommerce(repository).createListing({ channelId: "channel", productId: "product", title: "Online item", currency: "THB", price: "10.00" }, "key")).rejects.toThrow("ECOMMERCE_CURRENCY_MAPPING_REQUIRED");
});
it("E-commerce checkout passes every Sales operation through the transaction repository", async () => {
  const calls: string[] = [], cart = { id: "cart", status: "open", version: 2, branch_id: "branch", department_id: null, party_id: "customer", warehouse_id: "warehouse" };
  const transaction = ecommerceRepository({ cart: async (_id, lock) => { expect(lock).toBe(true); calls.push("lock"); return cart; }, replay: async () => null, cartLines: async () => [{ product_id: "product", quantity_milli: 1500, price_minor: 1250 }],
    saveSalesOrder: async (actor, input, key) => { expect(actor.branchId).toBe("branch"); expect(input.lines).toEqual([{ productId: "product", quantity: "1.500", price: "12.50", taxBps: 0 }]); expect(key).toBe("ecommerce-order-cart"); calls.push("sales-draft"); return { id: "order" }; },
    transitionSalesOrder: async (_actor, id, input, key) => { expect([id, input, key]).toEqual(["order", { action: "submit", expectedVersion: 1 }, "ecommerce-submit-cart"]); calls.push("sales-submit"); },
    checkout: async (command, source, id) => { expect([command.id, source.id, id]).toEqual(["ecommerce-checkout-cart", "cart", "order"]); calls.push("checkout-receipt"); return { id: command.id, status: "submitted", version: 1 }; } });
  const pooled = ecommerceRepository({ transaction: async action => { calls.push("begin"); const result = await action(transaction); calls.push("commit"); return result; } });
  expect(await ecommerce(pooled).checkoutCart({ cartId: "cart", expectedVersion: 2, orderDate: "2026-10-05", dueDate: "2026-10-07" }, "key")).toMatchObject({ status: "submitted" });
  expect(calls).toEqual(["begin", "lock", "sales-draft", "sales-submit", "checkout-receipt", "commit"]);
});
it("E-commerce submission failure leaves checkout persistence to the transaction rollback", async () => {
  const calls: string[] = [], transaction = ecommerceRepository({ cart: async () => ({ id: "cart", status: "open", version: 1 }), replay: async () => null, cartLines: async () => [{ product_id: "product", quantity_milli: 1000, price_minor: 100 }], saveSalesOrder: async () => ({ id: "order" }), transitionSalesOrder: async () => { throw new Error("Sales permission revoked"); } });
  const pooled = ecommerceRepository({ transaction: async action => { try { return await action(transaction); } catch (failure) { calls.push("rollback"); throw failure; } } });
  await expect(ecommerce(pooled).checkoutCart({ cartId: "cart", expectedVersion: 1, orderDate: "2026-10-05", dueDate: "2026-10-07" }, "key")).rejects.toThrow("Sales permission revoked");
  expect(calls).toEqual(["rollback"]);
});
it("E-commerce channel review enforces independence and Company scope before committing", async () => {
  const repository = ecommerceRepository({ replay: async () => null, reviewRecord: async () => ({ status: "draft", version: 1, created_by: "author" }) }), input = { action: "activate", expectedVersion: 1, reason: "Independent review" };
  await expect(ecommerce(repository).changeEcommerce("channel", "channel", input, "key")).rejects.toThrow("CONFLICT");
  await expect(ecommerce(repository, { actor: { ...ecommerceActor, userId: "reviewer", branchId: "branch" } }).changeEcommerce("channel", "channel", input, "key")).rejects.toThrow("COMPANY_ECOMMERCE_REQUIRED");
});
const assetInput = { branchId: "requested", departmentId: "requested-department", code: "A", name: "Lathe", category: "Machinery", currency: "THB", cost: "500", acquiredDate: "2026-10-01" };
it("asset registration keeps scope, Finance recognition and replay under one injected repository", async () => {
  const calls: string[] = []; let receipt: Record<string, unknown> | null = null;
  const repository: AssetRegistrationRepository = {
    replay: async () => receipt, currencyScale: async () => 2,
    prepareAcquisitionCost: async () => { calls.push("recognition"); },
    register: async (command, state) => { expect(state).toEqual({ branch: "scope-branch", department: "scope-department", costMinor: 50000, now: 100 }); calls.push("commit"); return receipt = { id: command.id, version: 1 }; },
  };
  const register = createAssetRegistration({ authorize: () => { calls.push("authorize"); }, authorizeFinanceLink: () => { calls.push("finance"); }, parse: () => ({ ...assetInput, financeAssetId: "finance-asset", financeAssetVersion: 2 }), scope: { branchId: "scope-branch", departmentId: "scope-department" }, repository: async () => repository, decimalUnits: () => 50000n, clock: { now: () => 100 }, error });
  const first = await register(assetInput, "asset-key"), replay = await register(assetInput, "asset-key");
  expect(replay).toEqual(first); expect(calls).toEqual(["authorize", "recognition", "finance", "commit", "authorize"]);
});
it("an acquisition overflow produces no recognition, receipt, or owner write", async () => {
  const calls: string[] = [];
  const register = createAssetRegistration({ authorize: () => {}, authorizeFinanceLink: () => {}, parse: () => assetInput, scope: { branchId: null, departmentId: null }, clock: { now: () => 100 }, decimalUnits: () => BigInt(Number.MAX_SAFE_INTEGER) + 1n, error,
    repository: async () => ({ replay: async () => null, currencyScale: async () => 2, prepareAcquisitionCost: async () => { calls.push("money"); }, register: async () => { calls.push("write"); return {}; } }) });
  await expect(register(assetInput, "overflow")).rejects.toThrow("MONEY_OVERFLOW"); expect(calls).toEqual([]);
});
function timeline(repository: CustomerTimelineRepository, overrides: Partial<Parameters<typeof createCustomerTimeline>[0]> = {}) {
  return createCustomerTimeline({ requireRead: () => {}, parseQuery: () => ({ limit: 1 }), decodeCursor: () => { throw new Error("bad cursor"); }, encodeCursor: cursor => JSON.stringify(cursor), repository: async () => repository, coverage: () => ["Sales quotations/orders"], error, ...overrides });
}
it("an invalid cursor or an invisible customer cannot read source events", async () => {
  let reads = 0;
  const repository: CustomerTimelineRepository = { visibleCustomer: async () => null, events: async () => { reads++; return []; } };
  await expect(timeline(repository, { parseQuery: () => ({ cursor: "invalid", limit: 1 }) })("customer")).rejects.toThrow("TIMELINE_CURSOR_INVALID");
  await expect(timeline(repository)("foreign-customer")).rejects.toThrow("CUSTOMER_TIMELINE_NOT_FOUND"); expect(reads).toBe(0);
});
it("the Customer 360 projection strips source internals and advances from the last displayed event", async () => {
  const row = { id: "order:1", source_id: "1", source_type: "order", title: "Sales order", status: "confirmed", version: 2, at: 100, href: "/commerce/orders/1", customer_id: "customer", customer_name: "Customer", internal_note: "private source note" };
  const result = await timeline({ visibleCustomer: async () => ({ id: "customer", name: "Customer", code: "CUS" }), events: async (_id, _cursor, limit) => { expect(limit).toBe(2); return [row, { ...row, id: "order:0", at: 99 }]; } })("customer");
  expect(result.hasMore).toBe(true); expect(result.nextCursor).toBe('{"at":100,"id":"order:1"}');
  expect(result.items[0]).not.toHaveProperty("internal_note"); expect(result.items[0]).not.toHaveProperty("customer_name"); expect(result.items[0]).not.toHaveProperty("customer_id");
});
it("E-commerce channel replay retains the command identity without another clock or write", async () => {
  let receipt: Record<string, unknown> | null = null, writes = 0, ticks = 0;
  const repository: ChannelRepository = { replay: async () => receipt, create: async (command, at) => { expect(command).toMatchObject({ operation: "ecommerce.channel.create", id: "ecommerce-channel-key", input: { name: "Shop" } }); expect(at).toBe(50); writes++; return receipt = { id: command.id, version: 1, status: "draft" }; } };
  const create = createChannelApplication({ authorize: () => {}, repository: async () => repository, clock: { now: () => { ticks++; return 50; } } });
  const input = { name: " Shop ", channelType: "web", configuration: {} }, first = await create(input, "key");
  expect(await create(input, "key")).toEqual(first); expect([writes, ticks]).toEqual([1, 1]);
});
it("manufacturing allocations reject duplicates, quarantined/stale lots, and partial serials", () => {
  const milli = (value: string) => Number(value) * 1000, allocation = { productId: "material", lotId: "lot", quantity: "1", balanceVersion: 1 };
  expect(materialAllocationViolation("lot", 2000, [allocation, allocation], milli)?.code).toBe("LOT_ALLOCATION_DUPLICATE");
  const lot = { status: "active", held: false, balance_version: 1, quantity_milli: 2000, expiry_date: "2026-10-01" }, facts = { balanceVersion: 1, quantity: 500, trackingMode: "serial", entryDate: "2026-09-30" };
  expect(materialLotViolation({ ...lot, held: true }, facts)?.code).toBe("MANUFACTURING_LOT_UNAVAILABLE");
  expect(materialLotViolation(lot, { ...facts, balanceVersion: 2 })?.code).toBe("MANUFACTURING_LOT_UNAVAILABLE");
  expect(materialLotViolation(lot, { ...facts, entryDate: "2026-10-02" })?.code).toBe("MANUFACTURING_LOT_UNAVAILABLE");
  expect(materialLotViolation(lot, facts)?.code).toBe("SERIAL_QUANTITY_INVALID");
  expect(outputAllocationViolation("serial", 1000, [{ quantity: "1" }], milli)?.code).toBe("MANUFACTURING_OUTPUT_IDENTITY");
  expect(materialAllocationViolation("none", 1000, [], () => { throw new Error("must not parse"); })).toBeNull();
});
