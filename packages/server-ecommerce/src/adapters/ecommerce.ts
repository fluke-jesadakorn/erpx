import {commerceOwnerTransaction} from "@erpx/server-commerce/public/purchase-boundary";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Row } from "@erpx/server-commerce/public/commerce";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { decimalToMinor } from "@erpx/feature-finance/lib/finance-money";
import { snapshotMoney } from "@erpx/server-finance/public/company-money";
import { isTransactionalSqlDatabase, type SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { changeOrderReservation, getOrder, orderSchema, requestCommerceReturn, returnRequestSchema, saveOrder, transitionOrder } from "@erpx/server-commerce/public/commerce";
import { createEcommerceApplication, type EcommerceRepository } from "@erpx/server-ecommerce/application/commerce";
import { ecommerceCurrencyConfiguration, ecommerceProductCandidates, ecommerceWarehouseCandidates, ecommerceWarehouse, ecommerceFulfillmentCurrent } from "@erpx/server-commerce/public/ecommerce-sources";
import { ecommerceCustomerCandidates } from "@erpx/server-finance/public/ecommerce-customers";
import {commercePackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {channelSchema} from '@erpx/server-ecommerce/domain/channel';
import {listingSchema,cartSchema,lineSchema,checkoutSchema,actionSchema} from '@erpx/server-ecommerce/domain/commerce';
import {requireEcommercePurchasedRead,ecommercePurchaseControls,withEcommercePurchasedCommand,type EcommercePurchaseCommand} from '@erpx/server-ecommerce/adapters/purchase-boundary';
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
  ...(canStartTransaction && isTransactionalSqlDatabase(write) ? { transaction: <Result>(action: (repository: EcommerceRepository) => Promise<Result>) => commerceOwnerTransaction(write,a,tx => action(bindRepository(tx, a, false))) } : {}),
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
  async abandonCart(command,now){const {key,operation,input,id}=command;return executeCommand(write,a,key,operation,input,id,input.expectedVersion+1,{id,status:'abandoned',version:input.expectedVersion+1},[
    assertion(write,key,'cart-abandon',"EXISTS(SELECT 1 FROM ecommerce_carts c WHERE c.id=? AND c.version=? AND c.status='open' AND NOT EXISTS(SELECT 1 FROM ecommerce_checkouts checkout WHERE checkout.cart_id=c.id))",[id,input.expectedVersion]),
    write.prepare("UPDATE ecommerce_carts SET status='abandoned',version=version+1,updated_at=? WHERE id=?").bind(now,id),
  ]);},
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
  salesOrder: (actor, id) => getOrder(actor, id, write),
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
async function command(a:WorkContext,metadata:EcommercePurchaseCommand,run:(owner:ReturnType<typeof application>)=>Promise<Record<string,unknown>>,provided?:SqlDatabase){
 if(await commercePackageEnforcementMode()==='disabled')return run(application(a,provided));
 return withEcommercePurchasedCommand(a,metadata,database=>run(application(a,database)),provided);
}
export async function ecommerceWorkspace(a: WorkContext) {access(a);const write=(await holdingResources(a.holdingId)).write;await requireEcommercePurchasedRead(write,a);const data=await application(a,write).ecommerceWorkspace(),purchase=await ecommercePurchaseControls(write,a);return {...data,...(purchase?{purchase}: {})};}
export async function createChannel(a: WorkContext, raw: unknown, key: string) {access(a,true);const input=channelSchema.parse(raw),id=`ecommerce-channel-${key}`;return command(a,{operation:'ecommerce.channel.create',key,id,purpose:'new-work',after:{kind:'channel',id}},owner=>owner.createChannel(input,key));}
export async function createListing(a: WorkContext, raw: unknown, key: string) {access(a,true);const input=listingSchema.parse(raw),id=`ecommerce-listing-${key}`;return command(a,{operation:'ecommerce.listing.create',key,id,purpose:'new-work',after:{kind:'listing',id}},owner=>owner.createListing(input,key));}
export async function createCart(a: WorkContext, raw: unknown, key: string) {access(a,true);const input=cartSchema.parse(raw),id=`ecommerce-cart-${key}`;return command(a,{operation:'ecommerce.cart.create',key,id,purpose:'new-work',after:{kind:'cart',id}},owner=>owner.createCart(input,key));}
export async function addCartLine(a: WorkContext, raw: unknown, key: string) {access(a,true);const input=lineSchema.parse(raw),target={kind:'cart' as const,id:input.cartId};return command(a,{operation:'ecommerce.cart.add',key,id:input.cartId,purpose:'new-work',before:target,after:target},owner=>owner.addCartLine(input,key));}
export async function checkoutCart(a: WorkContext, raw: unknown, key: string, ownerWrite?: SqlDatabase) {access(a,true);const input=checkoutSchema.parse(raw),id=`ecommerce-checkout-${input.cartId}`;return command(a,{operation:'ecommerce.checkout',key,id,purpose:'new-work',before:{kind:'cart',id:input.cartId},after:{kind:'checkout',id}},owner=>owner.checkoutCart(input,key),ownerWrite);}
export async function changeEcommerce(a: WorkContext, kind: "channel" | "listing" | "checkout" | "cart", id: string, raw: unknown, key: string, ownerWrite?: SqlDatabase) {access(a,true);if(!['channel','listing','checkout','cart'].includes(kind))throw new ApiError(422,'ECOMMERCE_ENTITY_REQUIRED','Choose an original Ecommerce command.');const input=actionSchema.parse(raw),target={kind,id},purpose=['activate','publish','approve-order'].includes(input.action)?'new-work' as const:'settle-existing' as const;return command(a,{operation:`ecommerce.${kind}.${input.action}`,key,id,purpose,review:kind==='channel'||kind==='listing',before:target,after:target},owner=>owner.changeEcommerce(kind,id,input,key),ownerWrite);}
export async function ecommerceDetail(a: WorkContext, id: string) {access(a);const write=(await holdingResources(a.holdingId)).write;await requireEcommercePurchasedRead(write,a,{kind:'checkout',id});const data=await application(a,write).ecommerceDetail(id),purchase=await ecommercePurchaseControls(write,a);return {...data,...(purchase?{purchase}: {})};}
