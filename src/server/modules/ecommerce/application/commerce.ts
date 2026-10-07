import type { WorkContext } from "../../identity_access/public/authorization";
import type { Row } from "../../commerce/public/commerce";
import type { ChannelCommand } from "../domain/channel";
import { createChannelApplication } from "./channel";
import { listingSchema, cartSchema, lineSchema, checkoutSchema, actionSchema, cartLineCurrent, reviewTransition, type ListingInput, type CartInput, type CartLineInput, type CheckoutInput, type EcommerceAction, type EcommerceRecordKind } from "../domain/commerce";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import type { Clock } from "../../shared/application/ports";

export type EcommerceCommand<Input = unknown> = { key: string; operation: string; id: string; input: Input };
export type EcommerceOrder = Awaited<ReturnType<typeof import("../../commerce/public/commerce").getOrder>>;
export type EcommerceRepository = {
  transaction?<Result>(action: (repository: EcommerceRepository) => Promise<Result>): Promise<Result>;
  replay(command: EcommerceCommand): Promise<Record<string, unknown> | null>;
  workspace(): Promise<{ channels: Row[]; listings: Row[]; carts: Row[]; checkouts: Row[]; products: Row[]; parties: Row[]; warehouses: Row[] }>;
  createChannel(command: ChannelCommand, now: number): Promise<Record<string, unknown>>;
  functionalCurrency(): Promise<{ currency: string; base_currency: string } | null>;
  prepareMoney(input: { sourceType: string; sourceId: string; amountKind: string; transactionDate: string; currency: string; currencyScale: number; amountUnits: string }): Promise<void>;
  createListing(command: EcommerceCommand<ListingInput>, now: number): Promise<Record<string, unknown>>;
  warehouse(id: string): Promise<Row | null>;
  createCart(command: EcommerceCommand<CartInput>, warehouse: Row, now: number): Promise<Record<string, unknown>>;
  cart(id: string, lock: boolean): Promise<Row | null>;
  listing(id: string): Promise<Row | null>;
  addCartLine(command: EcommerceCommand<CartLineInput>, cart: Row, listing: Row, lineId: string, now: number, lineCreatedAt: number): Promise<Record<string, unknown>>;
  cartLines(id: string): Promise<Row[]>;
  saveSalesOrder(actor: WorkContext, input: { kind: "sales"; partyId: unknown; warehouseId: unknown; orderDate: string; dueDate: string; note: string; lines: { productId: unknown; quantity: string; price: string; taxBps: number }[] }, key: string): Promise<Record<string, unknown>>;
  transitionSalesOrder(actor: WorkContext, id: string, input: { action: "submit" | "approve"; expectedVersion: number }, key: string): Promise<unknown>;
  checkout(command: EcommerceCommand<CheckoutInput>, cart: Row, orderId: unknown, now: number): Promise<Record<string, unknown>>;
  reviewRecord(kind: "channel" | "listing", id: string): Promise<Row | null>;
  commitReview(command: EcommerceCommand<EcommerceAction>, kind: "channel" | "listing", expected: string, status: string, now: number): Promise<Record<string, unknown>>;
  checkoutRecord(id: string, detail: boolean): Promise<Row | null>;
  salesOrder(actor: WorkContext, id: string, detail?: boolean): Promise<EcommerceOrder>;
  reserveSalesOrder(actor: WorkContext, id: string, input: { action: "reserve"; expectedVersion: number; lineId: string; quantity: string; expiresAt: number; reason: string }, key: string): Promise<unknown>;
  confirmCheckout(command: EcommerceCommand<EcommerceAction>, orderVersion: number, now: number): Promise<Record<string, unknown>>;
  syncFulfillment(command: EcommerceCommand<EcommerceAction>, order: EcommerceOrder, now: number): Promise<Record<string, unknown>>;
  fulfillmentCurrent(id: string, orderId: unknown, version: number): Promise<boolean>;
  requestReturn(actor: WorkContext, fulfillmentId: string, input: { expectedVersion: number; date: string; reason: string; lines: NonNullable<EcommerceAction["lines"]> }, key: string): Promise<Record<string, unknown>>;
  commitReturn(command: EcommerceCommand<EcommerceAction>, result: Record<string, unknown>, now: number): Promise<Record<string, unknown>>;
  fulfillmentLinks(id: string): Promise<Row[]>;
  returnLinks(id: string): Promise<Row[]>;
};

export function createEcommerceApplication(ports: {
  actor: WorkContext; authorize(write?: boolean): void; authorizeReview(): void;
  repository(): Promise<EcommerceRepository>; clock: Clock;
  error(status: number, code: string, message: string): Error;
}) {
  const actor = ports.actor;
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function requireFunctionalCurrency(repository: EcommerceRepository, currency: string) {
    const configuration = await repository.functionalCurrency();
    if (!configuration || configuration.currency !== currency || configuration.base_currency !== currency) return fail(422, "ECOMMERCE_CURRENCY_MAPPING_REQUIRED", "Use the configured Sales functional currency for this channel and cart.");
  }
  const scoped = (row: Row): WorkContext => ({ ...actor, branchId: String(row.branch_id), departmentId: row.department_id ? String(row.department_id) : null });
  const createChannel = createChannelApplication({ authorize: () => ports.authorize(true), clock: ports.clock, repository: async () => { const repository = await ports.repository(); return { replay: command => repository.replay(command), create: (command, now) => repository.createChannel(command, now) }; } });
  async function ecommerceWorkspace() { ports.authorize(); return (await ports.repository()).workspace(); }
  async function createListing(raw: unknown, key: string) {
    ports.authorize(true);
    const input = listingSchema.parse(raw), repository = await ports.repository(), command = { input, key, id: `ecommerce-listing-${key}`, operation: "ecommerce.listing.create" };
    const prior = await repository.replay(command); if (prior) return prior;
    await requireFunctionalCurrency(repository, input.currency);
    const now = ports.clock.now();
    await repository.prepareMoney({ sourceType: "ecommerce_listing", sourceId: command.id, amountKind: "unit_price", transactionDate: new Date(now).toISOString().slice(0, 10), currency: input.currency, currencyScale: 2, amountUnits: String(decimalToMinor(input.price)) });
    return repository.createListing(command, now);
  }
  async function createCart(raw: unknown, key: string) {
    ports.authorize(true);
    const input = cartSchema.parse(raw), repository = await ports.repository(), warehouse = await repository.warehouse(input.warehouseId);
    if (!warehouse) return fail(404, "WAREHOUSE_NOT_FOUND", "Warehouse not found.");
    const command = { input, key, id: `ecommerce-cart-${key}`, operation: "ecommerce.cart.create" }, prior = await repository.replay(command); if (prior) return prior;
    await requireFunctionalCurrency(repository, input.currency);
    return repository.createCart(command, warehouse, ports.clock.now());
  }
  async function addCartLine(raw: unknown, key: string) {
    ports.authorize(true);
    const input = lineSchema.parse(raw), repository = await ports.repository(), command = { input, key, id: input.cartId, operation: "ecommerce.cart.add" }, prior = await repository.replay(command); if (prior) return prior;
    const cart = await repository.cart(input.cartId, false), listing = await repository.listing(input.listingId);
    if (!cart || !listing) return fail(404, "ECOMMERCE_SOURCE_NOT_FOUND", "Cart or listing not found.");
    if (!cartLineCurrent(cart, listing, input)) return fail(409, "ECOMMERCE_SOURCE_CHANGED", "Cart or listing changed.");
    const now = ports.clock.now(), lineId = `ecommerce-cart-line-${key}`;
    await repository.prepareMoney({ sourceType: "ecommerce_cart_command", sourceId: key, amountKind: "added_subtotal", transactionDate: new Date(now).toISOString().slice(0, 10), currency: String(cart.currency), currencyScale: 2, amountUnits: String((BigInt(decimalToMinor(input.quantity, 3)) * BigInt(String(listing.price_minor)) + BigInt(500)) / BigInt(1000)) });
    return repository.addCartLine(command, cart, listing, lineId, now, ports.clock.now());
  }
  async function checkoutCart(raw: unknown, key: string, current?: EcommerceRepository): Promise<Record<string, unknown>> {
    ports.authorize(true);
    const input = checkoutSchema.parse(raw), repository = current ?? await ports.repository();
    if (!current && repository.transaction) return repository.transaction(transaction => checkoutCart(raw, key, transaction));
    const cart = await repository.cart(input.cartId, true);
    if (!cart) return fail(404, "CART_NOT_FOUND", "Cart not found.");
    const command = { input, key, id: `ecommerce-checkout-${input.cartId}`, operation: "ecommerce.checkout" }, prior = await repository.replay(command); if (prior) return prior;
    if (cart.status !== "open" || Number(cart.version) !== input.expectedVersion) return fail(409, "CONFLICT", "Cart changed or was checked out.");
    const lines = await repository.cartLines(input.cartId);
    if (!lines.length) return fail(422, "EMPTY_CART", "Add at least one listing.");
    const order = await repository.saveSalesOrder(scoped(cart), { kind: "sales", partyId: cart.party_id, warehouseId: cart.warehouse_id, orderDate: input.orderDate, dueDate: input.dueDate, note: `E-commerce cart ${cart.id}`, lines: lines.map(line => ({ productId: line.product_id, quantity: minorToDecimal(Number(line.quantity_milli), 3), price: minorToDecimal(Number(line.price_minor)), taxBps: 0 })) }, `ecommerce-order-${cart.id}`);
    await repository.transitionSalesOrder(scoped(cart), String(order.id), { action: "submit", expectedVersion: 1 }, `ecommerce-submit-${cart.id}`);
    return repository.checkout(command, cart, order.id, ports.clock.now());
  }
  async function changeEcommerce(kind: EcommerceRecordKind, id: string, raw: unknown, key: string, current?: EcommerceRepository): Promise<Record<string, unknown>> {
    ports.authorize(true);
    const input = actionSchema.parse(raw), repository = current ?? await ports.repository();
    if (!current && repository.transaction) return repository.transaction(transaction => changeEcommerce(kind, id, raw, key, transaction));
    const command = { input, key, id, operation: `ecommerce.${kind}.${input.action}` }, prior = await repository.replay(command); if (prior) return prior;
    if (kind !== "checkout") {
      const row = await repository.reviewRecord(kind, id);
      if (!row) return fail(404, "ECOMMERCE_RECORD_NOT_FOUND", "Record not found.");
      ports.authorizeReview();
      if (actor.branchId || actor.departmentId) return fail(403, "COMPANY_ECOMMERCE_REQUIRED", "Review shared channel and listing terms through a Company assignment.");
      const transition = reviewTransition(kind, input.action);
      if (!transition) return fail(422, "INVALID_ECOMMERCE_TRANSITION", "Use the lifecycle command belonging to this record.");
      if (row.status !== transition.expected || Number(row.version) !== input.expectedVersion || (["activate", "publish", "reject"].includes(input.action) && row.created_by === actor.userId)) return fail(409, "CONFLICT", "Independent current review is required.");
      return repository.commitReview(command, kind, transition.expected, transition.status, ports.clock.now());
    }
    const checkout = await repository.checkoutRecord(id, false);
    if (!checkout) return fail(404, "CHECKOUT_NOT_FOUND", "Checkout not found.");
    if (Number(checkout.version) !== input.expectedVersion) return fail(409, "ECOMMERCE_SOURCE_CHANGED", "Reload the checkout before performing this action.");
    const scopedActor = scoped(checkout);
    if (input.action === "approve-order") {
      if (checkout.status !== "submitted") return fail(409, "ECOMMERCE_SOURCE_CHANGED", "Only a submitted checkout can be approved.");
      await repository.transitionSalesOrder(scopedActor, String(checkout.sales_order_id), { action: "approve", expectedVersion: Number(checkout.current_order_version) }, `${key}:approve`);
      const order = await repository.salesOrder(scopedActor, String(checkout.sales_order_id)); let orderVersion = Number(order.order.version);
      for (const line of order.lines) {
        await repository.reserveSalesOrder(scopedActor, String(checkout.sales_order_id), { action: "reserve", expectedVersion: orderVersion, lineId: String(line.id), quantity: minorToDecimal(Number(line.quantity_milli) - Number(line.fulfilled_milli), 3), expiresAt: input.expiresAt ?? ports.clock.now() + 86400000, reason: input.reason }, `${key}:reserve:${line.id}`);
        orderVersion++;
      }
      return repository.confirmCheckout(command, orderVersion, ports.clock.now());
    }
    if (input.action === "sync-fulfillment") {
      const order = await repository.salesOrder(scopedActor, String(checkout.sales_order_id));
      return repository.syncFulfillment(command, order, ports.clock.now());
    }
    if (input.action === "request-return") {
      if (!input.fulfillmentId || !input.fulfillmentVersion || !input.date || !input.lines) return fail(422, "RETURN_DETAIL_REQUIRED", "Fulfillment, version, date and lines are required.");
      if (!await repository.fulfillmentCurrent(input.fulfillmentId, checkout.sales_order_id, input.fulfillmentVersion)) return fail(404, "ECOMMERCE_FULFILLMENT_NOT_FOUND", "Select a current delivery from this checkout.");
      const result = await repository.requestReturn(scopedActor, input.fulfillmentId, { expectedVersion: input.fulfillmentVersion, date: input.date, reason: input.reason, lines: input.lines }, `${key}:return`);
      return repository.commitReturn(command, result, ports.clock.now());
    }
    return fail(409, "INVALID_ECOMMERCE_TRANSITION", "Unsupported checkout action.");
  }
  async function ecommerceDetail(id: string) {
    ports.authorize(); const repository = await ports.repository(), checkout = await repository.checkoutRecord(id, true);
    if (!checkout) return fail(404, "CHECKOUT_NOT_FOUND", "Checkout not found.");
    const order = await repository.salesOrder(scoped(checkout), String(checkout.sales_order_id), true);
    return { checkout, order, fulfillmentLinks: await repository.fulfillmentLinks(id), returnLinks: await repository.returnLinks(id) };
  }
  return { ecommerceWorkspace, createChannel, createListing, createCart, addCartLine, checkoutCart, changeEcommerce, ecommerceDetail };
}
