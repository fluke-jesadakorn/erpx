import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { isValidElement, type ReactElement } from "react";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import MaintenancePage from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/maintenance/[id]/screen";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: db, files: env.H001_FILES }), runtimeEnv: async () => env }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ requireWorkContext: async () => actor, publicScope: () => ({ kind: "holding", holdingSlug: "test" }) }));
// This suite owns presenter route decoding and current Company/branch source
// guards. It binds the page's native detail boundary to the owner read over the
// same disposable database; native HTTP reads are gated by
// maintenance.rust-maintenance-read-postgres.integration and the browser journey.
vi.mock("@erpx/feature-assets/public/native-assets", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/feature-assets/public/native-assets")>();
  const { maintenanceDetail } = await import("@erpx/server-maintenance/public/asset-maintenance");
  return { ...actual, ownedMaintenanceDetail: (actor: WorkContext, _holdingSlug: string, id: string) => maintenanceDetail(actor, id) };
});
vi.mock("@erpx/server-work_projection/public/page-error", () => ({ pageError: (error: unknown) => { throw error; } }));
// Owner rows the native Assets register command persists; the TypeScript writer
// is retired and the native Rust suites own command behavior.
async function seedAsset(actor:{companyId:string;userId:string},input:{branchId:string;departmentId?:string|null;code:string;name:string;category:string;currency?:string;costMinor?:number;acquiredDate:string;warrantyEnd?:string|null}){
 const id=`asset-${key()}`,at=Date.now();
 await db.batch([
  db.prepare("INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.branchId,input.departmentId??null,input.code,input.name,input.category,input.currency??'THB',input.costMinor??0,input.acquiredDate,input.warrantyEnd??null,actor.userId,at,at),
  db.prepare("INSERT INTO operational_asset_events VALUES(?,?,1,'register',?,'Asset registered','good','registered',NULL,NULL,?)").bind(`asset-event-${key()}`,id,actor.userId,at),
 ]);
 return {id,version:1};
}
let actor: WorkContext, f: Awaited<ReturnType<typeof expenseFixture>>, workId: string;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture(); actor = { ...f.owner, modules: ["maintenance", "assets"] };
  const asset = await seedAsset(actor, { branchId: f.branch, code: key(), name: "Encoded native work source", category: "Inspection equipment", currency: "THB", costMinor: 0, acquiredDate: "2026-01-01" });
  // Owner rows for the retained reads. Preventive-plan and work commands are
  // native; this suite only needs the generated work the owner unit would store.
  const planId = `maintenance-${key()}`, dueAt = Date.now() - 86_400_000, now = Date.now();
  workId = `${planId}:work`;
  await db.transaction(async tx => {
    await tx.prepare("INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,status,version,created_by,edited_by,approved_by,approved_at,created_at,updated_at) VALUES(?,?,?,NULL,?,?,?,?,1,'Asia/Bangkok','2026-01-01T09:00','2026-01-01T09:00',?,0,'active',2,?,?,?,?,?,?)").bind(planId, f.company, f.branch, String(asset.id), "Daily encoded inspection", "normal", actor.userId, dueAt, actor.userId, actor.userId, actor.userId, now, now, now).run();
    await tx.prepare("INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at) SELECT ?,id,version,'activate',?,?,to_jsonb(p),? FROM maintenance_preventive_plans p WHERE id=?").bind(`preventive-event-${key()}`, actor.userId, "Independent review of the owner plan.", now, planId).run();
    await tx.prepare("INSERT INTO maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,scheduled_at,version,created_by,created_at,updated_at) VALUES(?,?,?,NULL,?,?,?,'scheduled',?,?,1,?,?,?)").bind(workId, f.company, f.branch, String(asset.id), "Daily encoded inspection", "normal", actor.userId, dueAt, actor.userId, now, now).run();
    await tx.prepare("INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at) VALUES(?,?,1,'create',?,?,'scheduled',?,?,NULL,NULL,?)").bind(`maintenance-event-${key()}`, workId, actor.userId, "Generate the actual due work.", actor.userId, dueAt, now).run();
    await tx.prepare("INSERT INTO maintenance_preventive_occurrences(id,plan_id,plan_version,work_order_id,due_local,due_at,source_asset_version,definition_json,generated_by,generated_at) VALUES(?,?,2,?,'2026-01-01T09:00',?,1,?,?,?)").bind(`preventive-occurrence-${key()}`, planId, workId, dueAt, JSON.stringify({ title: "Daily encoded inspection", priority: "normal", ownerId: actor.userId }), actor.userId, now).run();
  });
});

it("renders the real generated work when the compiled route parameter escapes its original command suffix", async () => {
  expect(workId).toMatch(/:work$/);
  const page = await MaintenancePage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(workId) }) });
  const controls = page.props.children.find((child: unknown) => isValidElement(child) && "detail" in (child.props as Record<string, unknown>)) as ReactElement<{ detail: { work: { id: string; status: string; title: string }; history: Array<{ action: string }> } }>;
  expect(controls.props.detail.work).toMatchObject({ id: workId, status: "scheduled", title: "Daily encoded inspection" });
  expect(controls.props.detail.history.map(event => event.action)).toEqual(["create"]);
  expect((await db.prepare("SELECT work_order_id FROM maintenance_preventive_occurrences WHERE work_order_id=?").bind(workId).all()).results).toEqual([{ work_order_id: workId }]);
});

it("preserves current Company and branch source guards for encoded generated work links", async () => {
  actor = { ...actor, companyId: "foreign-company" };
  await expect(MaintenancePage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(workId) }) })).rejects.toMatchObject({ status: 404, code: "MAINTENANCE_WORK_NOT_FOUND" });
  actor = { ...f.owner, modules: ["maintenance", "assets"], branchId: "foreign-branch" };
  await expect(MaintenancePage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(workId) }) })).rejects.toMatchObject({ status: 404, code: "MAINTENANCE_WORK_NOT_FOUND" });
});
