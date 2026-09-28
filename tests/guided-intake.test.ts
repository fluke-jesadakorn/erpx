import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { guidedIntakeHandoffKey, guidedIntakeInputSchema, saveGuidedIntakeHandoff } from "../src/lib/ai/guided-intake";
import { guidedIntakeOptions, prepareGuidedIntake } from "../src/server/guided-intake";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const aiEnv = {AI_BASE_URL:"https://opencode.ai/zen/go/v1",AI_MODEL:"muse-spark-1.3-contributor",AI_API_KEY:"synthetic-key"};
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});

it("isolates a browser handoff by assignment and workspace",()=>{
  const current=guidedIntakeHandoffKey("company","user","assignment-a","/holdings/acme","purchase");
  expect(current).not.toBe(guidedIntakeHandoffKey("company","user","assignment-b","/holdings/acme","purchase"));
  expect(current).not.toBe(guidedIntakeHandoffKey("company","user","assignment-a","/holdings/acme/branches/east","purchase"));
});

it("reports storage failure so Continue can retain the prepared draft",()=>{
  vi.stubGlobal("sessionStorage",{setItem:()=>{throw new Error("storage disabled");}});
  try { expect(saveGuidedIntakeHandoff("company","user","assignment","/holdings/acme","task",{title:"Follow up"})).toBe(false); }
  finally { vi.unstubAllGlobals(); }
});

it("prepares a purchase draft with permitted canonical actions, without creating a purchase request",async()=>{
  const generate=vi.fn(async()=>({result:{intent:"purchase",summary:"The design team needs monitors.",draft:{itemName:"monitors",quantity:"2",title:"Two monitors",description:"For the design team",neededDate:"2026-10-10",supplierId:"secret"},sourceIds:[]},inputTokens:10,outputTokens:7}));
  const input=guidedIntakeInputSchema.parse({message:"I need 2 monitors for the design team",contextPath:"/home"});
  const id=key();
  const result=await prepareGuidedIntake(db,f.claimant,aiEnv,input,id,generate);
  expect(result.intent).toBe("purchase");
  expect(result.draft).toEqual({title:"Two monitors",description:"For the design team",itemName:"monitors",quantity:"2"});
  expect(result.missing).toEqual([]);
  expect(result.actions.find(action=>action.suggested)?.href).toContain("/requests/new?kind=purchase");
  expect(result.actions.find(action=>action.id==="leave")?.href).toContain("/leave/new");
  expect((await db.prepare("SELECT count(*)::int AS n FROM employee_requests WHERE company_id=?").bind(f.company).first<{n:number}>())?.n).toBe(0);
  expect(await prepareGuidedIntake(db,f.claimant,aiEnv,input,id,generate)).toMatchObject({intent:"purchase"});
  expect(generate).toHaveBeenCalledTimes(1);
});

it("does not let model output create routes, unavailable intents, or invented money",async()=>{
  const generate=vi.fn(async()=>({result:{intent:"expense",summary:"A claim might be needed.",draft:{paymentIntent:"reimbursement",amount:"9000",currency:"THB",merchant:"Cafe",businessPurpose:"Client meeting",expenseDate:"2026-09-21",approverId:"owner"},sourceIds:["fake-source"]},inputTokens:8,outputTokens:5}));
  const input=guidedIntakeInputSchema.parse({message:"I paid for the client meeting at Cafe"});
  const result=await prepareGuidedIntake(db,f.claimant,aiEnv,input,key(),generate);
  expect(result.draft).toEqual({paymentIntent:"reimbursement",merchant:"Cafe",businessPurpose:"Client meeting"});
  expect(result.missing).toContain("Amount");
  expect(result.missing).toContain("Receipt or payment evidence");
  expect(result.sources.map(source=>source.id)).toEqual(["message"]);
  expect(result.actions.find(action=>action.suggested)?.href).toContain("/expenses/new");
  expect(result.actions.every(action=>!action.href.includes("javascript:"))).toBe(true);
});

it("recognizes an explicit number word next to the item and fills a plain title",async()=>{
  const generate=vi.fn(async()=>({result:{intent:"purchase",summary:"The team needs monitors.",draft:{itemName:"monitors",description:"For the design team",quantity:"2"},sourceIds:[]},inputTokens:8,outputTokens:5}));
  const result=await prepareGuidedIntake(db,f.claimant,aiEnv,guidedIntakeInputSchema.parse({message:"I need two monitors for the design team"}),key(),generate);
  expect(result.draft).toMatchObject({title:"monitors",itemName:"monitors",quantity:"2"});
  expect(result.missing).toEqual([]);
});

it("rechecks access after inference and never returns a stale prepared draft",async()=>{
  const generate=vi.fn(async()=>{
    await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();
    return {result:{intent:"task",summary:"Create a task.",draft:{title:"Follow up"},sourceIds:[]},inputTokens:8,outputTokens:4};
  });
  await expect(prepareGuidedIntake(db,f.claimant,aiEnv,guidedIntakeInputSchema.parse({message:"Create a follow up task"}),key(),generate)).rejects.toMatchObject({code:"WORK_CONTEXT_CHANGED"});
  expect((await db.prepare("SELECT state FROM ai_workspace_turns WHERE company_id=? ORDER BY created_at DESC LIMIT 1").bind(f.company).first<{state:string}>())?.state).toBe("failed");
});

it("keeps direct forms available if the company disables AI",async()=>{
  const options=await guidedIntakeOptions(db,f.claimant,aiEnv);
  expect(options.actions.some(action=>action.id==="purchase")).toBe(true);
  await db.prepare("INSERT INTO ai_company_settings(company_id,mode,version,updated_by,updated_at) VALUES(?,'disabled',1,?,?) ON CONFLICT(company_id) DO UPDATE SET mode='disabled',version=ai_company_settings.version+1").bind(f.company,f.owner.userId,Date.now()).run();
  const disabled=await guidedIntakeOptions(db,f.claimant,aiEnv);
  expect(disabled.unavailable).toContain("disabled");
  expect(disabled.actions.some(action=>action.id==="purchase")).toBe(true);
});
