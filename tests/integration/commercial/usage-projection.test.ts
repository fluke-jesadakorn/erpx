import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import {randomUUID} from 'node:crypto';
import { applyMigrations, env } from "../../support/runtime-env";
import { readPlatformUsageProjection, syncHoldingUsageProjection } from "../../../src/server/modules/commercial/public/usage-projection";
import type { UsageHolding, UsageItem } from "../../../src/server/modules/commercial/public/usage";
import { billingMonth, monthBounds } from "../../../src/lib/billing/billing";
import {readHoldingUsage} from "../../../src/server/modules/commercial/public/usage";
import {expenseFixture,key} from "../../support/helpers/expense-fixture";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

const binding = { id: "h001", slug: "acme", displayName: "Acme" };
const month = "2026-09";
const empty = () => ({ ai: { requests: 0, inputTokens: 0, outputTokens: 0 },
  storage: { bytes: 0, fileCount: 0, averageDailyBytes: 0 } });
const holding = (): UsageHolding => ({
  holdingId: binding.id, holdingSlug: binding.slug, holdingName: binding.displayName,
  companies: [], totals: empty(), sandboxStoragePreview: { status: "unpriced", amountSatang: null },
  meterCoverage:{ai:"partial",storage:"partial",seat:"unavailable",unreconciledOriginals:0},
  seatUsage:{monthly:{status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null},activeNow:{status:"unavailable",count:null,asOf:null}},
});
const ai = (overrides: Partial<UsageItem> = {}): UsageItem => ({
  id: "provider-row-1", holdingId: binding.id, companyId: "company-1", companyName: "Company One",
  branchId: "branch-1", branchName: "Branch One", type: "ai", quantity: 30,
  unit: "tokens", period: month, source: "secret-feature · secret-model",
  pricingStatus: "byok", estimatedSatang: null, fundingSource: "byok",
  inputTokens: 20, outputTokens: 10, occurredAt: Date.UTC(2026, 8, 12), ...overrides,
});
const storage = (): UsageItem => ({
  id: "attachment-source-1", holdingId: binding.id, companyId: "company-1", companyName: "Company One",
  branchId: null, branchName: null, type: "storage", quantity: 1024,
  unit: "bytes", period: month, source: "Original file", pricingStatus: "unpriced",
  estimatedSatang: null, averageDailyBytes: 512, activeAtPeriodEnd: true,
});

beforeAll(async () => {
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);
  await env.CONTROL_DB.prepare(`INSERT INTO holding_bindings(
    holding_id,slug,display_name,write_binding,read_binding,files_binding,status)
    VALUES('h001','acme','Acme','H001_DB','H001_READ_DB','H001_FILES','active')
    ON CONFLICT(holding_id) DO NOTHING`).run();
});

it('keeps Holding source facts local while publishing only the billing meter view',async()=>{
 const fixture=await expenseFixture(),sourceId=key(),{start}=monthBounds(month);
 await env.H001_DB.prepare("INSERT INTO ai_provider_usage(id,company_id,branch_id,model,feature,input_tokens,output_tokens,status,created_at,funding_source) VALUES(?,?,?,'private-provider-model','private-work-feature',20,10,'succeeded',?,'byok')").bind(sourceId,fixture.company,fixture.branch,start+1).run();
 const snapshot=await readHoldingUsage(binding,month,fixture.branch),item=snapshot.items.find(item=>item.id===sourceId);
 expect(item).toMatchObject({companyId:fixture.company,branchId:fixture.branch,quantity:30,inputTokens:20,outputTokens:10,pricingStatus:'byok'});expect(item?.companyName).not.toBe('Unknown company');
 expect(snapshot.holding.seatUsage.monthly.status).toBe('unavailable');expect(snapshot.holding.seatUsage.activeNow.status).toBe('unavailable');
 await syncHoldingUsageProjection(env.CONTROL_DB,binding,month,snapshot,2000);
 const projection=await readPlatformUsageProjection(env.CONTROL_DB,[binding],month);expect(projection.items).toHaveLength(1);expect(projection.items[0]).toMatchObject({quantity:30,inputTokens:20,outputTokens:10});
 const rows=(await env.CONTROL_DB.prepare('SELECT source_key,model FROM billing_usage_meters WHERE holding_id=? AND month=?').bind(binding.id,month).all()).results;expect(rows).toHaveLength(1);expect(rows[0]).toMatchObject({model:null});expect(JSON.stringify(rows)).not.toContain(sourceId);expect(JSON.stringify(projection)).not.toContain('private-provider-model');expect(JSON.stringify(projection)).not.toContain('private-work-feature');
});

it('rolls back meters and header on a late PostgreSQL failure and allows the identical refresh to recover',async()=>{
 await syncHoldingUsageProjection(env.CONTROL_DB,binding,month,{holding:holding(),items:[ai()]},1000);
 const original=await env.CONTROL_DB.prepare('SELECT snapshot_id,refreshed_at,source_count FROM billing_usage_snapshots WHERE holding_id=? AND month=?').bind(binding.id,month).first(),fault=`usage_projection_fault_${randomUUID().replaceAll('-','')}`;
 await env.CONTROL_DB.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'USAGE_PROJECTION_LATE_FAILURE' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER UPDATE ON billing_usage_snapshots FOR EACH ROW WHEN(NEW.holding_id='h001' AND NEW.refreshed_at=2000) EXECUTE FUNCTION ${fault}();`);
 const corrected={holding:holding(),items:[ai({quantity:60,inputTokens:40,outputTokens:20}),storage()]};
 try{await expect(syncHoldingUsageProjection(env.CONTROL_DB,binding,month,corrected,2000)).rejects.toThrow();}finally{await env.CONTROL_DB.exec(`DROP TRIGGER ${fault} ON billing_usage_snapshots;DROP FUNCTION ${fault}();`);}
 expect(await env.CONTROL_DB.prepare('SELECT snapshot_id,refreshed_at,source_count FROM billing_usage_snapshots WHERE holding_id=? AND month=?').bind(binding.id,month).first()).toEqual(original);
 expect((await readPlatformUsageProjection(env.CONTROL_DB,[binding],month)).items).toEqual([expect.objectContaining({quantity:30,inputTokens:20,outputTokens:10})]);
 await syncHoldingUsageProjection(env.CONTROL_DB,binding,month,corrected,2000);await syncHoldingUsageProjection(env.CONTROL_DB,binding,month,corrected,2000);
 expect(await env.CONTROL_DB.prepare('SELECT refreshed_at,source_count FROM billing_usage_snapshots WHERE holding_id=? AND month=?').bind(binding.id,month).first()).toEqual({refreshed_at:2000,source_count:2});
 expect(await env.CONTROL_DB.prepare('SELECT COUNT(*)::int count FROM billing_usage_meters WHERE holding_id=? AND month=?').bind(binding.id,month).first()).toEqual({count:2});
});

it('serializes concurrent refreshes and keeps the greatest observation rather than arrival order',async()=>{
 await Promise.all([syncHoldingUsageProjection(env.CONTROL_DB,binding,month,{holding:holding(),items:[ai({quantity:60,inputTokens:40,outputTokens:20})]},2000),syncHoldingUsageProjection(env.CONTROL_DB,binding,month,{holding:holding(),items:[ai()]},1000)]);
 expect(await env.CONTROL_DB.prepare('SELECT refreshed_at FROM billing_usage_snapshots WHERE holding_id=? AND month=?').bind(binding.id,month).first()).toEqual({refreshed_at:2000});expect((await readPlatformUsageProjection(env.CONTROL_DB,[binding],month)).items).toEqual([expect.objectContaining({quantity:60,inputTokens:40,outputTokens:20})]);
});

beforeEach(async () => {
  await env.CONTROL_DB.prepare("DELETE FROM billing_usage_snapshots WHERE holding_id=?").bind(binding.id).run();
});

it("projects only billing meter metadata and deduplicates retried source IDs", async () => {
  const item = ai();
  await syncHoldingUsageProjection(env.CONTROL_DB, binding, month,
    { holding: holding(), items: [item, item, storage()] }, 1000);
  await syncHoldingUsageProjection(env.CONTROL_DB, binding, month,
    { holding: holding(), items: [item, storage()] }, 1001);
  const rows = await env.CONTROL_DB.prepare("SELECT * FROM billing_usage_meters WHERE holding_id=? AND month=?")
    .bind(binding.id, month).all<Record<string, unknown>>();
  expect(rows.results).toHaveLength(2);
  const serialized = JSON.stringify(rows.results);
  expect(serialized).not.toContain("provider-row-1");
  expect(serialized).not.toContain("attachment-source-1");
  expect(serialized).not.toContain("secret-feature");
  expect(serialized).not.toContain("secret-model");
  const view = await readPlatformUsageProjection(env.CONTROL_DB, [binding], month);
  expect(view.unavailable).toEqual([]);
  expect(view.holdings[0].totals).toMatchObject({
    ai: { requests: 1, inputTokens: 20, outputTokens: 10 },
    storage: { bytes: 1024, fileCount: 1, averageDailyBytes: 512 },
  });
  expect(view.holdings[0].companies[0].unassigned.storage.bytes).toBe(1024);
  expect(view.holdings[0].companies[0].branches[0].totals.ai.requests).toBe(1);
  expect(view.items.find(row => row.type === "ai")?.pricingStatus).toBe("byok");
});

it("replaces corrected meters and preserves the newer source snapshot", async () => {
  await syncHoldingUsageProjection(env.CONTROL_DB, binding, month,
    { holding: holding(), items: [ai(), storage()] }, 2000);
  await syncHoldingUsageProjection(env.CONTROL_DB, binding, month,
    { holding: holding(), items: [ai({ quantity: 50, outputTokens: 30 })] }, 3000);
  await syncHoldingUsageProjection(env.CONTROL_DB, binding, month,
    { holding: holding(), items: [ai(), storage()] }, 2500);
  const view = await readPlatformUsageProjection(env.CONTROL_DB, [binding], month);
  expect(view.holdings[0].totals.ai.outputTokens).toBe(30);
  expect(view.holdings[0].totals.storage.fileCount).toBe(0);
  expect(view.items).toHaveLength(1);
});

it("distinguishes a measured empty Holding from a missing projection", async () => {
  let view = await readPlatformUsageProjection(env.CONTROL_DB, [binding], month);
  expect(view.holdings).toEqual([]);
  expect(view.unavailable).toEqual([{ holdingId: binding.id, holdingName: binding.displayName, reason: "Meter projection pending" }]);
  await syncHoldingUsageProjection(env.CONTROL_DB, binding, month,
    { holding: holding(), items: [] }, 4000);
  view = await readPlatformUsageProjection(env.CONTROL_DB, [binding], month);
  expect(view.unavailable).toEqual([]);
  expect(view.holdings[0].totals.ai.requests).toBe(0);
});

it("does not turn incomplete original-file coverage into a zero-price estimate", async () => {
  const incomplete = { ...holding(), meterCoverage: {
    ai: "partial" as const, storage: "partial" as const, seat:"unavailable" as const, unreconciledOriginals: 7,
  } };
  await syncHoldingUsageProjection(env.CONTROL_DB, binding, month,
    { holding: incomplete, items: [] }, 5000);
  const view = await readPlatformUsageProjection(env.CONTROL_DB, [binding], month);
  expect((view.holdings[0] as UsageHolding & { meterCoverage: typeof incomplete.meterCoverage }).meterCoverage)
    .toEqual(incomplete.meterCoverage);
  expect(view.holdings[0].sandboxStoragePreview).toEqual({ status: "unpriced", amountSatang: null });
});

it("projects reconciled cost and exact byte-time without exposing source IDs", async () => {
  const item = ai({ id: "provider-secret-request", fundingSource: "erpx",
    pricingStatus: "unpriced", requestStatus: "succeeded", gatewayStatus: "succeeded",
    provider: "openai", model: "private-model", unitsJson: '{"cached_input_tokens":10}',
    costStatus: "reconciled", verifiedCostSatang: 1234,
    costEvidenceKey: "a".repeat(64), costReconciledAt: 1000 });
  const file = { ...storage(), byteMilliseconds: "90071992547409930000",
    activeFrom: Date.UTC(2026,8,1), activeUntil: Date.UTC(2026,9,1) };
  await syncHoldingUsageProjection(env.CONTROL_DB,binding,month,
    { holding: holding(), items: [item,file] },6000);
  const rows = await env.CONTROL_DB.prepare(`SELECT meter_kind,request_status,cost_status,
    verified_cost_satang,byte_milliseconds::text byte_milliseconds,model
    FROM billing_usage_meters WHERE holding_id=? AND month=? ORDER BY meter_kind`)
    .bind(binding.id,month).all<Record<string,unknown>>();
  expect(rows.results[0]).toMatchObject({ meter_kind: "ai", request_status: "succeeded",
    cost_status: "reconciled", verified_cost_satang: 1234, model: null });
  expect(rows.results[1]).toMatchObject({ meter_kind: "storage",
    byte_milliseconds: "90071992547409930000" });
  expect(JSON.stringify(rows.results)).not.toContain("provider-secret-request");
  expect(JSON.stringify(rows.results)).not.toContain("private-model");
});

it("keeps multiple provider attempts and their cost states separate in the Platform view", async () => {
  const first=ai({id:"expense-image-page-1",fundingSource:"erpx",pricingStatus:"unpriced",
    requestStatus:"succeeded",gatewayStatus:"succeeded",provider:"api.provider.com",
    unitsJson:'{"input_tokens":20}',costStatus:"reconciled",verifiedCostSatang:80,
    costEvidenceKey:"a".repeat(64),costReconciledAt:1000});
  const second=ai({id:"expense-image-page-2",fundingSource:"erpx",pricingStatus:"unpriced",
    requestStatus:"succeeded",gatewayStatus:"succeeded",provider:"api.provider.com",
    unitsJson:'{"input_tokens":20}',costStatus:"unavailable",verifiedCostSatang:null});
  await syncHoldingUsageProjection(env.CONTROL_DB,binding,month,{holding:holding(),items:[first,second]},7000);
  const view=await readPlatformUsageProjection(env.CONTROL_DB,[binding],month);
  const attempts=view.items.filter(item=>item.type==="ai");
  expect(attempts).toHaveLength(2);
  expect(attempts.map(item=>item.costStatus).sort()).toEqual(["reconciled","unavailable"]);
  expect(attempts.map(item=>item.verifiedCostSatang)).toContain(80);
  expect(attempts.map(item=>item.verifiedCostSatang)).toContain(null);
  expect(view.holdings[0].totals.ai.requests).toBe(2);
  expect(JSON.stringify(view)).not.toContain("expense-image-page-1");
});

it("projects only a hashed seat identity and clips to the principal's active history",async()=>{
  const next=billingMonth(monthBounds(billingMonth(Date.now())).end);
  const bounds=monthBounds(next),userId=`seat-human-${crypto.randomUUID()}`,mid=bounds.start+10*86400000;
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,0,?,?)')
    .bind(userId,"Private human",`${userId}@example.invalid`,Date.now(),Date.now()).run();
  await env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)")
    .bind(`principal-${crypto.randomUUID()}`,userId,"Private human",Date.now()).run();
  await env.CONTROL_DB.prepare("INSERT INTO billing_principal_status_events(user_id,status,effective_at) VALUES(?,'suspended',?)")
    .bind(userId,mid).run();
  const scoped={...holding(),meterCoverage:{ai:"partial" as const,storage:"partial" as const,
    seat:"measured" as const,unreconciledOriginals:0}};
  await syncHoldingUsageProjection(env.CONTROL_DB,binding,next,{holding:scoped,items:[],seats:{
    coverage:"measured",users:[{userId,intervals:[[bounds.start,bounds.end]]}],
  }},Date.now());
  const rows=await env.CONTROL_DB.prepare("SELECT user_key,intervals_json::text intervals_json FROM billing_seat_usage WHERE holding_id=? AND month=?")
    .bind(binding.id,next).all<{user_key:string;intervals_json:string}>();
  expect(rows.results).toHaveLength(1);
  expect(rows.results[0].user_key).toMatch(/^[a-f0-9]{64}$/);
  expect(JSON.parse(rows.results[0].intervals_json)).toEqual([[bounds.start,mid]]);
  expect(JSON.stringify(rows.results)).not.toContain(userId);
  expect(JSON.stringify(rows.results)).not.toContain("Private human");
  const summary=await env.CONTROL_DB.prepare("SELECT seat_coverage,seat_source_count FROM billing_usage_snapshots WHERE holding_id=? AND month=?")
    .bind(binding.id,next).first<{seat_coverage:string;seat_source_count:number}>();
  expect(summary).toEqual({seat_coverage:"measured",seat_source_count:1});
});

it("publishes a current active-seat count without claiming monthly seat coverage",async()=>{
  const current=billingMonth(Date.now()),asOf=Date.now();
  const scoped={...holding(),seatUsage:{
    monthly:{status:"unavailable" as const,distinctUsers:null,activeSeatMilliseconds:null},
    activeNow:{status:"measured" as const,count:3,asOf},
  }};
  await syncHoldingUsageProjection(env.CONTROL_DB,binding,current,
    {holding:scoped,items:[],seats:{coverage:"unavailable",users:[]}},asOf);
  const view=await readPlatformUsageProjection(env.CONTROL_DB,[binding],current);
  expect(view.holdings[0].seatUsage).toEqual({
    monthly:{status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null},
    activeNow:{status:"measured",count:3,asOf},
  });
  expect(view.holdings[0].meterCoverage.seat).toBe("unavailable");
});
