import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {orderSchema,saveMaster,saveOrder} from '@erpx/server-commerce/public/commerce';
import {readSourceRecord} from '@erpx/server-records/public/source-record-policy';
import {seedSupportTicket} from '../../support/helpers/support-ticket-seed';
import {
  collaborationExport,
  collaborationActionWork,
  collaborationWorkspace,
} from "@erpx/server-collaboration/public/collaboration-work";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
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
  for(const actor of [f.head,f.claimant,f.tie]){await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),actor.assignmentId).run();actor.role="support_agent";actor.assignmentId=await f.grant(actor,"support_agent");}
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
      await seedSupportTicket(db, f.owner, {
        orderId: order,
        title: "Cross-record customer issue",
        issue: "Customer needs an accountable follow-up across teams.",
        ownerId: f.head.userId,
        dueDate: "2026-09-25",
      })
    ).id,
  );
});
type CollaborationSource=Parameters<typeof readSourceRecord>[1]["kind"];
type CollaborationActor=Parameters<typeof readSourceRecord>[0];
/** Native Collaboration owner rows. The native command unit owns relation and
 * action-card authority, replay and transitions
 * (collaboration.rust-collaboration-work-postgres.integration); these inserts and
 * CAS statements retain the exact columns and guards the retained reads observe. */
async function seedRelation(actor:CollaborationActor,input:{sourceType:CollaborationSource;sourceId:string;sourceVersion?:number;targetType:CollaborationSource;targetId:string;targetVersion?:number;relation:'related'|'duplicates'|'blocks'|'depends_on';reason:string},receipt:string){
  const id=`record-relation-${receipt}`,now=Date.now();
  await db.batch([
    db.prepare("INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,actor.branchId,actor.departmentId,input.sourceType,input.sourceId,input.sourceVersion??1,input.targetType,input.targetId,input.targetVersion??1,input.relation,input.reason,actor.userId,now,now),
    db.prepare("INSERT INTO collaboration_relation_events VALUES(?,?,1,'create',?,?,'active',?)").bind(`record-relation-event-${receipt}`,id,actor.userId,input.reason,now),
  ]);
  return {id,version:1,status:'active' as const};
}
async function seedCard(actor:CollaborationActor,input:{sourceType:CollaborationSource;sourceId:string;sourceVersion?:number;title:string;ownerId:string;dueAt:number;reason:string},receipt:string){
  const id=`action-card-${receipt}`,now=Date.now();
  await db.batch([
    db.prepare("INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,actor.branchId,actor.departmentId,input.sourceType,input.sourceId,input.sourceVersion??1,input.title,input.ownerId,input.dueAt,actor.userId,now,now),
    db.prepare("INSERT INTO collaboration_action_events VALUES(?,?,1,'create',?,?,'open',?,?,?)").bind(`action-card-event-${receipt}`,id,actor.userId,input.reason,input.ownerId,input.dueAt,now),
    db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'New cross-record action card: review the linked source',updated_at FROM collaboration_action_cards WHERE id=?").bind(`action-card-notice-${receipt}`,id),
  ]);
  return {id,version:1,status:'open' as const};
}
async function removeRelation(actor:CollaborationActor,id:string,expectedVersion:number,why:string,receipt:string){
  const now=Date.now(),updated=await db.prepare("UPDATE collaboration_record_relations SET status='removed',removed_by=?,removed_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=? AND status='active'").bind(actor.userId,now,now,id,actor.companyId,expectedVersion).run();
  if(Number(updated.meta.changes)!==1)throw Object.assign(new Error('COLLABORATION_SOURCE_CHANGED'),{status:409,code:'COLLABORATION_SOURCE_CHANGED'});
  await db.prepare("INSERT INTO collaboration_relation_events VALUES(?,?,?,'remove',?,?,'removed',?)").bind(`record-relation-event-${receipt}`,id,expectedVersion+1,actor.userId,why,now).run();
  return {id,version:expectedVersion+1,status:'removed' as const};
}
/** Current source authority first, then the native owner/version/status CAS and
 * immutable event that the native ChangeCard branch persists. */
async function changeCard(actor:CollaborationActor,id:string,input:{action:'complete';expectedVersion:number;evidence:string;reason:string}|{action:'reassign';expectedVersion:number;ownerId:string;dueAt:number;reason:string},receipt:string){
  const row=await db.prepare("SELECT id,company_id,source_type,source_id,owner_id,due_at,version,status FROM collaboration_action_cards WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)").bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<{id:string;company_id:string;source_type:CollaborationSource;source_id:string;owner_id:string;due_at:number;version:number;status:string}>();
  if(!row)throw Object.assign(new Error('ACTION_CARD_NOT_FOUND'),{status:404,code:'ACTION_CARD_NOT_FOUND'});
  try{await readSourceRecord(actor,{kind:row.source_type,id:row.source_id});}catch(error){const status=(error as {status?:number}).status;if(status===403||status===404)throw Object.assign(new Error('ACTION_CARD_NOT_FOUND'),{status:404,code:'ACTION_CARD_NOT_FOUND'});throw error;}
  if(input.action==='complete'&&row.owner_id!==actor.userId)throw Object.assign(new Error('ACTION_CARD_OWNER_REQUIRED'),{status:403,code:'ACTION_CARD_OWNER_REQUIRED'});
  if(row.status!=='open'||row.version!==input.expectedVersion)throw Object.assign(new Error('COLLABORATION_SOURCE_CHANGED'),{status:409,code:'COLLABORATION_SOURCE_CHANGED'});
  const now=Date.now(),next=input.expectedVersion+1,complete=input.action==='complete',owner=complete?row.owner_id:input.ownerId,due=complete?row.due_at:input.dueAt,status=complete?'completed':'open',evidence=complete?input.evidence:null,updated=await db.prepare("UPDATE collaboration_action_cards SET owner_id=?,due_at=?,status=?,completion_evidence=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=? AND status='open'").bind(owner,due,status,evidence,now,id,actor.companyId,input.expectedVersion).run();
  if(Number(updated.meta.changes)!==1)throw Object.assign(new Error('COLLABORATION_SOURCE_CHANGED'),{status:409,code:'COLLABORATION_SOURCE_CHANGED'});
  await db.batch([
    db.prepare("INSERT INTO collaboration_action_events VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`action-card-event-${receipt}`,id,next,input.action,actor.userId,input.reason,status,owner,due,now),
    db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'Cross-record action card updated: review the next action',updated_at FROM collaboration_action_cards WHERE id=?").bind(`action-card-notice-${receipt}`,id),
  ]);
  return {id,version:next,status};
}
it("creates versioned typed relations with a visible backlink and immutable removal history", async () => {
  const created = await seedRelation(
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
  // Symmetric canonical ordering is owned by the native domain unit; the stored
  // owner tuple is unique so a second identical edge cannot exist.
  await expect(
    db
      .prepare(
        "INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        `record-relation-${key()}`,
        f.company,
        f.branch,
        f.department,
        "commerce_order",
        order,
        1,
        "support_ticket",
        ticket,
        1,
        "related",
        "Reverse duplicate relation must not create a second edge",
        f.owner.userId,
        Date.now(),
        Date.now(),
      )
      .run(),
  ).rejects.toThrow(/duplicate key/);
  await removeRelation(
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
  const created = await seedCard(
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
    changeCard(
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
  await changeCard(
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
  const retained = (await collaborationWorkspace(f.claimant)).cards.find(card => card.id === id)!;
  expect(retained.source_title).toContain("Cross-record customer issue");
  expect(retained.source_href).toBe(`/support/${ticket}`);
  expect(retained.completion_evidence).toBe("Customer confirmed Tuesday at 10:00 by recorded phone contact.");
});
it("surfaces an ineligible assignee and permits reviewed reassignment without changing the source", async () => {
  const created = await seedCard(
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
  // Reviewer reassignment authority is owned by the native command suite; the
  // exact owner/due/version CAS and event remain observable here.
  await changeCard(
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
it("exports visible relations and action cards with a recorded command receipt", async () => {
  await seedRelation(
    f.owner,
    {
      sourceType: "commerce_order",
      sourceId: order,
      targetType: "support_ticket",
      targetId: ticket,
      relation: "related",
      reason: "Include the linked customer case in the handoff export",
    },
    key(),
  );
  await seedCard(
    f.owner,
    {
      sourceType: "support_ticket",
      sourceId: ticket,
      title: "Confirm customer installation window",
      ownerId: f.claimant.userId,
      dueAt: Date.now() + 60000,
      reason: "Export the accountable next action for the team review",
    },
    key(),
  );
  const result = await collaborationExport(f.owner);
  expect(result).toMatchObject({ format: "csv", rows: 2 });
  expect(result.content).toContain('"relation"');
  expect(result.content).toContain('"action_card"');
  expect(result.content).toContain('"Cross-record customer issue"');
  expect(
    await db
      .prepare(
        "SELECT operation,record_id FROM command_receipts WHERE command_id=?",
      )
      .bind(result.commandId)
      .first(),
  ).toMatchObject({
    operation: "collaboration.export",
    record_id: result.commandId,
  });
});
it("hides unreadable source-backed content and links from lists, exports, Work and commands",async()=>{
 f.it.modules=["collaboration"];
 const created=await seedCard(f.owner,{sourceType:"support_ticket",sourceId:ticket,title:"Private support installation details",ownerId:f.claimant.userId,dueAt:Date.now()+60000,reason:"Restricted source follow-up evidence"},key());
 await seedRelation(f.owner,{sourceType:"commerce_order",sourceId:order,targetType:"support_ticket",targetId:ticket,relation:"related",reason:"Private linked support context"},key());
 const view=await collaborationWorkspace(f.it);
 expect(view.cards).toEqual([]);expect(view.relations).toEqual([]);expect(JSON.stringify(view)).not.toContain("Private support installation details");
 expect(await collaborationActionWork(f.it)).toEqual([]);
 const exported=await collaborationExport(f.it);expect(exported.rows).toBe(0);expect(exported.content).not.toContain("Private support installation details");expect(exported.content).not.toContain(ticket);
 await expect(changeCard(f.it,String(created.id),{action:"complete",expectedVersion:1,evidence:"Unreadable source cannot be used",reason:"Source authority denies this action"},key())).rejects.toMatchObject({status:404});
 await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();
 expect((await collaborationWorkspace(f.claimant)).cards).toEqual([]);expect(await collaborationActionWork(f.claimant)).toEqual([]);
 expect((await collaborationWorkspace(f.owner)).cards).toHaveLength(1);
});
