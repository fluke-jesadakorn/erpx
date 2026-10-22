import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { guidedIntakeHandoffKey, guidedIntakeInputSchema, saveGuidedIntakeHandoff } from "@erpx/feature-shared/lib/ai/guided-intake";
import { guidedIntakeOptions, prepareGuidedIntake } from "@erpx/server-operations/public/guided-intake";
import {createAgent,createAgentVersion,createAgentAssignment,changeAgentRecord,changePrivateSession} from "@erpx/server-agent/public/agent-governance";
import type { AiEndpoint, AiMessage } from "@erpx/server-agent/public/ai-transport";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
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
  expect(result.actions.find(action=>action.suggested)?.href).toBe("/holdings/test/services/desk/new?kind=purchase");
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

async function assignedProposalAgent(){
  const reviewer=await f.member("proposal-policy-reviewer","holding_owner",10);
  f.owner.modules=["agent"];reviewer.modules=["agent"];
  const agent=await createAgent(f.owner,{name:"Reviewed proposal assistant",purpose:"Propose fields for human review only"},key());
  await changeAgentRecord(reviewer,"agent",agent.id,{action:"approve",expectedVersion:1,reason:"Independent catalogue review"},key());
  const version=await createAgentVersion(f.owner,{agentId:agent.id,prompt:"Preserve explicit task text. Never submit a business command.",allowedSources:["/home"]},key());
  await changeAgentRecord(reviewer,"version",version.id,{action:"approve",expectedVersion:1,reason:"Independent approved source and prompt review"},key());
  const assignment=await createAgentAssignment(f.owner,{agentVersionId:version.id,userId:f.claimant.userId,scopePath:"/"},key());
  return {reviewer,version,assignment};
}
it("enforces the independently approved Agent prompt and source policy for reviewed field proposals",async()=>{
  const {version}=await assignedProposalAgent(),generate=vi.fn(async(_endpoint:AiEndpoint,messages:AiMessage[])=>{
    expect(messages.find(message=>typeof message.content==='string'&&message.content.includes(`Approved agent version ${version.id}`))?.content).toContain("Never submit a business command.");
    return {result:{intent:"task",summary:"Review the proposed task",draft:{title:"Fictional task only"},sourceIds:[]},inputTokens:8,outputTokens:4};
  });
  const input=guidedIntakeInputSchema.parse({message:"Create a Fictional task only",contextPath:"/home"}),id=key();
  expect(await prepareGuidedIntake(db,f.claimant,aiEnv,input,id,generate)).toMatchObject({intent:"task",draft:{title:"Fictional task only"}});
  expect(await prepareGuidedIntake(db,f.claimant,aiEnv,input,id,generate)).toMatchObject({intent:"task"});expect(generate).toHaveBeenCalledTimes(1);
  await expect(prepareGuidedIntake(db,f.claimant,aiEnv,{...input,contextPath:"/work"},key(),generate)).rejects.toMatchObject({code:"AI_SOURCE_FORBIDDEN"});expect(generate).toHaveBeenCalledTimes(1);
  expect(await db.prepare("SELECT count(*) n FROM staff_tasks WHERE company_id=?").bind(f.company).first("n")).toBe(0);
  const row=await db.prepare("SELECT t.private_session_id,e.governance_snapshot_json,e.session_id FROM ai_workspace_turns t JOIN ai_invocation_evidence e ON e.turn_id=t.id WHERE t.id=?").bind(id).first<{private_session_id:string;governance_snapshot_json:string;session_id:string}>();
  expect(JSON.parse(row!.governance_snapshot_json)).toMatchObject({agentVersionId:version.id,allowedSources:["/home"]});expect(row!.session_id).toBe(row!.private_session_id);
  await changePrivateSession(f.owner,row!.private_session_id,{action:"hold",expectedVersion:1,reason:"Preserve the reviewed private proposal evidence"},key());
  await changePrivateSession(f.claimant,row!.private_session_id,{action:"expire",expectedVersion:2,reason:"User starts a separate private session"},key());
  await expect(prepareGuidedIntake(db,f.claimant,aiEnv,input,id,generate)).rejects.toMatchObject({code:"AI_REQUEST_EXISTS"});expect(generate).toHaveBeenCalledTimes(1);
  const nextId=key();
  expect(await prepareGuidedIntake(db,f.claimant,aiEnv,input,nextId,generate)).toMatchObject({intent:"task"});
  expect(generate).toHaveBeenCalledTimes(2);
  expect(generate.mock.calls[0][0].sessionId).not.toBe(generate.mock.calls[1][0].sessionId);
  const next=await db.prepare("SELECT private_session_id FROM ai_workspace_turns WHERE id=?").bind(nextId).first<{private_session_id:string}>();
  expect(next!.private_session_id).not.toBe(row!.private_session_id);
  expect(await db.prepare("SELECT status,legal_hold,version FROM ai_private_session_controls WHERE session_id=?").bind(row!.private_session_id).first()).toEqual({status:"expired",legal_hold:true,version:3});
  expect(await db.prepare("SELECT count(*) FROM ai_invocation_evidence WHERE session_id=?").bind(row!.private_session_id).first("count")).toBe(1);
  expect(await db.prepare("SELECT count(*) FROM ai_invocation_evidence WHERE session_id=?").bind(next!.private_session_id).first("count")).toBe(1);
});
it("rejects a field proposal when approved Agent governance changes while its provider is working",async()=>{
  const {assignment}=await assignedProposalAgent(),id=key(),generate=vi.fn(async()=>{
    await changeAgentRecord(f.owner,"assignment",assignment.id,{action:"revoke",expectedVersion:1,reason:"Withdraw this exact source policy during inference"},key());
    return {result:{intent:"task",summary:"Withdrawn proposal",draft:{title:"Must remain unavailable"},sourceIds:[]},inputTokens:8,outputTokens:4};
  });
  await expect(prepareGuidedIntake(db,f.claimant,aiEnv,guidedIntakeInputSchema.parse({message:"Create a task",contextPath:"/home"}),id,generate)).rejects.toMatchObject({code:"AI_AGENT_CHANGED"});
  expect(await db.prepare("SELECT state,answer_json FROM ai_workspace_turns WHERE id=?").bind(id).first()).toEqual({state:"failed",answer_json:null});
  expect(await db.prepare("SELECT count(*) n FROM staff_tasks WHERE company_id=?").bind(f.company).first("n")).toBe(0);
});
