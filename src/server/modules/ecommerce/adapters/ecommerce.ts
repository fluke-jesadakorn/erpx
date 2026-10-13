import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "../../finance/public/commercial-owner-projection-sources";
import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { Row } from "../../commerce/public/commerce";
import { holdingResources } from "../../runtime/public/runtime";
import { requirePermission } from "../../identity_access/public/iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "../../workflow/public/commands";
import { ApiError } from "../../runtime/public/http";
import { decimalToMinor } from "@/lib/finance/finance-money";
import { snapshotMoney } from "../../finance/public/company-money";
import { isTransactionalSqlDatabase, type SqlDatabase } from "../../runtime/public/runtime/types";
import { changeOrderReservation, getOrder, orderSchema, requestCommerceReturn, returnRequestSchema, saveOrder, transitionOrder } from "../../commerce/public/commerce";
import { createEcommerceApplication, type EcommerceRepository } from "../application/commerce";
import { ecommerceCurrencyConfiguration, ecommerceProductCandidates, ecommerceWarehouseCandidates, ecommerceWarehouse, ecommerceFulfillmentCurrent } from "../../commerce/public/ecommerce-sources";
import { ecommerceCustomerCandidates } from "../../finance/public/ecommerce-customers";
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "commerce.sales" : "commerce.read");
  if (!hasModuleAccess(a.modules, "ecommerce"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access E-commerce.",
    );
}
const cartScope = "c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)";
function scopeValues(a:WorkContext){return [a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];}

function bindRepository(write: SqlDatabase, a: WorkContext, canStartTransaction: boolean): EcommerceRepository {
 let money: Awaited<ReturnType<typeof snapshotMoney>> | undefined;
 return {
  ...(canStartTransaction && isTransactionalSqlDatabase(write) ? { transaction: <Result>(action: (repository: EcommerceRepository) => Promise<Result>) => write.transaction(tx => action(bindRepository(tx, a, false))) } : {}),
  replay: command => replayCommand(write, a, command.key, command.operation, command.id, command.input),
  async workspace() {
  const [channels, listings, carts, checkouts, products, parties, warehouses] =
      await Promise.all([
        write
          .prepare(
            "SELECT * FROM ecommerce_channels WHERE company_id=? ORDER BY updated_at DESC",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            `SELECT l.*,p.name product_name,c.name channel_name FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=l.product_id WHERE c.company_id=? ORDER BY l.updated_at DESC`,
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            `SELECT c.* FROM ecommerce_carts c WHERE ${cartScope} ORDER BY c.updated_at DESC`,
          )
          .bind(...scopeValues(a))
          .all<Row>(),
        write
          .prepare(
            `SELECT x.*,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id WHERE ${cartScope} ORDER BY x.updated_at DESC`,
          )
          .bind(...scopeValues(a))
          .all<Row>(),
        ecommerceProductCandidates(write, a.companyId),
        ecommerceCustomerCandidates(write, a.companyId),
        ecommerceWarehouseCandidates(write, a),
      ]);
  return {
    channels: channels.results,
    listings: listings.results,
    carts: carts.results,
    checkouts: checkouts.results,
    products,
    parties,
    warehouses,
  };
  },
  async createChannel(command, now) {
   return executeCommand(write, a, command.key, command.operation, command.input, command.id, 1, { id: command.id, status: "draft", version: 1 }, [
    write.prepare("INSERT INTO ecommerce_channels VALUES(?,?,?,?,?,'draft',?,NULL,1,?,?)").bind(command.id, a.companyId, command.input.name, command.input.channelType, JSON.stringify(command.input.configuration), a.userId, now, now),
   ]);
  },
  functionalCurrency: () => ecommerceCurrencyConfiguration(write, a.companyId),
  async prepareMoney(input) { money = await snapshotMoney(write, a, input); },
  async createListing(command, now) {
   const { input: i, id, operation: op, key } = command;
   if (!money) throw new Error("ECOMMERCE_MONEY_NOT_PREPARED");
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    1,
    { id, status: "draft", version: 1 },
    [
      money.statement,
      assertion(
        write,
        key,
        "channel",
        "EXISTS(SELECT 1 FROM ecommerce_channels WHERE id=? AND company_id=? AND status='active')",
        [i.channelId, a.companyId],
      ),
      assertion(
        write,
        key,
        "product",
        `EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_products}) commerce_products WHERE id=? AND company_id=? AND status='active')`,
        [i.productId, a.companyId],
      ),
      write
        .prepare(
          "INSERT INTO ecommerce_listings VALUES(?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
        )
        .bind(
          id,
          i.channelId,
          i.productId,
          i.title,
          i.currency,
          decimalToMinor(i.price),
          a.userId,
          now,
          now,
        ),
    ],
  );
  },
  warehouse: id => ecommerceWarehouse(write, a, id),
  async createCart(command, warehouse, now) {
   const { input: i, id, operation: op, key } = command;
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    1,
    { id, status: "open", version: 1 },
    [
      assertion(
        write,
        key,
        "channel",
        "EXISTS(SELECT 1 FROM ecommerce_channels WHERE id=? AND company_id=? AND status='active')",
        [i.channelId, a.companyId],
      ),
      assertion(
        write,
        key,
        "party",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active')`,
        [i.partyId, a.companyId],
      ),
      write
        .prepare(
          "INSERT INTO ecommerce_carts VALUES(?,?,?,?,?,?,?,?,'open',?,1,?,?)",
        )
        .bind(
          id,
          a.companyId,
          warehouse.branch_id,
          a.departmentId,
          i.channelId,
          i.partyId,
          i.warehouseId,
          i.currency,
          a.userId,
          now,
          now,
        ),
    ],
  );
  },
  cart: (id, lock) => write.prepare(`SELECT c.* FROM ecommerce_carts c WHERE c.id=? AND ${cartScope}${lock ? " FOR UPDATE" : ""}`).bind(id, ...scopeValues(a)).first<Row>(),
  async listing(id) {
  return await write
      .prepare(
        "SELECT l.*,c.company_id,c.status channel_status FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
      )
      .bind(id, a.companyId)
      .first<Row>();

  },
  async addCartLine(command, cart, listing, lineId, now, lineCreatedAt) {
   const { input: i, operation: op, key } = command;
   if (!money) throw new Error("ECOMMERCE_MONEY_NOT_PREPARED");
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    i.cartId,
    i.expectedVersion + 1,
    { id: i.cartId, lineId, status: "open", version: i.expectedVersion + 1 },
    [
      money.statement,
      write.prepare('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE').bind(i.cartId),
      assertion(write,key,'listing-current',"EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND l.version=? AND l.status='published' AND c.status='active' AND c.company_id=? AND l.currency=?)",[i.listingId,i.listingVersion,a.companyId,cart.currency]),
      assertion(
        write,
        key,
        "cart",
        "EXISTS(SELECT 1 FROM ecommerce_carts WHERE id=? AND version=? AND status='open')",
        [i.cartId, i.expectedVersion],
      ),
      write
        .prepare(
          "INSERT INTO ecommerce_cart_lines VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(cart_id,listing_id) DO UPDATE SET quantity_milli=ecommerce_cart_lines.quantity_milli+excluded.quantity_milli",
        )
        .bind(
          lineId,
          i.cartId,
          i.listingId,
          i.listingVersion,
          listing.product_id,
          decimalToMinor(i.quantity, 3),
          listing.price_minor,
          lineCreatedAt,
        ),
      write
        .prepare(
          "UPDATE ecommerce_carts SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, i.cartId),
    ],
  );
  },
  cartLines: async id => (await write.prepare("SELECT * FROM ecommerce_cart_lines WHERE cart_id=? ORDER BY ordinal").bind(id).all<Row>()).results,
  saveSalesOrder: (actor, input, key) => saveOrder(actor, null, orderSchema.parse(input), key, write),
  transitionSalesOrder: (actor, id, input, key) => transitionOrder(actor, id, input, key, write),
  async checkout(command, cart, orderId, now) {
   const { input: i, id: checkoutId, operation: op, key } = command;
   const order = { id: orderId }, result = { id: checkoutId, cartId: cart.id, salesOrderId: order.id, status: "submitted", version: 1 };
  return executeCommand(write, a, key, op, i, checkoutId, 1, result, [
    assertion(
      write,
      key,
      "cart",
      "EXISTS(SELECT 1 FROM ecommerce_carts WHERE id=? AND version=? AND status='open')",
      [i.cartId, i.expectedVersion],
    ),
    write
      .prepare(
        "UPDATE ecommerce_carts SET status='checked_out',version=version+1,updated_at=? WHERE id=?",
      )
      .bind(now, i.cartId),
    write
      .prepare(
        "INSERT INTO ecommerce_checkouts VALUES(?,?,?,?,?,'submitted',?,1,?,?)",
      )
      .bind(
        checkoutId,
        i.cartId,
        i.expectedVersion,
        String(order.id),
        2,
        a.userId,
        now,
        now,
      ),
  ]);
  },
  async reviewRecord(kind, id) {
   const table = kind === "channel" ? "ecommerce_channels" : "ecommerce_listings";
   return write.prepare(`SELECT x.*${kind === "listing" ? ",c.company_id" : ""} FROM ${table} x ${kind === "listing" ? "JOIN ecommerce_channels c ON c.id=x.channel_id" : ""} WHERE x.id=? AND ${kind === "listing" ? "c" : "x"}.company_id=?`).bind(id, a.companyId).first<Row>();
  },
  async commitReview(command, kind, expected, status, now) {
   const { input: i, id, operation: op, key } = command, table = kind === "channel" ? "ecommerce_channels" : "ecommerce_listings";
    return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      { id, status, version: i.expectedVersion + 1 },
      [
        assertion(
          write,
          key,
          "record",
          `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status=? AND (?='retire' OR created_by<>?))`,
          [id,i.expectedVersion,expected,i.action,a.userId],
        ),
        write
          .prepare(
            `UPDATE ${table} SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?`,
          )
          .bind(status, a.userId, now, id),
      ],
    );
  },
  async checkoutRecord(id, detail) {
   if (detail) {
   return await write
      .prepare(
        `SELECT x.*,c.channel_id,c.party_id,c.warehouse_id,c.branch_id,c.department_id,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope}`,
      )
      .bind(id,...scopeValues(a))
      .first<Row>();

   }
  return await write
    .prepare(
      `SELECT x.*,c.branch_id,c.department_id,o.version current_order_version,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope} FOR UPDATE OF x,c,o`,
    )
    .bind(id,...scopeValues(a))
    .first<Row>();

  },
  salesOrder: (actor, id, detail) => detail ? getOrder(actor, id) : getOrder(actor, id, write),
  reserveSalesOrder: (actor, id, input, key) => changeOrderReservation(actor, id, input, key, write),
  async confirmCheckout(command, v, now) {
   const { input: i, id, operation: op, key } = command;
    return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      {
        id,
        status: "confirmed",
        version: i.expectedVersion + 1,
        salesOrderVersion: v,
      },
      [
        assertion(
          write,
          key,
          "checkout",
          "EXISTS(SELECT 1 FROM ecommerce_checkouts WHERE id=? AND version=? AND status='submitted')",
          [id, i.expectedVersion],
        ),
        write
          .prepare(
            "UPDATE ecommerce_checkouts SET status='confirmed',sales_order_version=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(v, now, id),
      ],
    );
  },
  async syncFulfillment(command, order, now) {
   const { input: i, id, operation: op, key } = command;
    const links=order.fulfillments.map(f=>write
        .prepare(
          "INSERT INTO ecommerce_fulfillment_links VALUES(?,?,?,?,?,?) ON CONFLICT(fulfillment_id) DO NOTHING",
        )
        .bind(
          `ecommerce-fulfillment-${f.id}`,
          id,
          f.id,
          order.order.version,
          order.order.state,
          now,
        ));
    return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      {
        id,
        status: order.order.state,
        version: i.expectedVersion + 1,
      },
      [
        ...links,
        assertion(
          write,
          key,
          "checkout",
          "EXISTS(SELECT 1 FROM ecommerce_checkouts WHERE id=? AND version=?)",
          [id, i.expectedVersion],
        ),
        write
          .prepare(
            "UPDATE ecommerce_checkouts SET status=?,sales_order_version=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(order.order.state, order.order.version, now, id),
      ],
    );
  },
  fulfillmentCurrent: (id, orderId, version) => ecommerceFulfillmentCurrent(write, id, orderId, version),
  requestReturn: (actor, id, input, key) => requestCommerceReturn(actor, id, returnRequestSchema.parse(input), key, write),
  async commitReturn(command, ret, now) {
   const { input: i, id, operation: op, key } = command;
    return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      {
        id,
        status: "return_requested",
        commerceReturnId: ret.id,
        version: i.expectedVersion + 1,
      },
      [
        assertion(
          write,
          key,
          "checkout",
          "EXISTS(SELECT 1 FROM ecommerce_checkouts WHERE id=? AND version=?)",
          [id, i.expectedVersion],
        ),
        write
          .prepare("INSERT INTO ecommerce_return_links VALUES(?,?,?,?,?,?,?)")
          .bind(
            `ecommerce-return-${key}`,
            id,
            i.fulfillmentId,
            ret.id,
            i.fulfillmentVersion,
            ret.status,
            now,
          ),
        write
          .prepare(
            "UPDATE ecommerce_checkouts SET version=version+1,updated_at=? WHERE id=?",
          )
          .bind(now, id),
      ],
    );
  },
  fulfillmentLinks: async id => (await write.prepare("SELECT * FROM ecommerce_fulfillment_links WHERE checkout_id=?").bind(id).all<Row>()).results,
  returnLinks: async id => (await write.prepare(`SELECT l.*,r.status current_return_status,r.version current_return_version FROM ecommerce_return_links l JOIN (${commerceProjectionSources.commerce_returns}) r ON r.id=l.commerce_return_id WHERE l.checkout_id=?`).bind(id).all<Row>()).results,
 };
}
function application(a: WorkContext, ownerWrite?: SqlDatabase) {
 return createEcommerceApplication({ actor: a, authorize: write => access(a, write), authorizeReview: () => requirePermission(a, "commerce.approve"), clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), repository: async () => bindRepository(ownerWrite ?? (await holdingResources(a.holdingId)).write, a, !ownerWrite) });
}
export async function ecommerceWorkspace(a: WorkContext) { return application(a).ecommerceWorkspace(); }
export async function createChannel(a: WorkContext, raw: unknown, key: string) { return application(a).createChannel(raw, key); }
export async function createListing(a: WorkContext, raw: unknown, key: string) { return application(a).createListing(raw, key); }
export async function createCart(a: WorkContext, raw: unknown, key: string) { return application(a).createCart(raw, key); }
export async function addCartLine(a: WorkContext, raw: unknown, key: string) { return application(a).addCartLine(raw, key); }
export async function checkoutCart(a: WorkContext, raw: unknown, key: string, ownerWrite?: SqlDatabase) { return application(a, ownerWrite).checkoutCart(raw, key); }
export async function changeEcommerce(a: WorkContext, kind: "channel" | "listing" | "checkout", id: string, raw: unknown, key: string, ownerWrite?: SqlDatabase) { return application(a, ownerWrite).changeEcommerce(kind, id, raw, key); }
export async function ecommerceDetail(a: WorkContext, id: string) { return application(a).ecommerceDetail(id); }
