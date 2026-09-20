import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import { orderSchema, saveMaster, saveOrder } from "../src/server/commerce";
import { createSupportTicket } from "../src/server/support";
import {
  changeActionCard,
  collaborationActionWork,
  collaborationWorkspace,
  createActionCard,
  createRecordRelation,
  removeRecordRelation,
} from "../src/server/collaboration-work";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  order: string,
  ticket: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  f.head.role = "support_agent";
  f.head.assignmentId = await f.grant(f.head, "support_agent");
  for (const a of [f.owner, f.head, f.claimant, f.tie])
    a.modules = ["collaboration", "support"];
  const customer = await saveMaster(
      f.owner,
      "parties",
      null,
      { kind: "customer", code: key(), name: "Collaboration customer" },
      key(),
    ),
    warehouse = await saveMaster(
      f.owner,
      "warehouses",
      null,
      { branchId: f.branch, code: key(), name: "Collaboration warehouse" },
      key(),
    ),
    product = await saveMaster(
      f.owner,
      "products",
      null,
      { sku: key(), name: "Collaboration item", price: "10" },
      key(),
    ),
    source = await saveOrder(
      f.owner,
      null,
      orderSchema.parse({
        kind: "quote",
        partyId: customer.id,
        warehouseId: warehouse.id,
        orderDate: "2026-09-20",
        dueDate: "2026-09-25",
        lines: [
          { productId: product.id, quantity: "1", price: "10", taxBps: 0 },
        ],
      }),
      key(),
    );
  order = String(source.id);
  ticket = String(
    (
      await createSupportTicket(
        f.owner,
        {
          orderId: order,
          sourceVersion: 1,
          title: "Cross-record customer issue",
          issue: "Customer needs an accountable follow-up across teams.",
          ownerId: f.head.userId,
          dueDate: "2026-09-25",
        },
        key(),
      )
    ).id,
  );
});
it("creates versioned typed relations with a visible backlink and immutable removal history", async () => {
  const created = await createRecordRelation(
      f.owner,
      {
        sourceType: "commerce_order",
        sourceId: order,
        targetType: "support_ticket",
        targetId: ticket,
        relation: "related",
        reason: "Support case originates from this customer quotation",
      },
      key(),
    ),
    id = String(created.id),
    view = await collaborationWorkspace(f.owner);
  expect(view.relations.find((x) => x.id === id)).toMatchObject({
    source_type: "commerce_order",
    source_id: order,
    source_version: 1,
    target_type: "support_ticket",
    target_id: ticket,
    target_version: 1,
    status: "active",
  });
  await expect(
    createRecordRelation(
      f.owner,
      {
        sourceType: "support_ticket",
        sourceId: ticket,
        targetType: "commerce_order",
        targetId: order,
        relation: "related",
        reason: "Reverse duplicate relation must not create a second edge",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await removeRecordRelation(
    f.owner,
    id,
    1,
    "The support case was linked to the wrong source",
    key(),
  );
  expect(
    (await collaborationWorkspace(f.owner)).relations.some((x) => x.id === id),
  ).toBe(false);
  await expect(
    db
      .prepare("DELETE FROM collaboration_relation_events WHERE relation_id=?")
      .bind(id)
      .run(),
  ).rejects.toThrow("COLLABORATION_HISTORY_IMMUTABLE");
});
it("routes action cards to the current owner and requires completion evidence", async () => {
  const created = await createActionCard(
      f.owner,
      {
        sourceType: "support_ticket",
        sourceId: ticket,
        title: "Confirm customer installation window",
        ownerId: f.claimant.userId,
        dueAt: Date.now() + 60000,
        reason: "Sales and Support need one accountable next action",
      },
      key(),
    ),
    id = String(created.id);
  expect(
    (await collaborationActionWork(f.claimant)).find((x) => x.id === id),
  ).toMatchObject({ queue: "todo", canAct: true });
  await expect(
    changeActionCard(
      f.owner,
      id,
      {
        action: "complete",
        expectedVersion: 1,
        evidence: "Manager cannot complete another owner action.",
        reason: "Wrong actor",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "ACTION_CARD_OWNER_REQUIRED" });
  await changeActionCard(
    f.claimant,
    id,
    {
      action: "complete",
      expectedVersion: 1,
      evidence:
        "Customer confirmed Tuesday at 10:00 by recorded phone contact.",
      reason: "Action completed with source evidence",
    },
    key(),
  );
  expect(
    (await collaborationActionWork(f.claimant)).find((x) => x.id === id)?.queue,
  ).toBe("completed");
});
it("surfaces an ineligible assignee and permits reviewed reassignment without changing the source", async () => {
  const created = await createActionCard(
      f.owner,
      {
        sourceType: "support_ticket",
        sourceId: ticket,
        title: "Prepare field visit context",
        ownerId: f.claimant.userId,
        dueAt: Date.now() + 60000,
        reason: "Field technician needs verified site details",
      },
      key(),
    ),
    id = String(created.id);
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE id=?")
    .bind(Date.now(), f.claimant.assignmentId)
    .run();
  expect(
    (await collaborationActionWork(f.owner)).find((x) => x.id === id)?.blocker,
  ).toContain("no longer eligible");
  await changeActionCard(
    f.owner,
    id,
    {
      action: "reassign",
      expectedVersion: 1,
      ownerId: f.tie.userId,
      dueAt: Date.now() + 120000,
      reason: "Reassign after original owner access ended",
    },
    key(),
  );
  expect(
    (await collaborationActionWork(f.tie)).find((x) => x.id === id),
  ).toMatchObject({ queue: "todo", canAct: true });
  expect(
    await db
      .prepare("SELECT version FROM support_tickets WHERE id=?")
      .bind(ticket)
      .first("version"),
  ).toBe(1);
  await expect(
    db
      .prepare("UPDATE collaboration_action_cards SET source_id=? WHERE id=?")
      .bind(order, id)
      .run(),
  ).rejects.toThrow("COLLABORATION_ACTION_SOURCE_IMMUTABLE");
});
