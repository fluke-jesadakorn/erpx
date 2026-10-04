import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { snapshotMoney } from "./company-money";
import { isTransactionalSqlDatabase, type SqlDatabase } from "./runtime/types";
import {
  changeOrderReservation,
  getOrder,
  orderSchema,
  requestCommerceReturn,
  returnRequestSchema,
  saveOrder,
  transitionOrder,
} from "./commerce";
const reason = z.string().trim().min(3).max(2000),
  version = z.number().int().positive(),
  qty = z.string().regex(/^\d+(\.\d{1,3})?$/);
const channelSchema = z.object({
  name: z.string().trim().min(2).max(160),
  channelType: z.enum(["web", "marketplace", "social"]),
  configuration: z.record(z.string(), z.unknown()),
});
const listingSchema = z.object({
  channelId: z.string().min(1),
  productId: z.string().min(1),
  title: z.string().trim().min(2).max(200),
  currency: z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase()),
  price: z.string().regex(/^\d+(\.\d{1,2})?$/),
});
const cartSchema = z.object({
  channelId: z.string().min(1),
  partyId: z.string().min(1),
  warehouseId: z.string().min(1),
  currency: z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase()),
});
const lineSchema = z.object({
  cartId: z.string().min(1),
  expectedVersion: version,
  listingId: z.string().min(1),
  listingVersion: version,
  quantity: qty,
});
const checkoutSchema = z.object({
  cartId: z.string().min(1),
  expectedVersion: version,
  orderDate: z.string().date(),
  dueDate: z.string().date(),
});
const actionSchema = z.object({
  action: z.enum([
    "activate",
    "reject",
    "retire",
    "publish",
    "approve-order",
    "sync-fulfillment",
    "request-return",
  ]),
  expectedVersion: version,
  reason,
  expiresAt: z.number().int().positive().optional(),
  fulfillmentId: z.string().min(1).optional(),
  fulfillmentVersion: version.optional(),
  date: z.string().date().optional(),
  lines: z
    .array(z.object({ lineId: z.string().min(1), quantity: qty, condition:z.enum(['unopened','damaged','unknown']).default('unknown'), conditionNote:z.string().trim().max(500).default(''), qualityDueDate:z.union([z.string().date(),z.literal('')]).default(''),allocations:z.array(z.object({lotId:z.string().min(1),quantity:qty})).max(200).optional() }))
    .optional(),
});
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
async function functionalCurrency(write:SqlDatabase,a:WorkContext,currency:string){
 const config=await write.prepare('SELECT s.currency,c.base_currency FROM commerce_settings s JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id WHERE s.company_id=?').bind(a.companyId).first<{currency:string;base_currency:string}>();
 if(!config||config.currency!==currency||config.base_currency!==currency)throw new ApiError(422,'ECOMMERCE_CURRENCY_MAPPING_REQUIRED','Use the configured Sales functional currency for this channel and cart.');
}
export async function ecommerceWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    [channels, listings, carts, checkouts, products, parties, warehouses] =
      await Promise.all([
        write
          .prepare(
            "SELECT * FROM ecommerce_channels WHERE company_id=? ORDER BY updated_at DESC",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT l.*,p.name product_name,c.name channel_name FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN commerce_products p ON p.id=l.product_id WHERE c.company_id=? ORDER BY l.updated_at DESC",
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
            `SELECT x.*,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN commerce_orders o ON o.id=x.sales_order_id WHERE ${cartScope} ORDER BY x.updated_at DESC`,
          )
          .bind(...scopeValues(a))
          .all<Row>(),
        write
          .prepare(
            "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name",
          )
          .bind(a.companyId, a.branchId, a.branchId)
          .all<Row>(),
      ]);
  return {
    channels: channels.results,
    listings: listings.results,
    carts: carts.results,
    checkouts: checkouts.results,
    products: products.results,
    parties: parties.results,
    warehouses: warehouses.results,
  };
}
export async function createChannel(a: WorkContext, raw: unknown, key: string) {
  access(a, true);
  const i = channelSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `ecommerce-channel-${key}`,
    op = "ecommerce.channel.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now();
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
      write
        .prepare(
          "INSERT INTO ecommerce_channels VALUES(?,?,?,?,?,'draft',?,NULL,1,?,?)",
        )
        .bind(
          id,
          a.companyId,
          i.name,
          i.channelType,
          JSON.stringify(i.configuration),
          a.userId,
          now,
          now,
        ),
    ],
  );
}
export async function createListing(a: WorkContext, raw: unknown, key: string) {
  access(a, true);
  const i = listingSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `ecommerce-listing-${key}`,
    op = "ecommerce.listing.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  await functionalCurrency(write,a,i.currency);
  const now=Date.now(),money=await snapshotMoney(write,a,{sourceType:'ecommerce_listing',sourceId:id,amountKind:'unit_price',transactionDate:new Date(now).toISOString().slice(0,10),currency:i.currency,currencyScale:2,amountUnits:String(decimalToMinor(i.price))});
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
        "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')",
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
}
export async function createCart(a: WorkContext, raw: unknown, key: string) {
  access(a, true);
  const i = cartSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    warehouse = await write
      .prepare(
        "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)",
      )
      .bind(i.warehouseId, a.companyId, a.branchId, a.branchId)
      .first<Row>();
  if (!warehouse)
    throw new ApiError(404, "WAREHOUSE_NOT_FOUND", "Warehouse not found.");
  const id = `ecommerce-cart-${key}`,
    op = "ecommerce.cart.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  await functionalCurrency(write,a,i.currency);
  const now=Date.now();
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
        "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active')",
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
}
export async function addCartLine(a: WorkContext, raw: unknown, key: string) {
  access(a, true);
  const i=lineSchema.parse(raw),{write}=await holdingResources(a.holdingId),op='ecommerce.cart.add',prior=await replayCommand(write,a,key,op,i.cartId,i);
  if(prior)return prior;
  const cart = await write
      .prepare(`SELECT c.* FROM ecommerce_carts c WHERE c.id=? AND ${cartScope}`)
      .bind(i.cartId,...scopeValues(a))
      .first<Row>(),
    listing = await write
      .prepare(
        "SELECT l.*,c.company_id,c.status channel_status FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
      )
      .bind(i.listingId, a.companyId)
      .first<Row>();
  if (!cart || !listing)
    throw new ApiError(
      404,
      "ECOMMERCE_SOURCE_NOT_FOUND",
      "Cart or listing not found.",
    );
  if (
    cart.status !== "open" ||
    Number(cart.version) !== i.expectedVersion ||
    listing.status !== "published" || listing.channel_status!=='active' ||
    Number(listing.version) !== i.listingVersion ||
    cart.channel_id !== listing.channel_id ||
    cart.currency !== listing.currency
  )
    throw new ApiError(
      409,
      "ECOMMERCE_SOURCE_CHANGED",
      "Cart or listing changed.",
    );
  const now=Date.now(),lineId=`ecommerce-cart-line-${key}`,
    money=await snapshotMoney(write,a,{sourceType:'ecommerce_cart_command',sourceId:key,amountKind:'added_subtotal',transactionDate:new Date(now).toISOString().slice(0,10),currency:String(cart.currency),currencyScale:2,amountUnits:String((BigInt(decimalToMinor(i.quantity,3))*BigInt(String(listing.price_minor))+BigInt(500))/BigInt(1000))});
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
          Date.now(),
        ),
      write
        .prepare(
          "UPDATE ecommerce_carts SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, i.cartId),
    ],
  );
}
export async function checkoutCart(a: WorkContext, raw: unknown, key: string, ownerWrite?:SqlDatabase):Promise<Record<string,unknown>> {
  access(a, true);
  const i = checkoutSchema.parse(raw),write=ownerWrite??(await holdingResources(a.holdingId)).write;
  if(!ownerWrite&&isTransactionalSqlDatabase(write))return write.transaction(tx=>checkoutCart(a,raw,key,tx));
  const cart = await write.prepare(`SELECT c.* FROM ecommerce_carts c WHERE c.id=? AND ${cartScope} FOR UPDATE`).bind(i.cartId,...scopeValues(a)).first<Row>();
  if (!cart) throw new ApiError(404, "CART_NOT_FOUND", "Cart not found.");
  const checkoutId = `ecommerce-checkout-${i.cartId}`,
    op = "ecommerce.checkout",
    prior = await replayCommand(write, a, key, op, checkoutId, i);
  if (prior) return prior;
  if (cart.status !== "open" || Number(cart.version) !== i.expectedVersion)
    throw new ApiError(409, "CONFLICT", "Cart changed or was checked out.");
  const lines = (
    await write
      .prepare(
        "SELECT * FROM ecommerce_cart_lines WHERE cart_id=? ORDER BY ordinal",
      )
      .bind(i.cartId)
      .all<Row>()
  ).results;
  if (!lines.length)
    throw new ApiError(422, "EMPTY_CART", "Add at least one listing.");
  const order = await saveOrder(
    {
      ...a,
      branchId: String(cart.branch_id),
      departmentId: cart.department_id ? String(cart.department_id) : null,
    },
    null,
    orderSchema.parse({
      kind: "sales",
      partyId: cart.party_id,
      warehouseId: cart.warehouse_id,
      orderDate: i.orderDate,
      dueDate: i.dueDate,
      note: `E-commerce cart ${cart.id}`,
      lines: lines.map((x) => ({
        productId: x.product_id,
        quantity: minorToDecimal(Number(x.quantity_milli), 3),
        price: minorToDecimal(Number(x.price_minor)),
        taxBps: 0,
      })),
    }),
    `ecommerce-order-${cart.id}`,
    write,
  );
  await transitionOrder(
    {
      ...a,
      branchId: String(cart.branch_id),
      departmentId: cart.department_id ? String(cart.department_id) : null,
    },
    String(order.id),
    { action: "submit", expectedVersion: 1 },
    `ecommerce-submit-${cart.id}`,
    write,
  );
  const now = Date.now(),
    result = {
      id: checkoutId,
      cartId: cart.id,
      salesOrderId: order.id,
      status: "submitted",
      version: 1,
    };
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
}
export async function changeEcommerce(
  a: WorkContext,
  kind: "channel" | "listing" | "checkout",
  id: string,
  raw: unknown,
  key: string,
  ownerWrite?:SqlDatabase,
):Promise<Record<string,unknown>> {
  access(a, true);
  const i = actionSchema.parse(raw),
    write=ownerWrite??(await holdingResources(a.holdingId)).write;
  if(!ownerWrite&&isTransactionalSqlDatabase(write))return write.transaction(tx=>changeEcommerce(a,kind,id,raw,key,tx));
  const op=`ecommerce.${kind}.${i.action}`,prior=await replayCommand(write,a,key,op,id,i);
  if(prior)return prior;
  if (kind !== "checkout") {
    const table =
        kind === "channel" ? "ecommerce_channels" : "ecommerce_listings",
      row = await write
        .prepare(
          `SELECT x.*${kind === "listing" ? ",c.company_id" : ""} FROM ${table} x ${kind === "listing" ? "JOIN ecommerce_channels c ON c.id=x.channel_id" : ""} WHERE x.id=? AND ${kind === "listing" ? "c" : "x"}.company_id=?`,
        )
        .bind(id, a.companyId)
        .first<Row>();
    if (!row)
      throw new ApiError(
        404,
        "ECOMMERCE_RECORD_NOT_FOUND",
        "Record not found.",
      );
    requirePermission(a,'commerce.approve');
    if(a.branchId||a.departmentId)throw new ApiError(403,'COMPANY_ECOMMERCE_REQUIRED','Review shared channel and listing terms through a Company assignment.');
    const allowed=kind==='channel'?['activate','reject','retire']:['publish','reject','retire'];
    if(!allowed.includes(i.action))throw new ApiError(422,'INVALID_ECOMMERCE_TRANSITION','Use the lifecycle command belonging to this record.');
    const active = kind === "channel" ? "active" : "published",
      expected = i.action === "retire" ? active : "draft",
      status =
        i.action === "activate" || i.action === "publish"
          ? active
          : i.action === "reject"
            ? "rejected"
            : "retired";
    if (
      row.status !== expected ||
      Number(row.version) !== i.expectedVersion ||
      (["activate", "publish", "reject"].includes(i.action) &&
        row.created_by === a.userId)
    )
      throw new ApiError(
        409,
        "CONFLICT",
        "Independent current review is required.",
      );
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
          .bind(status, a.userId, Date.now(), id),
      ],
    );
  }
  const checkout = await write
    .prepare(
      `SELECT x.*,c.branch_id,c.department_id,o.version current_order_version,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN commerce_orders o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope} FOR UPDATE OF x,c,o`,
    )
    .bind(id,...scopeValues(a))
    .first<Row>();
  if (!checkout)
    throw new ApiError(404, "CHECKOUT_NOT_FOUND", "Checkout not found.");
  const scoped = {
    ...a,
    branchId: String(checkout.branch_id),
    departmentId: checkout.department_id
      ? String(checkout.department_id)
      : null,
  };
  if(Number(checkout.version)!==i.expectedVersion)throw new ApiError(409,'ECOMMERCE_SOURCE_CHANGED','Reload the checkout before performing this action.');
  if (i.action === "approve-order") {
    if(checkout.status!=='submitted')throw new ApiError(409,'ECOMMERCE_SOURCE_CHANGED','Only a submitted checkout can be approved.');
    await transitionOrder(
      scoped,
      String(checkout.sales_order_id),
      {
        action: "approve",
        expectedVersion: Number(checkout.current_order_version),
      },
      `${key}:approve`,
      write,
    );
    const order = await getOrder(scoped,String(checkout.sales_order_id),write);
    let v = Number(order.order.version);
    for (const line of order.lines) {
      await changeOrderReservation(
        scoped,
        String(checkout.sales_order_id),
        {
          action: "reserve",
          expectedVersion: v,
          lineId: String(line.id),
          quantity: minorToDecimal(
            Number(line.quantity_milli) - Number(line.fulfilled_milli),
            3,
          ),
          expiresAt: i.expiresAt ?? Date.now() + 86400000,
          reason: i.reason,
        },
        `${key}:reserve:${line.id}`,
        write,
      );
      v++;
    }
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
          .bind(v, Date.now(), id),
      ],
    );
  }
  if (i.action === "sync-fulfillment") {
    const order = await getOrder(scoped,String(checkout.sales_order_id),write),
      now=Date.now(),links=order.fulfillments.map(f=>write
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
  }
  if (i.action === "request-return") {
    if (!i.fulfillmentId || !i.fulfillmentVersion || !i.date || !i.lines)
      throw new ApiError(
        422,
        "RETURN_DETAIL_REQUIRED",
        "Fulfillment, version, date and lines are required.",
      );
    if(!await write.prepare('SELECT 1 FROM commerce_fulfillments WHERE id=? AND order_id=? AND version=?').bind(i.fulfillmentId,checkout.sales_order_id,i.fulfillmentVersion).first())throw new ApiError(404,'ECOMMERCE_FULFILLMENT_NOT_FOUND','Select a current delivery from this checkout.');
    const ret = await requestCommerceReturn(
        scoped,
        i.fulfillmentId,
        returnRequestSchema.parse({
          expectedVersion: i.fulfillmentVersion,
          date: i.date,
          reason: i.reason,
          lines: i.lines,
        }),
        `${key}:return`,
        write,
      ),
      now = Date.now();
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
  }
  throw new ApiError(
    409,
    "INVALID_ECOMMERCE_TRANSITION",
    "Unsupported checkout action.",
  );
}
export async function ecommerceDetail(a: WorkContext, id: string) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    checkout = await write
      .prepare(
        `SELECT x.*,c.channel_id,c.party_id,c.warehouse_id,c.branch_id,c.department_id,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN commerce_orders o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope}`,
      )
      .bind(id,...scopeValues(a))
      .first<Row>();
  if (!checkout)
    throw new ApiError(404, "CHECKOUT_NOT_FOUND", "Checkout not found.");
  const order = await getOrder(
    {
      ...a,
      branchId: String(checkout.branch_id),
      departmentId: checkout.department_id
        ? String(checkout.department_id)
        : null,
    },
    String(checkout.sales_order_id),
  );
  return {
    checkout,
    order,
    fulfillmentLinks: (
      await write
        .prepare(
          "SELECT * FROM ecommerce_fulfillment_links WHERE checkout_id=?",
        )
        .bind(id)
        .all<Row>()
    ).results,
    returnLinks: (
      await write
        .prepare("SELECT l.*,r.status current_return_status,r.version current_return_version FROM ecommerce_return_links l JOIN commerce_returns r ON r.id=l.commerce_return_id WHERE l.checkout_id=?")
        .bind(id)
        .all<Row>()
    ).results,
  };
}
