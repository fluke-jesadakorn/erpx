import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {env,applyMigrations} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {assistantPage,assistantAskSchema} from "@erpx/feature-work_projection/lib/ai/workspace-assistant";
import {askAssistant,assistantSources,assistantView} from "@erpx/server-agent/public/workspace-assistant";
import {assertProductionFeatureRoute} from "@erpx/server-pms/public/hospitality-rollout";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const aiEnv={AI_BASE_URL:"https://opencode.ai/zen/go/v1",AI_MODEL:"muse-spark-1.3-contributor",AI_API_KEY:"synthetic-key"};
let f:Awaited<ReturnType<typeof expenseFixture>>;
const generate=vi.fn(async()=>({result:{answer:"Review this page.",sourceIds:["page-guide"],followUps:[]},inputTokens:10,outputTokens:5}));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();generate.mockClear();});
it("isolates record and filter contexts while ignoring navigation-only pagination",()=>{
  expect(assistantPage("/requests/one?cursor=abc").path).toBe("/requests/one");
  expect(assistantPage("/commerce?view=purchase&q=desk").path).not.toBe(assistantPage("/commerce?view=sales&q=desk").path);
  expect(()=>assistantPage("//evil.example/home")).toThrow();
  expect(()=>assistantPage("/requests/%2e%2e/settings")).toThrow();
});
it("persists page history without carrying it to another page or actor",async()=>{
  const input=assistantAskSchema.parse({path:"/home",question:"Explain this page"});
  await askAssistant(db,f.claimant,aiEnv,input,key(),generate);
  expect((await assistantView(db,f.claimant,aiEnv,"/home","page")).turns).toHaveLength(1);
  expect((await assistantView(db,f.claimant,aiEnv,"/work","page")).turns).toHaveLength(0);
  expect((await assistantView(db,f.owner,aiEnv,"/home","page")).turns).toHaveLength(0);
});
it("rejects cross-page requests for a role without the explicit permission",async()=>{
  await expect(askAssistant(db,f.claimant,aiEnv,assistantAskSchema.parse({path:"/home",mode:"workspace",question:"Read all pages"}),key(),generate)).rejects.toMatchObject({code:"AI_CONTEXT_FORBIDDEN"});
  await expect(askAssistant(db,f.claimant,aiEnv,assistantAskSchema.parse({path:"/home",question:"Read work too",contextPaths:["/work"]}),key(),generate)).rejects.toMatchObject({code:"AI_CONTEXT_FORBIDDEN"});
  expect(generate).not.toHaveBeenCalled();
});
it("replays a completed message without paying for a second inference",async()=>{
  const input=assistantAskSchema.parse({path:"/home",question:"What next?"}),id=key();
  await askAssistant(db,f.claimant,aiEnv,input,id,generate);
  await askAssistant(db,f.claimant,aiEnv,input,id,generate);
  expect(generate).toHaveBeenCalledTimes(1);
  await expect(askAssistant(db,f.claimant,aiEnv,{...input,question:"Another question"},id,generate)).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
});
it("does not reveal answers after the authorizing assignment is revoked",async()=>{
  const input=assistantAskSchema.parse({path:"/home",question:"What next?"});
  await expect(askAssistant(db,f.claimant,aiEnv,input,key(),async()=>{
    await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();
    return generate();
  })).rejects.toMatchObject({code:"WORK_CONTEXT_CHANGED"});
  expect((await db.prepare("SELECT state FROM ai_workspace_turns WHERE company_id=?").bind(f.company).first<{state:string}>())?.state).toBe("failed");
});
it("uses owner-scoped restaurant evidence and omits payment references from prompts",async()=>{
  const sources=await assistantSources(db,f.owner,"/pos?tab=sale");
  expect(sources.map(s=>s.id.split(':')[0])).toEqual(["guide","record"]);
  expect(sources[1].data).toMatchObject({title:"Restaurant point of sale"});
  expect(JSON.stringify(sources[1].data)).not.toMatch(/credential|apiKey|guest_name|payment.reference/i);
  await expect(assistantSources(db,f.claimant,"/pos?tab=sale")).rejects.toMatchObject({code:"AI_CONTEXT_FORBIDDEN"});
});
it("uses scoped inventory and Finance evidence without claiming unseen rows or amounts",async()=>{
  const stock=await assistantSources(db,f.owner,"/commerce?view=stock");
  expect(stock[1].data).toMatchObject({title:"Inventory position · first page",firstPageOnly:true,stock:expect.any(Array)});
  expect(stock[1].data).toHaveProperty("moreStockRows");
  expect(JSON.stringify(stock[1].data)).not.toMatch(/credential|apiKey|bank_account|guest_name/i);
  const finance=await assistantSources(db,f.owner,"/finance");
  expect(finance[1].data).toMatchObject({title:"Finance workbench · company counts",amountsUnavailable:true});
  const close=await assistantSources(db,f.owner,"/finance?view=close");
  expect(close[1].data).toMatchObject({title:"Finance close · recent periods",recentPeriodsOnly:true,periods:expect.any(Array)});
  await expect(assistantSources(db,f.claimant,"/finance")).rejects.toMatchObject({code:"AI_CONTEXT_FORBIDDEN"});
  await expect(assistantSources(db,{...f.owner,branchId:"branch-outside-company-scope"},"/finance")).rejects.toMatchObject({code:"AI_CONTEXT_FORBIDDEN"});
  await expect(assistantSources(db,f.claimant,"/commerce?view=stock")).rejects.toMatchObject({code:"AI_CONTEXT_FORBIDDEN"});
});
it("requires explicit company AI opt-in in production while retaining the manual guide",async()=>{
  vi.stubEnv("NODE_ENV","production");
  try {
    const view=await assistantView(db,f.owner,aiEnv,"/home","page");
    expect(view.unavailable).toContain("not enabled for this company");
    expect(view.guide.title).toBe("Today");
    await expect(askAssistant(db,f.owner,aiEnv,assistantAskSchema.parse({path:"/home",question:"What next?"}),key(),generate)).rejects.toMatchObject({code:"AI_PILOT_OPT_IN_REQUIRED"});
    expect(generate).not.toHaveBeenCalled();
  } finally {vi.unstubAllEnvs();}
});
it("keeps deferred module evidence out of production assistant prompts",async()=>{
  vi.stubEnv("NODE_ENV","production");
  try {
    await expect(assistantSources(db,f.owner,"/pms")).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await expect(assistantSources(db,f.owner,"/projects")).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await expect(assistantSources(db,f.owner,"/commerce?view=sales")).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await expect(assistantSources(db,f.owner,"/commerce/sales")).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await expect(assistantSources(db,f.owner,"/finance/engines")).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?, 'projects', 'pilot', 1, 'Invalid fixture rollout', ?, ?)").bind(f.company,f.owner.userId,Date.now()).run();
    await expect(assistantSources(db,f.owner,"/projects")).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await expect(assertProductionFeatureRoute(db,f.company,"projects","POST")).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?, 'pos', 'pilot', 1, 'Fixture acceptance', ?, ?)").bind(f.company,f.owner.userId,Date.now()).run();
    expect((await assistantSources(db,f.owner,"/pos?tab=sale")).map(source=>source.id.split(':')[0])).toEqual(["guide","record"]);
  } finally {vi.unstubAllEnvs();}
});
