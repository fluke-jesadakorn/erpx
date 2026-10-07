import { env } from "./runtime-env";
import { applyMigrations, type TestMigration } from "./runtime-env";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import type { WorkContext, WorkAssignment } from "../src/server/authorization";
import {
  saveServiceTeam,
  serviceTeamSchema,
  resolveServiceTeam,
} from "../src/server/service-teams";
import {
  actOnEmployeeRequest,
  employeeRequestDetail,
  employeeRequestSchema,
  getEmployeeRequest,
  requestActionSchema,
  saveEmployeeRequest,
  prepareRequestOrderHandoff,
  employeeRequestWork,
} from "../src/server/employee-requests";
import { executeCommand } from "../src/server/commands";
import { serviceAuthoritySnapshot } from "../src/server/service-access";
import {
  actOnHandbook,
  handbookArticle,
  handbookList,
  handbookSchema,
  saveHandbookArticle,
  setHandbookEditor,
} from "../src/server/handbook";
import {
  responsibilityContext,
  responsibilityRoles,
} from "../src/lib/responsibilities";
import { commercePermissions } from "../src/server/commerce";
import { resourceRequestRoute } from "../src/server/expense-routing";

vi.mock("server-only", () => ({}));
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});
beforeEach(async () => {
  f = await expenseFixture();
});
async function team(
  category = "it",
  scopeKind: WorkAssignment["scopeKind"] = "holding",
  scopeNodeId = f.company,
  lead = f.it,
) {
  const input = serviceTeamSchema.parse({
    name: `${category} support`,
    members: [{ userId: lead.userId, responsibility: "lead" }],
    coverage: [{ category, scopeKind, scopeNodeId }],
  });
  return { ...(await saveServiceTeam(db, f.owner, null, input, key())), input };
}
async function request(kind = "service", actor = f.claimant, extra = {}) {
  const input = employeeRequestSchema.parse({
    kind,
    title: "Office support",
    description: "Please help with the broken office equipment.",
    location: "Office desk",
    branchId: f.branch,
    departmentId: f.department,
    neededDate: "2026-09-20",
    items: kind === "service" ? [] : [{ description: "Laptop", quantity: "1" }],
    ...extra,
  });
  return {
    ...(await saveEmployeeRequest(db, actor, null, input, key())),
    input,
  };
}
async function act(actor: WorkContext, id: string, action: string, extra = {}) {
  const row = await getEmployeeRequest(db, actor, id);
  return actOnEmployeeRequest(
    db,
    actor,
    id,
    requestActionSchema.parse({
      action,
      expectedVersion: row.version,
      note: "Reviewed and confirmed with supporting details",
      ...extra,
    }),
    key(),
  );
}
async function assigned(kind = "service") {
  await team(
    kind === "equipment"
      ? "operations"
      : kind === "purchase"
        ? "purchasing"
        : "it",
  );
  const r = await request(kind);
  await act(f.claimant, r.id, "submit");
  if (kind !== "service") await act(f.head, r.id, "approve");
  if(kind === "purchase"){await act(f.reviewer,r.id,"approve");await act(f.manager,r.id,"approve");}
  await act(f.it, r.id, "assign", { assigneeUserId: f.it.userId });
  await act(f.it, r.id, "start");
  return r;
}
it("allows incomplete service drafts without profiles; purchases require an engagement", async () => {
  const missing = await f.member("missing", "employee", 1, f.department, false);
  missing.beta = true;
  await expect(saveEmployeeRequest(db, missing, null, employeeRequestSchema.parse({kind:"purchase"}), key())).rejects.toMatchObject({code:"STAFF_LINK_REQUIRED"});
  for (const kind of ["service", "equipment"]) {
    const input = employeeRequestSchema.parse({ kind });
    const command = key();
    const saved = await saveEmployeeRequest(db, missing, null, input, command);
    expect(
      await saveEmployeeRequest(db, missing, null, input, command),
    ).toEqual(saved);
    expect(
      (await getEmployeeRequest(db, missing, saved.id)).requester_user_id,
    ).toBe(missing.userId);
    await expect(act(missing, saved.id, "submit")).rejects.toMatchObject({
      code: "REQUEST_DETAILS_REQUIRED",
    });
    expect((await getEmployeeRequest(db, missing, saved.id)).state).toBe(
      "draft",
    );
  }
});
it("routes a no-profile service request directly, without granting business or IT authority", async () => {
  await team();
  const missing = await f.member("missing", "employee", 1, f.department, false);
  missing.beta = true;
  const r = await request("service", missing);
  await act(missing, r.id, "submit");
  expect((await getEmployeeRequest(db, missing, r.id)).state).toBe("queued");
  expect(commercePermissions(f.it).purchase).toBe(false);
  expect(
    (
      await db
        .prepare(
          "SELECT count(*) n FROM assignments WHERE user_id=? AND role!='employee'",
        )
        .bind(missing.userId)
        .first<{ n: number }>()
    )?.n,
  ).toBe(0);
  await expect(
    saveServiceTeam(db, f.it, null, (await team("facilities")).input, key()),
  ).rejects.toMatchObject({ code: "OWNER_REQUIRED" });
});
it("keeps missing coverage requests and repairs routing with an audited owner reason", async () => {
  const r = await request();
  await act(f.claimant, r.id, "submit");
  expect(
    (await employeeRequestDetail(db, f.claimant, r.id)).blockedReason,
  ).toContain("coverage");
  await team();
  await expect(
    act(f.owner, r.id, "repair", { note: "" }),
  ).rejects.toMatchObject({ code: "REASON_REQUIRED" });
  await act(f.owner, r.id, "repair");
  const d = await employeeRequestDetail(db, f.claimant, r.id);
  expect(d.request.state).toBe("queued");
  expect(d.events.at(-1)?.action).toBe("repair");
});
it("uses department over branch over company, preserving open routing until explicit reassignment", async () => {
  const shared = await team();
  const old = await request();
  await act(f.claimant, old.id, "submit");
  const branch = await team("it", "branch", f.branch, f.head),
    local = await team("it", "department", f.department, f.tie);
  const scope = {
    company_id: f.company,
    branch_id: f.branch,
    department_id: f.department,
  };
  expect((await resolveServiceTeam(db, scope, "it"))?.team.id).toBe(local.id);
  expect(
    (
      await resolveServiceTeam(
        db,
        { ...scope, department_id: f.accountsDepartment },
        "it",
      )
    )?.team.id,
  ).toBe(branch.id);
  expect(
    (
      await resolveServiceTeam(
        db,
        { ...scope, branch_id: null, department_id: null },
        "it",
      )
    )?.team.id,
  ).toBe(shared.id);
  expect((await getEmployeeRequest(db, f.claimant, old.id)).team_id).toBe(
    shared.id,
  );
  await act(f.owner, old.id, "repair");
  expect((await getEmployeeRequest(db, f.claimant, old.id)).team_id).toBe(
    local.id,
  );
});
it("rejects ambiguous coverage, stale team changes, and leads without covered access", async () => {
  const t = await team();
  await expect(team()).rejects.toMatchObject({ status: 409 });
  await saveServiceTeam(
    db,
    f.owner,
    t.id,
    { ...t.input, name: "Revised team", expectedVersion: 1 },
    key(),
  );
  await expect(
    saveServiceTeam(db, f.owner, t.id, t.input, key()),
  ).rejects.toMatchObject({ code: "VERSION_CONFLICT" });
  await db
    .prepare(
      "UPDATE assignments SET scope_kind='department',scope_node_id=? WHERE user_id=? AND role='employee'",
    )
    .bind(f.department, f.it.userId)
    .run();
  await db
    .prepare(
      "UPDATE assignments SET revoked_at=? WHERE user_id=? AND role!='employee'",
    )
    .bind(Date.now(), f.it.userId)
    .run();
  await expect(team("engineering")).rejects.toMatchObject({ status: 422 });
});
it("supports conversation, requester questions, resolution, reopening and explicit final confirmation", async () => {
  const r = await assigned();
  await act(f.it, r.id, "ask");
  expect(
    (await employeeRequestWork(db, f.claimant)).find((i) => i.id === r.id)
      ?.queue,
  ).toBe("todo");
  await act(f.claimant, r.id, "reply");
  await act(f.claimant, r.id, "comment");
  expect(
    (await employeeRequestWork(db, f.claimant)).find((i) => i.id === r.id)
      ?.queue,
  ).toBe("waiting");
  await act(f.it, r.id, "resolve");
  expect((await getEmployeeRequest(db, f.claimant, r.id)).state).toBe(
    "resolved",
  );
  await act(f.claimant, r.id, "reopen");
  await act(f.it, r.id, "resolve");
  await act(f.claimant, r.id, "confirm");
  expect((await getEmployeeRequest(db, f.claimant, r.id)).state).toBe("closed");
  await expect(act(f.it, r.id, "confirm")).rejects.toMatchObject({
    status: 403,
  });
});
it("rejects revoked team access and prevents company, branch and department leakage", async () => {
  const r = await assigned();
  const stranger = await f.member("stranger");
  await expect(getEmployeeRequest(db, stranger, r.id)).rejects.toMatchObject({
    status: 404,
  });
  const other = await expenseFixture();
  await expect(getEmployeeRequest(db, other.owner, r.id)).rejects.toMatchObject(
    { status: 404 },
  );
  await expect(
    getEmployeeRequest(
      db,
      { ...f.owner, departmentId: f.accountsDepartment },
      r.id,
    ),
  ).rejects.toMatchObject({ status: 404 });
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?")
    .bind(Date.now(), f.it.userId)
    .run();
  await expect(act(f.it, r.id, "resolve")).rejects.toMatchObject({
    status: 404,
  });
  expect(
    (await employeeRequestDetail(db, f.owner, r.id)).blockedReason,
  ).toContain("unavailable");
});
it("revalidates membership, coverage and rank snapshots in the committing transaction", async () => {
  const snapshot = await serviceAuthoritySnapshot(db, f.company),
    command = key();
  await db
    .prepare("UPDATE staff_levels SET rank=50 WHERE id=?")
    .bind(`${f.company}-level-1`)
    .run();
  await expect(
    executeCommand(
      db,
      f.owner,
      command,
      "test.authority",
      {},
      f.company,
      1,
      {},
      snapshot(command),
    ),
  ).rejects.toMatchObject({ status: 409 });
  const next = await serviceAuthoritySnapshot(db, f.company),
    nextKey = key();
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE id=?")
    .bind(Date.now(), f.owner.assignmentId)
    .run();
  await expect(
    executeCommand(
      db,
      f.owner,
      nextKey,
      "test.authority",
      {},
      f.company,
      1,
      {},
      next(nextKey),
    ),
  ).rejects.toMatchObject({ status: 409 });
});
it("requires resource profiles, but department approval does not require Accounts or Treasury", async () => {
  const missing = await f.member("missing", "employee", 1, f.department, false),
    r = await request("equipment", missing);
  await expect(act(missing, r.id, "submit")).rejects.toMatchObject({
    status: 409,
  });
  await db
    .prepare(
      "UPDATE assignments SET revoked_at=? WHERE role IN ('finance_preparer','finance_approver','treasury')",
    )
    .bind(Date.now())
    .run();
  const route = await resourceRequestRoute(
    db,
    f.company,
    f.claimant.userId,
    f.claimant.userId,
  );
  expect(route.blockedReason).toBeNull();
  expect(route.steps).toHaveLength(1);
  expect(route.steps[0].candidateUserIds).toEqual(
    expect.arrayContaining([f.head.userId, f.tie.userId]),
  );
  const good = await request("equipment");
  await act(f.claimant, good.id, "submit");
  await act(f.head, good.id, "approve");
  expect(
    JSON.parse(
      (await getEmployeeRequest(db, f.claimant, good.id)).approval_json,
    ).decision,
  ).toBe("approved");
});
it("permits only one concurrent independent decision and preserves correction before resubmission", async () => {
  await team("operations");
  const r = await request("equipment");
  await act(f.claimant, r.id, "submit");
  await expect(act(f.claimant, r.id, "approve")).rejects.toMatchObject({
    status: 403,
  });
  await act(f.head, r.id, "return");
  let row = await getEmployeeRequest(db, f.claimant, r.id);
  expect(row.description).toBe(r.input.description);
  await saveEmployeeRequest(
    db,
    f.claimant,
    r.id,
    {
      ...r.input,
      title: "Corrected equipment purpose",
      expectedVersion: row.version,
    },
    key(),
  );
  await act(f.claimant, r.id, "submit");
  row = await getEmployeeRequest(db, f.head, r.id);
  const input = requestActionSchema.parse({
    action: "approve",
    expectedVersion: row.version,
    note: "Independent evidence reviewed",
  });
  const decisions = await Promise.allSettled(
    [f.head, f.tie].map((w) => actOnEmployeeRequest(db, w, r.id, input, key())),
  );
  expect(decisions.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect(
    (await employeeRequestDetail(db, f.claimant, r.id)).events.filter(
      (e) => e.action === "approve",
    ),
  ).toHaveLength(1);
});
it("excludes the service preparer from resource approvals and cannot approve spending by closing a service job", async () => {
  const service = await assigned();
  const child = await request("equipment", f.it, {
    parentRequestId: service.id,
  });
  const row = await getEmployeeRequest(db, f.it, child.id);
  expect(row.requester_user_id).toBe(f.claimant.userId);
  await act(f.it, child.id, "submit");
  await act(f.it, service.id, "resolve");
  await act(f.claimant, service.id, "confirm");
  expect((await getEmployeeRequest(db, f.claimant, child.id)).state).toBe(
    "awaiting_approval",
  );
  await expect(act(f.it, child.id, "approve")).rejects.toMatchObject({
    status: 403,
  });
});
it("records equipment handover separately from inventory and accounting postings", async () => {
  const r = await assigned("equipment");
  await act(f.it, r.id, "resolve", { assetReference: "ASSET-LAPTOP-42" });
  const row = await getEmployeeRequest(db, f.claimant, r.id),
    handover = JSON.parse(row.handover_json);
  expect(handover.assetReference).toBe("ASSET-LAPTOP-42");
  expect(handover.handedBy).toBe(f.it.userId);
  expect(handover.receivedBy).toBe(f.claimant.userId);
  expect(row.state).toBe("fulfilled");
  expect(
    (
      await db
        .prepare("SELECT count(*) n FROM finance_journals WHERE company_id=?")
        .bind(f.company)
        .first<{ n: number }>()
    )?.n,
  ).toBe(0);
  await act(f.claimant, r.id, "confirm");
});
it("requires purchasing authority and an approved unchanged request for an atomic order handoff", async () => {
  const r = await assigned("purchase"),
    row = await getEmployeeRequest(db, f.it, r.id);
  await expect(
    act({ ...f.it, beta: true }, r.id, "link-order", {
      purchaseOrderId: "fake",
    }),
  ).rejects.toMatchObject({ code: "BETA_CAPABILITY_UNAVAILABLE" });
  await expect(
    act(f.it, r.id, "link-order", { purchaseOrderId: "fake" }),
  ).rejects.toMatchObject({ code: "PERMISSION_REQUIRED" });
  const grantId = await f.grant(f.it, "purchasing_operator");
  const purchasing = {
    ...f.it,
    assignmentId: grantId,
    role: "purchasing_operator",
  };
  const command = key(),
    prepared = await prepareRequestOrderHandoff(
      db,
      purchasing,
      r.id,
      "test-order",
      command,
      row.version,
    );
  await act(f.it, r.id, "comment");
  await expect(
    executeCommand(
      db,
      prepared.actor,
      command,
      "test.handoff",
      {},
      r.id,
      row.version,
      {},
      prepared.steps,
    ),
  ).rejects.toMatchObject({ status: 409 });
  expect(
    (await getEmployeeRequest(db, f.it, r.id)).purchase_order_id,
  ).toBeNull();
});
it("uses only the active responsibility even when other grants exist", async () => {
  const grant = (
    role: string,
    scopeKind: WorkAssignment["scopeKind"] = "holding",
    scopeNodeId: string | null = null,
    companyId = f.company,
  ) =>
    ({
      id: key(),
      role,
      scopeKind,
      scopeNodeId,
      companyId,
      branchId:null,
      modules: [],
    }) satisfies WorkAssignment;
  const actor = {
    ...f.it,
    assignments: [
      grant("employee"),
      grant("purchasing_operator"),
      grant("finance_approver", "department", f.accountsDepartment),
      grant("holding_owner", "holding", null, "other-company"),
    ],
  };
  expect(responsibilityRoles(actor)).toEqual(["it_admin"]);
  expect(responsibilityRoles(actor)).not.toContain("finance_approver");
  expect(commercePermissions(actor).purchase).toBe(false);
  expect(
    responsibilityContext(actor, ["holding_owner", "finance_approver"]),
  ).toBeNull();
  expect(
    responsibilityContext(actor, ["purchasing_operator"]),
  ).toBeNull();
  const purchasing={...actor,role:"purchasing_operator",assignmentId:actor.assignments[1].id};
  expect(commercePermissions(purchasing).purchase).toBe(true);
});
it("keeps handbook drafts, published revisions and search strictly separate", async () => {
  await setHandbookEditor(
    db,
    f.owner,
    { userId: f.it.userId, active: true, expectedVersion: 0 },
    key(),
  );
  const input = handbookSchema.parse({
    title: "Office help",
    body: "Published instructions for requesting support.",
  });
  const r = await saveHandbookArticle(db, f.it, null, input, key());
  expect((await handbookList(db, f.claimant)).articles).toHaveLength(0);
  await expect(handbookArticle(db, f.claimant, r.id)).rejects.toMatchObject({
    status: 404,
  });
  await actOnHandbook(
    db,
    f.it,
    r.id,
    { action: "publish", expectedVersion: r.version },
    key(),
  );
  const published = await handbookArticle(db, f.it, r.id, true);
  await saveHandbookArticle(
    db,
    f.it,
    r.id,
    {
      ...input,
      body: "Unpublished secret draft",
      expectedVersion: published.article.version!,
    },
    key(),
  );
  expect((await handbookArticle(db, f.claimant, r.id)).revision.body).toBe(
    input.body,
  );
  expect((await handbookList(db, f.claimant, "secret")).articles).toHaveLength(
    0,
  );
  expect((await handbookArticle(db, f.claimant, r.id)).history).toHaveLength(1);
  expect((await handbookArticle(db, f.it, r.id, true)).history).toHaveLength(2);
  await expect(
    handbookArticle(db, f.claimant, r.id, true),
  ).rejects.toMatchObject({ status: 403 });
});
it("revokes handbook editing immediately, denies cross-company reads and rejects stale publication", async () => {
  await setHandbookEditor(
    db,
    f.owner,
    { userId: f.it.userId, active: true, expectedVersion: 0 },
    key(),
  );
  const input = handbookSchema.parse({
      title: "Company guide",
      body: "Original draft",
    }),
    r = await saveHandbookArticle(db, f.it, null, input, key());
  await saveHandbookArticle(
    db,
    f.it,
    r.id,
    { ...input, body: "Revised draft", expectedVersion: r.version },
    key(),
  );
  await expect(
    actOnHandbook(
      db,
      f.it,
      r.id,
      { action: "publish", expectedVersion: r.version },
      key(),
    ),
  ).rejects.toMatchObject({ status: 409 });
  await setHandbookEditor(
    db,
    f.owner,
    { userId: f.it.userId, active: false, expectedVersion: 1 },
    key(),
  );
  await expect(
    saveHandbookArticle(
      db,
      f.it,
      r.id,
      { ...input, expectedVersion: 2 },
      key(),
    ),
  ).rejects.toMatchObject({ status: 403 });
  const other = await expenseFixture();
  await expect(
    handbookArticle(db, other.owner, r.id, true),
  ).rejects.toMatchObject({ status: 404 });
});

it("retains immutable team configuration history and blocks unavailable local coverage", async () => {
  await team();
  const local = await team("it", "department", f.department, f.head);
  await saveServiceTeam(
    db,
    f.owner,
    local.id,
    { ...local.input, name: "Renamed local support", expectedVersion: 1 },
    key(),
  );
  const history = (
    await db
      .prepare(
        "SELECT version,configuration_json FROM service_team_revisions WHERE team_id=? ORDER BY version",
      )
      .bind(local.id)
      .all<{ version: number; configuration_json: string }>()
  ).results;
  expect(history.map((h) => h.version)).toEqual([1, 2]);
  expect(JSON.parse(history[0].configuration_json).name).toBe("it support");
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?")
    .bind(Date.now(), f.head.userId)
    .run();
  expect(
    await resolveServiceTeam(
      db,
      {
        company_id: f.company,
        branch_id: f.branch,
        department_id: f.department,
      },
      "it",
    ),
  ).toBeNull();
});
it("permits a purchase request with unknown cost and no promised delivery date", async () => {
  const r = await request("purchase", f.claimant, {
    neededDate: "",
    currency: "",
  });
  await act(f.claimant, r.id, "submit");
  expect((await getEmployeeRequest(db, f.head, r.id)).state).toBe(
    "awaiting_approval",
  );
});
it("opens historical published handbook content and never exposes a requested draft revision", async () => {
  const input = handbookSchema.parse({
    title: "Company guide",
    body: "First published edition",
  });
  const r = await saveHandbookArticle(db, f.owner, null, input, key());
  await actOnHandbook(
    db,
    f.owner,
    r.id,
    { action: "publish", expectedVersion: 1 },
    key(),
  );
  await saveHandbookArticle(
    db,
    f.owner,
    r.id,
    { ...input, body: "Second private draft", expectedVersion: 2 },
    key(),
  );
  expect(
    (await handbookArticle(db, f.claimant, r.id, false, 1)).revision.body,
  ).toBe(input.body);
  await expect(
    handbookArticle(db, f.claimant, r.id, false, 2),
  ).rejects.toMatchObject({ status: 404 });
  expect(
    (await handbookArticle(db, f.owner, r.id, true, 2)).revision.body,
  ).toBe("Second private draft");
  await actOnHandbook(
    db,
    f.owner,
    r.id,
    { action: "publish", expectedVersion: 3 },
    key(),
  );
  expect(
    (await handbookArticle(db, f.claimant, r.id, false, 1)).revision.body,
  ).toBe(input.body);
  expect((await handbookArticle(db, f.claimant, r.id)).revision.revision).toBe(
    2,
  );
});
it('maps an approved requisition into RFQ and atomically links the awarded PO without bypassing either owner',async()=>{
 const {createRfq,actRfq,rfqDetail}=await import('../src/server/procurement-rfqs');
 const {saveMaster,saveOrder,orderSchema,getOrder}=await import('../src/server/commerce');
 const {purchaseRequestProgress}=await import('../src/server/purchase-request-progress');
 const r=await assigned('purchase'),purchaser={...f.it,role:'purchasing_operator',assignmentId:await f.grant(f.it,'purchasing_operator')};
 const source=await getEmployeeRequest(db,purchaser,r.id),warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Requisition warehouse'},key())).id),product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Requested laptop',price:'100'},key())).id),party=String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:key(),name:'Approved supplier'},key())).id);
 const input={requestId:r.id,requestVersion:source.version,title:'Source requisition RFQ',warehouseId:warehouse,neededDate:'2026-09-20',demandEvidence:'Product mapped to the approved laptop request',lines:[{requestItemOrdinal:0,productId:product,quantity:'1'}]};
 await expect(createRfq(purchaser,{...input,lines:[{...input.lines[0],quantity:'2'}]},key())).rejects.toMatchObject({code:'RFQ_REQUEST_MAPPING_REQUIRED'});
 const id=String((await createRfq(purchaser,input,key())).id);await expect(createRfq(purchaser,input,key())).rejects.toMatchObject({status:422,code:'RFQ_REQUEST_MAPPING_REQUIRED'});
 const poInput=orderSchema.parse({kind:'purchase',requestId:r.id,requestVersion:source.version,partyId:party,warehouseId:warehouse,orderDate:'2026-09-20',dueDate:'2026-09-20',lines:[{productId:product,quantity:'1',price:'100',taxBps:0}]});
 await expect(saveOrder(purchaser,null,poInput,key())).rejects.toMatchObject({code:'REQUEST_RFQ_REQUIRED'});
 await expect(act(purchaser,r.id,'link-order',{purchaseOrderId:'unrelated-order'})).rejects.toMatchObject({code:'REQUEST_RFQ_REQUIRED'});
 let detail=await rfqDetail(purchaser,id);expect(detail.rfq.department_id).toBe(f.department);expect(detail.lines[0].request_item_ordinal).toBe(0);
 await actRfq(purchaser,id,{action:'offer',expectedVersion:1,supplierId:party,reference:'QUOTE-PR-001',evidence:'Supplier agrees to the requested laptop',arrivalDate:'2026-09-20',validUntil:'2099-12-31',lines:[{lineId:String(detail.lines[0].id),price:'100',taxBps:0}]},key());detail=await rfqDetail(purchaser,id);
 await act(f.it,r.id,'ask');expect((await rfqDetail(purchaser,id)).sourceBlocked).toBe(true);
 await expect(actRfq(purchaser,id,{action:'submit',expectedVersion:2,offerId:String(detail.current[0].id),reason:'Cannot award unresolved demand'},key())).rejects.toMatchObject({code:'RFQ_REQUEST_CHANGED'});await act(f.claimant,r.id,'reply');
 await actRfq(purchaser,id,{action:'submit',expectedVersion:2,offerId:String(detail.current[0].id),reason:'Matches approved request with full delivery'},key());await actRfq(f.owner,id,{action:'approve',expectedVersion:3,reason:'Independent comparison and award review'},key());
 await act(f.it,r.id,'comment');detail=await rfqDetail(purchaser,id);expect(detail.currentRequestVersion).toBeGreaterThan(source.version);
 const handoff={...poInput,requestVersion:detail.currentRequestVersion!,rfqId:id,rfqVersion:4},command=key(),po=await saveOrder(purchaser,null,handoff,command);expect(await saveOrder(purchaser,null,handoff,command)).toEqual(po);
 expect((await getEmployeeRequest(db,purchaser,r.id)).purchase_order_id).toBe(po.id);expect((await rfqDetail(purchaser,id)).rfq.order_id).toBe(po.id);expect((await getOrder(purchaser,String(po.id))).order.state).toBe('draft');
 const progress=await purchaseRequestProgress(db,purchaser,r.id);
 expect(progress.steps.map(step=>[step.id,step.state])).toEqual([
  ['approval','complete'],['sourcing','complete'],['order','current'],['receiving','waiting'],['invoice','waiting'],['payment','waiting'],
 ]);
 expect(progress.steps.find(step=>step.id==='sourcing')?.href).toBe(`/procurement-rfqs/${id}`);
 const requesterProgress=await purchaseRequestProgress(db,f.claimant,r.id);
 expect(requesterProgress.steps.find(step=>step.id==='sourcing')?.href).toBeNull();
});
it('splits one approved request across supplier awards and snapshots exact unit conversions for two purchase orders',async()=>{
 const {createRfq,actRfq,rfqDetail}=await import('../src/server/procurement-rfqs');
 const {requestQuotationSource}=await import('../src/server/employee-requests');
 const {saveMaster,saveOrder,orderSchema}=await import('../src/server/commerce');
 await team('purchasing');
 const r=await request('purchase',f.claimant,{currency:'THB',items:[{description:'Laptop',quantity:'24',unit:'each',estimatedCost:'240000'}]});
 await act(f.claimant,r.id,'submit');await act(f.head,r.id,'approve');await act(f.reviewer,r.id,'approve');await act(f.manager,r.id,'approve');await act(f.it,r.id,'assign',{assigneeUserId:f.it.userId});await act(f.it,r.id,'start');
 const purchaser={...f.it,role:'purchasing_operator',assignmentId:await f.grant(f.it,'purchasing_operator')};
 const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Split request warehouse'},key())).id);
 const caseProduct=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Laptop case of twelve',unit:'case',price:'120000'},key())).id);
 const eachProduct=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Laptop single unit',unit:'each',price:'10000'},key())).id);
 const suppliers=[String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:key(),name:'Case supplier'},key())).id),String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:key(),name:'Unit supplier'},key())).id)];
 let source=await requestQuotationSource(db,purchaser,r.id);
 const first=String((await createRfq(purchaser,{requestId:r.id,requestVersion:source.request.version,title:'First split award',warehouseId:warehouse,neededDate:'2026-09-20',demandEvidence:'Allocate half to case supplier',lines:[{requestItemOrdinal:0,requestQuantity:'12',requestUnit:'each',requestUnitsPerProductUnit:'12',productId:caseProduct,quantity:'1'}]},key())).id);
 source=await requestQuotationSource(db,purchaser,r.id);expect(source.items[0]).toMatchObject({requestedMilli:24000,allocatedMilli:12000,remainingMilli:12000});
 await expect(createRfq(purchaser,{requestId:r.id,requestVersion:source.request.version,title:'Excess allocation',warehouseId:warehouse,neededDate:'2026-09-20',demandEvidence:'Must not exceed approved quantity',lines:[{requestItemOrdinal:0,requestQuantity:'13',requestUnit:'each',requestUnitsPerProductUnit:'1',productId:eachProduct,quantity:'13'}]},key())).rejects.toMatchObject({code:'RFQ_REQUEST_MAPPING_REQUIRED'});
 const cancelled=String((await createRfq(purchaser,{requestId:r.id,requestVersion:source.request.version,title:'Temporary second split',warehouseId:warehouse,neededDate:'2026-09-20',demandEvidence:'Temporary allocation to verify release',lines:[{requestItemOrdinal:0,requestQuantity:'12',requestUnit:'each',requestUnitsPerProductUnit:'1',productId:eachProduct,quantity:'12'}]},key())).id);
 await actRfq(purchaser,cancelled,{action:'cancel',expectedVersion:1,reason:'Replace the temporary sourcing decision'},key());
 source=await requestQuotationSource(db,purchaser,r.id);expect(source.items[0].remainingMilli).toBe(12000);
 const raceInput=(title:string)=>({requestId:r.id,requestVersion:source.request.version,title,warehouseId:warehouse,neededDate:'2026-09-20',demandEvidence:'Concurrent allocation must have one winner',lines:[{requestItemOrdinal:0,requestQuantity:'12',requestUnit:'each',requestUnitsPerProductUnit:'1',productId:eachProduct,quantity:'12'}]});
 const raced=await Promise.allSettled([createRfq(purchaser,raceInput('Concurrent allocation A'),key()),createRfq(purchaser,raceInput('Concurrent allocation B'),key())]);
 expect(raced.filter(x=>x.status==='fulfilled')).toHaveLength(1);expect(raced.filter(x=>x.status==='rejected')).toHaveLength(1);
 expect((raced.find(x=>x.status==='rejected') as PromiseRejectedResult).reason).toMatchObject({status:409,code:'RFQ_REQUEST_OVERALLOCATED'});
 const raceWinner=String((raced.find(x=>x.status==='fulfilled') as PromiseFulfilledResult<{id:string}>).value.id);await actRfq(purchaser,raceWinner,{action:'cancel',expectedVersion:1,reason:'Concurrency proof complete; release allocation'},key());
 source=await requestQuotationSource(db,purchaser,r.id);expect(source.items[0].remainingMilli).toBe(12000);
 const second=String((await createRfq(purchaser,{requestId:r.id,requestVersion:source.request.version,title:'Second split award',warehouseId:warehouse,neededDate:'2026-09-20',demandEvidence:'Allocate remaining half to unit supplier',lines:[{requestItemOrdinal:0,requestQuantity:'12',requestUnit:'each',requestUnitsPerProductUnit:'1',productId:eachProduct,quantity:'12'}]},key())).id);
 source=await requestQuotationSource(db,purchaser,r.id);expect(source.fullyAllocated).toBe(true);expect(source.items[0].remainingMilli).toBe(0);
 const configs=[{id:first,supplier:suppliers[0],product:caseProduct,quantity:'1',price:'120000'},{id:second,supplier:suppliers[1],product:eachProduct,quantity:'12',price:'10000'}];
 const orderIds:string[]=[];
 for(const [index,config] of configs.entries()){
  let detail=await rfqDetail(purchaser,config.id);expect(detail.lines[0]).toMatchObject({request_quantity_milli:12000,request_unit:'each'});
  await actRfq(purchaser,config.id,{action:'offer',expectedVersion:1,supplierId:config.supplier,reference:`SPLIT-QUOTE-${index}`,evidence:'Supplier quote for allocated quantity',arrivalDate:'2026-09-20',validUntil:'2099-12-31',lines:[{lineId:String(detail.lines[0].id),price:config.price,taxBps:0}]},key());
  detail=await rfqDetail(purchaser,config.id);await actRfq(purchaser,config.id,{action:'submit',expectedVersion:2,offerId:String(detail.current[0].id),reason:'Best offer for this allocated quantity'},key());await actRfq(f.owner,config.id,{action:'approve',expectedVersion:3,reason:'Independent review of split supplier award'},key());
  detail=await rfqDetail(purchaser,config.id);const po=await saveOrder(purchaser,null,orderSchema.parse({kind:'purchase',requestId:r.id,requestVersion:detail.currentRequestVersion,rfqId:config.id,rfqVersion:4,partyId:config.supplier,warehouseId:warehouse,orderDate:'2026-09-19',dueDate:'2026-09-20',lines:[{productId:config.product,quantity:config.quantity,price:config.price,taxBps:0}]}),key());orderIds.push(String(po.id));
 }
 expect(new Set(orderIds).size).toBe(2);
 expect(Number(await db.prepare('SELECT count(*) n FROM procurement_request_orders WHERE request_id=?').bind(r.id).first('n'))).toBe(2);
 expect((await getEmployeeRequest(db,purchaser,r.id)).purchase_order_id).toBe(orderIds[0]);
 const progress=await (await import('../src/server/purchase-request-progress')).purchaseRequestProgress(db,purchaser,r.id);expect(progress.steps.find(step=>step.id==='sourcing')?.state).toBe('complete');expect(progress.steps.find(step=>step.id==='order')?.detail).toContain('2 linked purchase orders');
});

it('rolls back request draft, event, receipt, audit and outbox after a late failure then replays one retry',async()=>{
 const command=key(),id='request-'+command,input=employeeRequestSchema.parse({kind:'service',title:'Late atomic request',description:'Original issue facts',branchId:f.branch,departmentId:f.department}),suffix=command.replaceAll('-',''),fn='reject_request_'+suffix,trigger='reject_request_'+suffix;
 await db.prepare(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN RAISE EXCEPTION 'injected final request outbox failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} BEFORE INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fn}()`).run();
 try {await expect(saveEmployeeRequest(db,f.claimant,null,input,command)).rejects.toMatchObject({code:'23514'});
  expect(await db.prepare('SELECT 1 FROM employee_requests WHERE id=?').bind(id).first()).toBeNull();
  expect(await db.prepare('SELECT 1 FROM employee_request_events WHERE request_id=?').bind(id).first()).toBeNull();
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
 } finally {await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fn}()`).run();}
 const retry=await saveEmployeeRequest(db,f.claimant,null,input,command),replay=await saveEmployeeRequest(db,f.claimant,null,input,command);expect(replay).toEqual(retry);expect(retry.id).toBe(id);
 expect(await db.prepare('SELECT count(*) n FROM employee_request_events WHERE request_id=?').bind(id).first('n')).toBe(1);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n')).toBe(1);
});

it('rolls back handbook publication and canonical command journals then retries and replays once with live editor authority',async()=>{
 const draft=await saveHandbookArticle(db,f.owner,null,handbookSchema.parse({title:'Atomic handbook guide',body:'Published only after the complete command commits.'}),key()),command=key(),input={action:'publish' as const,expectedVersion:draft.version},suffix=command.replaceAll('-',''),fn='reject_handbook_'+suffix,trigger=fn;
 const before=await db.prepare('SELECT row_to_json(a) state FROM handbook_articles a WHERE id=?').bind(draft.id).first('state'),revisions=await db.prepare('SELECT jsonb_agg(to_jsonb(r) ORDER BY revision) state FROM handbook_revisions r WHERE article_id=?').bind(draft.id).first('state');
 await db.prepare(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN RAISE EXCEPTION 'injected final handbook publication failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fn}()`).run();
 try{await expect(actOnHandbook(db,f.owner,draft.id,input,command)).rejects.toMatchObject({code:'23514'});expect(await db.prepare('SELECT row_to_json(a) state FROM handbook_articles a WHERE id=?').bind(draft.id).first('state')).toEqual(before);expect(await db.prepare('SELECT jsonb_agg(to_jsonb(r) ORDER BY revision) state FROM handbook_revisions r WHERE article_id=?').bind(draft.id).first('state')).toEqual(revisions);for(const table of ['command_receipts','audit_events','outbox_events','command_assertions'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();}finally{await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fn}()`).run();}
 const retry=await actOnHandbook(db,f.owner,draft.id,input,command);expect(await actOnHandbook(db,f.owner,draft.id,input,command)).toEqual(retry);expect((await handbookArticle(db,f.claimant,draft.id)).revision).toMatchObject({revision:1,published_by:f.owner.userId});for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n')).toBe(1);
});
