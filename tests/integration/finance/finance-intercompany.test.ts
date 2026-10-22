import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { createGroupEntity, createIntercompany, decideIntercompany, groupEntitySchema, intercompanyTransactionSchema, intercompanyWorkspace } from "@erpx/server-finance/public/finance-intercompany";

vi.mock("server-only", () => ({}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;
let target: string, setupCompany: string;

beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  target = `target-${key()}`;
  setupCompany = `setup-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES(?,'company',?,? ,?,'active')").bind(setupCompany, setupCompany, "Setup company", setupCompany),
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${f.company}`, "h001", f.company, "Source company", "THB"),
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${target}`, "h001", target, "Target company", "THB"),
  ]);
});

it("registers an active organization company as a Group entity with replay and duplicate protection", async () => {
  const input = groupEntitySchema.parse({ companyId: setupCompany, reportingName: "Setup company", functionalCurrency: "usd" });
  const commandId = key();
  const created = await createGroupEntity(f.owner, input, commandId);
  expect(created).toMatchObject({ companyId: setupCompany, status: "active", version: 1 });
  expect(await createGroupEntity(f.owner, input, commandId)).toEqual(created);
  const workspace = await intercompanyWorkspace(f.owner);
  expect(workspace.entities.find(entity => entity.company_id === setupCompany)).toMatchObject({ reporting_name: "Setup company", functional_currency: "USD", version: 1 });
  expect(workspace.availableCompanies.some(company => company.company_id === setupCompany)).toBe(false);
  await expect(createGroupEntity(f.owner, input, key())).rejects.toMatchObject({ code: "GROUP_ENTITY_EXISTS" });
});

it("runs an idempotent intercompany create, independent match, settlement and elimination", async () => {
  const input = intercompanyTransactionSchema.parse({ targetCompanyId: target, reference: `IC-${key()}`, currency: "THB", amount: "125.50" });
  const created = await createIntercompany(f.owner, input, key());
  expect(created).toMatchObject({ status: "unmatched", version: 1, amountMinor: 12550 });
  const matched = await decideIntercompany(f.manager, String(created.id), "match", { expectedVersion: 1, reason: "Both posted company-side journals agree." }, key());
  expect(matched).toMatchObject({ status: "matched", version: 2 });
  const settled = await decideIntercompany(f.payer, String(created.id), "settle", { expectedVersion: 2, reason: "Settlement evidence reviewed against the intercompany reference." }, key());
  expect(settled).toMatchObject({ status: "settled", version: 3 });
  const eliminated = await decideIntercompany(f.owner, String(created.id), "eliminate", { expectedVersion: 3, reason: "Approved group elimination for the closed period." }, key());
  expect(eliminated).toMatchObject({ status: "eliminated", version: 4 });
  const workspace = await intercompanyWorkspace(f.owner);
  expect(workspace.statusCounts).toEqual({ eliminated: 1 });
  expect(await db.prepare("SELECT action,version FROM finance_intercompany_events WHERE transaction_id=? ORDER BY version").bind(created.id).all()).toMatchObject({ results: [{ action: "created", version: 1 }, { action: "matched", version: 2 }, { action: "settled", version: 3 }, { action: "eliminated", version: 4 }] });
  await expect(db.prepare("UPDATE finance_intercompany_events SET reason='tampered' WHERE transaction_id=? AND version=1").bind(created.id).run()).rejects.toThrow("FINANCE_INTERCOMPANY_EVENT_IMMUTABLE");
});

it("rejects self-review, stale versions and missing group entities", async () => {
  const input = intercompanyTransactionSchema.parse({ targetCompanyId: target, reference: `IC-${key()}`, currency: "THB", amount: "10" });
  const created = await createIntercompany(f.owner, input, key());
  await expect(decideIntercompany(f.owner, String(created.id), "match", { expectedVersion: 1, reason: "I created this item." }, key())).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  const matched = await decideIntercompany(f.manager, String(created.id), "match", { expectedVersion: 1, reason: "Independent review." }, key());
  await expect(decideIntercompany(f.payer, String(created.id), "settle", { expectedVersion: 1, reason: "Stale decision." }, key())).rejects.toMatchObject({ code: "INTERCOMPANY_CHANGED" });
  expect(matched.status).toBe("matched");
  const missing = `missing-${key()}`;
  await expect(createIntercompany(f.owner, { ...input, targetCompanyId: missing, reference: `IC-${key()}` }, key())).rejects.toMatchObject({ code: "GROUP_ENTITY_REQUIRED" });
});

it('rolls back an independent handoff and its immutable event after final outbox failure, then retries and replays once',async()=>{
 const item=await createIntercompany(f.owner,intercompanyTransactionSchema.parse({targetCompanyId:target,reference:`IC-${key()}`,currency:'THB',amount:'125.50'}),key()),id=String(item.id),command=key(),input={expectedVersion:1,reason:'Independent scoped document handoff'},fault=`intercompany_fault_${crypto.randomUUID().replaceAll('-','')}`,before=await db.prepare('SELECT * FROM finance_intercompany_transactions WHERE id=?').bind(id).first();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'INTERCOMPANY_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.intercompany.match') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideIntercompany(f.manager,id,'match',input,command)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_intercompany_transactions WHERE id=?').bind(id).first()).toEqual(before);
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_intercompany_events WHERE transaction_id=?').bind(id).first('n'))).toBe(1);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const matched=await decideIntercompany(f.manager,id,'match',input,command);expect(await decideIntercompany(f.manager,id,'match',input,command)).toEqual(matched);expect(matched).toMatchObject({status:'matched',version:2});
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_intercompany_events WHERE transaction_id=?').bind(id).first('n'))).toBe(2);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});
