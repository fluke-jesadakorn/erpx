import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import { deliveryEvidence } from "./helpers/delivery-evidence";
import { fulfillOrder, getOrder, saveMaster } from "../src/server/commerce";
import { assignFlowPrimary, resolveFlow } from "../src/server/flow-routing";
import type { WorkContext } from "../src/server/authorization";
import {
  addCartLine,
  changeEcommerce,
  checkoutCart,
  createCart,
  createChannel,
  createListing,
  ecommerceDetail,
} from "../src/server/ecommerce";

vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let fixture: Awaited<ReturnType<typeof expenseFixture>>,
  owner: WorkContext,
  reviewer: WorkContext,
  warehouse: string,
  product: string,
  party: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  fixture = await expenseFixture();
  owner = { ...fixture.owner, modules: ["ecommerce"] };
  reviewer = {
    ...fixture.tie,
    assignmentId: await fixture.grant(fixture.tie, "holding_owner"),
    role: "holding_owner",
    modules: ["ecommerce"],
  };
  warehouse = String(
    (
      await saveMaster(
        fixture.owner,
        "warehouses",
        null,
        { branchId: fixture.branch, code: key(), name: "Online warehouse" },
        key(),
      )
    ).id,
  );
  product = String(
    (
      await saveMaster(
        fixture.owner,
        "products",
        null,
        { sku: key(), name: "Online item", price: "20" },
        key(),
      )
    ).id,
  );
  party = String(
    (
      await saveMaster(
        fixture.owner,
        "parties",
        null,
        {
          kind: "customer",
          code: key(),
          name: "Online customer",
          taxId: key(),
          currency: "THB",
          paymentTermsDays: 7,
          creditLimit: "10000",
        },
        key(),
      )
    ).id,
  );
  await db
    .prepare(
      "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)",
    )
    .bind(warehouse, product)
    .run();
});

async function publishedListing() {
  const channel = await createChannel(
    owner,
    {
      name: "Web shop",
      channelType: "web",
      configuration: { storefront: "sandbox" },
    },
    key(),
  );
  await expect(
    changeEcommerce(
      owner,
      "channel",
      String(channel.id),
      {
        action: "activate",
        expectedVersion: 1,
        reason: "Self review is forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeEcommerce(
    reviewer,
    "channel",
    String(channel.id),
    {
      action: "activate",
      expectedVersion: 1,
      reason: "Channel configuration reviewed",
    },
    key(),
  );
  const listing = await createListing(
    owner,
    {
      channelId: channel.id,
      productId: product,
      title: "Published item",
      currency: "THB",
      price: "25.00",
    },
    key(),
  );
  await changeEcommerce(
    reviewer,
    "listing",
    String(listing.id),
    {
      action: "publish",
      expectedVersion: 1,
      reason: "Price and product reviewed",
    },
    key(),
  );
  return { channel, listing };
}

async function submittedCheckout(quantity = "1") {
  const { channel, listing } = await publishedListing();
  const cart = await createCart(
    owner,
    {
      channelId: channel.id,
      partyId: party,
      warehouseId: warehouse,
      currency: "THB",
    },
    key(),
  );
  const line = await addCartLine(
    owner,
    {
      cartId: cart.id,
      expectedVersion: 1,
      listingId: listing.id,
      listingVersion: 2,
      quantity,
    },
    key(),
  );
  const checkoutKey = key();
  const checkout = await checkoutCart(
    owner,
    {
      cartId: cart.id,
      expectedVersion: line.version,
      orderDate: "2026-09-21",
      dueDate: "2026-09-28",
    },
    checkoutKey,
  );
  expect(
    await checkoutCart(
      owner,
      {
        cartId: cart.id,
        expectedVersion: line.version,
        orderDate: "2026-09-21",
        dueDate: "2026-09-28",
      },
      checkoutKey,
    ),
  ).toEqual(checkout);
  return checkout;
}

async function assignStock(orderId: string) {
  const flow = await resolveFlow(db, owner, "order", orderId);
  await assignFlowPrimary(
    db,
    owner,
    "order",
    orderId,
    {
      stepKey: flow.current!.key,
      userId: owner.userId,
      reason: "Assign E-commerce fulfillment owner",
      saveDefault: false,
      expectedVersion: flow.version,
      recordVersion: flow.recordVersion,
    },
    key(),
  );
}

it("reviews channels and listings, locks checkout into Sales, and reserves shared stock", async () => {
  const checkout = await submittedCheckout("2");
  const result = await changeEcommerce(
    reviewer,
    "checkout",
    String(checkout.id),
    {
      action: "approve-order",
      expectedVersion: 1,
      reason: "Order and stock reviewed",
      expiresAt: Date.now() + 3600000,
    },
    key(),
  );
  expect(result).toMatchObject({ status: "confirmed", salesOrderVersion: 4 });
  expect(
    await db
      .prepare(
        "SELECT r.quantity_milli FROM inventory_order_reservations r JOIN commerce_order_lines l ON l.id=r.order_line_id WHERE l.order_id=?",
      )
      .bind(checkout.salesOrderId)
      .first("quantity_milli"),
  ).toBe(2000);
  const detail = await ecommerceDetail(owner, String(checkout.id));
  expect(detail.order.order).toMatchObject({ state: "confirmed", version: 4 });
});

it("fulfills only reserved stock and syncs immutable delivery evidence", async () => {
  const checkout = await submittedCheckout();
  await changeEcommerce(
    reviewer,
    "checkout",
    String(checkout.id),
    {
      action: "approve-order",
      expectedVersion: 1,
      reason: "Ready to fulfill",
      expiresAt: Date.now() + 3600000,
    },
    key(),
  );
  await assignStock(String(checkout.salesOrderId));
  const order = await getOrder(owner, String(checkout.salesOrderId));
  await fulfillOrder(
    owner,
    String(checkout.salesOrderId),
    {
      expectedVersion: Number(order.order.version),
      date: "2026-09-22",
      lines: [{ lineId: order.lines[0].id, quantity: "1" }],
      ...(await deliveryEvidence(db, owner, String(checkout.salesOrderId))),
    },
    key(),
  );
  const synced = await changeEcommerce(
    owner,
    "checkout",
    String(checkout.id),
    {
      action: "sync-fulfillment",
      expectedVersion: 2,
      reason: "Delivery created by Sales",
    },
    key(),
  );
  expect(synced.status).toBe("fulfilled");
  const detail = await ecommerceDetail(owner, String(checkout.id));
  expect(detail.fulfillmentLinks).toHaveLength(1);
  expect(
    await db
      .prepare(
        "SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
      )
      .bind(warehouse, product)
      .first("quantity_milli"),
  ).toBe(1000);
});

it("rejects stale carts and refuses a return before the Sales fulfillment is invoiced", async () => {
  const checkout = await submittedCheckout();
  await expect(
    createListing(
      owner,
      {
        channelId: "missing",
        productId: product,
        title: "Invalid listing",
        currency: "THB",
        price: "1",
      },
      key(),
    ),
  ).rejects.toBeTruthy();
  await changeEcommerce(
    reviewer,
    "checkout",
    String(checkout.id),
    {
      action: "approve-order",
      expectedVersion: 1,
      reason: "Ready to fulfill",
      expiresAt: Date.now() + 3600000,
    },
    key(),
  );
  await assignStock(String(checkout.salesOrderId));
  const order = await getOrder(owner, String(checkout.salesOrderId));
  await fulfillOrder(
    owner,
    String(checkout.salesOrderId),
    {
      expectedVersion: Number(order.order.version),
      date: "2026-09-22",
      lines: [{ lineId: order.lines[0].id, quantity: "1" }],
      ...(await deliveryEvidence(db, owner, String(checkout.salesOrderId))),
    },
    key(),
  );
  const delivered = await getOrder(owner, String(checkout.salesOrderId));
  await expect(
    changeEcommerce(
      owner,
      "checkout",
      String(checkout.id),
      {
        action: "request-return",
        expectedVersion: 2,
        reason: "Customer requested return",
        fulfillmentId: delivered.fulfillments[0].id,
        fulfillmentVersion: Number(delivered.fulfillments[0].version),
        date: "2026-09-23",
        lines: [{ lineId: delivered.fulfillmentLines[0].id, quantity: "1" }],
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "INVOICED_FULFILLMENT_NOT_FOUND" });
});
