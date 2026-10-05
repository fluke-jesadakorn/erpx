import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  agentWorkspace,
  agentEvidenceRecord,
  changeAgentRecord,
  createAgent,
  createAgentAssignment,
  createAgentPool,
  createAgentVersion,
  agentSourceAllowed,
  changeAgentPool,
  changePrivateSession,
} from "../src/server/agent-governance";
import { askAssistant,assistantView } from "../src/server/workspace-assistant";
import { assistantAskSchema } from "../src/lib/ai/workspace-assistant";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
const aiEnv = {
  AI_BASE_URL: "https://opencode.ai/zen/go/v1",
  AI_MODEL: "muse-spark-1.3-contributor",
  AI_API_KEY: "synthetic-key",
};
let f: Awaited<ReturnType<typeof expenseFixture>>, reviewer: WorkContext;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.modules = ["agent"];
  reviewer = {
    ...f.tie,
    modules: ["agent"],
    role: "holding_owner",
    assignmentId: await f.grant(f.tie, "holding_owner"),
  };
});
async function approvedVersion(allowedSources=["/home", "/work"]) {
  const agent = await createAgent(
    f.owner,
    {
      name: "Work guide",
      purpose:
        "Read authorized sources and explain the next user-controlled step.",
    },
    key(),
  );
  await expect(
    changeAgentRecord(
      f.owner,
      "agent",
      String(agent.id),
      {
        action: "approve",
        expectedVersion: 1,
        reason: "Self approval forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeAgentRecord(
    reviewer,
    "agent",
    String(agent.id),
    {
      action: "approve",
      expectedVersion: 1,
      reason: "Purpose and authority boundary reviewed",
    },
    key(),
  );
  const v = await createAgentVersion(
    f.owner,
    {
      agentId: agent.id,
      prompt:
        "Explain evidence without changing, approving, signing, or paying any record.",
      allowedSources,
    },
    key(),
  );
  await changeAgentRecord(
    reviewer,
    "version",
    String(v.id),
    {
      action: "approve",
      expectedVersion: 1,
      reason: "Prompt and source allowlist reviewed",
    },
    key(),
  );
  return v;
}
it("routes an approved immutable version through a pool and records private invocation evidence", async () => {
  const v = await approvedVersion(),
    pool = await createAgentPool(
      f.owner,
      { name: "Service team", userIds: [f.claimant.userId] },
      key(),
    );
  await createAgentAssignment(
    f.owner,
    { agentVersionId: v.id, poolId: pool.id, scopePath: "/home" },
    key(),
  );
  const generate = vi.fn<NonNullable<Parameters<typeof askAssistant>[5]>>(async () => ({
      result: {
        answer: "Review the current work guide.",
        sourceIds: [],
        followUps: [],
      },
      inputTokens: 12,
      outputTokens: 7,
    })),
    turn = key();
  await askAssistant(
    db,
    { ...f.claimant, modules: ["agent"] },
    aiEnv,
    assistantAskSchema.parse({
      path: "/home",
      question: "What should I do next?",
    }),
    turn,
    generate,
  );
  const evidence = await db
    .prepare("SELECT * FROM ai_invocation_evidence WHERE turn_id=?")
    .bind(turn)
    .first<Record<string, string | number | null>>();
  expect(evidence).toMatchObject({
    agent_version_id: v.id,
    outcome: "succeeded",
    input_tokens: 12,
    output_tokens: 7,
    user_id: f.claimant.userId,
  });
  expect(JSON.parse(evidence.authority_snapshot_json)).toMatchObject({
    assignmentId: f.claimant.assignmentId,
    companyId: f.company,
  });
  const session = await db
    .prepare("SELECT * FROM ai_private_sessions WHERE id=?")
    .bind(evidence.session_id)
    .first<Record<string, string | number | null>>();
  expect(session).toMatchObject({
    user_id: f.claimant.userId,
    agent_assignment_id: expect.any(String),
  });
  const detail = await agentEvidenceRecord(f.owner, "agent_invocation", String(evidence.id));
  expect(detail.row).toMatchObject({turn_id:turn,outcome:"succeeded",input_tokens:12,output_tokens:7});
  expect(detail.row).not.toHaveProperty("provider_snapshot_json");
  expect(detail.row).not.toHaveProperty("authority_snapshot_json");
  await expect(agentEvidenceRecord({...f.owner,companyId:"outside-company"}, "agent_invocation", String(evidence.id))).rejects.toMatchObject({status:404});
  const ownSession = await agentEvidenceRecord({...f.claimant,modules:["agent"]}, "agent_session", String(evidence.session_id));
  expect(ownSession.sessionData?.sessions).toHaveLength(1);
  expect(ownSession.row).toMatchObject({user_id:f.claimant.userId,effective_status:"active"});
  await expect(agentEvidenceRecord({...f.tie,modules:["agent"]}, "agent_session", String(evidence.session_id))).rejects.toMatchObject({status:404});
  expect(JSON.stringify(generate.mock.calls[0][1])).toContain("Explain evidence without changing, approving, signing, or paying any record.");
  expect(JSON.parse(String(evidence.governance_snapshot_json))).toMatchObject({agentVersionId:v.id,allowedSources:["/home","/work"],promptDigest:expect.stringMatching(/^[a-f0-9]{64}$/)});
  await expect(
    db
      .prepare("DELETE FROM ai_invocation_evidence WHERE turn_id=?")
      .bind(turn)
      .run(),
  ).rejects.toThrow("AGENT_EVIDENCE_IMMUTABLE");
});
it("enforces approved source boundaries before a provider attempt and leaves the manual guide available",async()=>{
  const v=await approvedVersion(["/home"]);
  await createAgentAssignment(f.owner,{agentVersionId:v.id,userId:f.owner.userId,scopePath:"/"},key());
  const generate=vi.fn();
  await expect(askAssistant(db,f.owner,aiEnv,assistantAskSchema.parse({path:"/home",mode:"workspace",contextPaths:["/work"],question:"Use unapproved work evidence"}),key(),generate))
    .rejects.toMatchObject({code:"AI_SOURCE_FORBIDDEN"});
  expect(generate).not.toHaveBeenCalled();
  expect(Number(await db.prepare("SELECT count(*) FROM ai_workspace_turns WHERE company_id=?").bind(f.company).first("count"))).toBe(0);
  const view=await assistantView(db,f.owner,aiEnv,"/work","page");
  expect(view).toMatchObject({turns:[],unavailable:expect.stringContaining("approved source policy"),guide:{title:"My work"}});
  expect(agentSourceAllowed("/workforce",["/work"])).toBe(false);
  expect(agentSourceAllowed("/work?view=actions",["/work"])).toBe(true);
  expect(agentSourceAllowed("/commerce?view=sales",["/commerce?view=stock"])).toBe(false);
});
it("refuses an in-flight revoked agent assignment and persists failure evidence",async()=>{
  const v=await approvedVersion(),assignment=await createAgentAssignment(f.owner,{agentVersionId:v.id,userId:f.claimant.userId,scopePath:"/home"},key()),turn=key();
  await expect(askAssistant(db,f.claimant,aiEnv,assistantAskSchema.parse({path:"/home",question:"Must use current assignment"}),turn,async()=>{
    await changeAgentRecord(f.owner,"assignment",String(assignment.id),{action:"revoke",expectedVersion:1,reason:"Withdraw agent access while inference is running"},key());
    return {result:{answer:"This answer must stay hidden",sourceIds:[],followUps:[]},inputTokens:5,outputTokens:5};
  })).rejects.toMatchObject({code:"AI_AGENT_CHANGED"});
  expect(await db.prepare("SELECT state,answer_json FROM ai_workspace_turns WHERE id=?").bind(turn).first()).toMatchObject({state:"failed",answer_json:null});
  expect(await db.prepare("SELECT agent_version_id,outcome FROM ai_invocation_evidence WHERE turn_id=?").bind(turn).first()).toMatchObject({agent_version_id:v.id,outcome:"failed"});
});
it("keeps retired versions out of new invocations and replays lifecycle commands once",async()=>{
  const v=await approvedVersion(),assignment=await createAgentAssignment(f.owner,{agentVersionId:v.id,userId:f.claimant.userId,scopePath:"/home"},key()),command=key(),input={action:"retire",expectedVersion:2,reason:"Replace the reviewed version"};
  const result=await changeAgentRecord(f.owner,"version",String(v.id),input,command);
  expect(await changeAgentRecord(f.owner,"version",String(v.id),input,command)).toEqual(result);
  const generate=vi.fn();
  await expect(askAssistant(db,f.claimant,aiEnv,assistantAskSchema.parse({path:"/home",question:"Can the retired version run?"}),key(),generate)).rejects.toMatchObject({code:"AI_AGENT_UNAVAILABLE"});
  expect(generate).not.toHaveBeenCalled();
  const view=await assistantView(db,f.claimant,aiEnv,"/home","page");
  expect(view.guide.title).toBe("Today");expect(view.unavailable).toContain("no longer approved");
  expect(await db.prepare("SELECT status FROM agent_assignments WHERE id=?").bind(assignment.id).first()).toMatchObject({status:"active"});
  expect(Number(await db.prepare("SELECT count(*) FROM audit_events WHERE command_id=?").bind(command).first("count"))).toBe(1);
});
it("replaces eligible pool members atomically, retains revisions and removes the previous routing grant",async()=>{
  const v=await approvedVersion(),pool=await createAgentPool(f.owner,{name:"Replaceable routing pool",userIds:[f.claimant.userId]},key());
  await createAgentAssignment(f.owner,{agentVersionId:v.id,poolId:pool.id,scopePath:"/home"},key());
  const invalid=key();
  await expect(changeAgentPool(f.owner,String(pool.id),{action:"replace-members",expectedVersion:1,userIds:["not-a-current-company-user"],reason:"Ineligible replacement must fail"},invalid)).rejects.toMatchObject({code:"CONFLICT"});
  expect(await db.prepare("SELECT version FROM agent_pools WHERE id=?").bind(pool.id).first()).toMatchObject({version:1});
  const input={action:"replace-members",expectedVersion:1,userIds:[reviewer.userId],reason:"Use the independently staffed replacement pool"},command=key();
  await changeAgentPool(f.owner,String(pool.id),input,command);
  await changeAgentPool(f.owner,String(pool.id),input,command);
  const view=await agentWorkspace(f.owner);
  expect(view.members.find(member=>member.pool_id===pool.id&&member.user_id===f.claimant.userId)?.revoked_at).toEqual(expect.any(Number));
  expect(view.poolHistory.filter(revision=>revision.pool_id===pool.id)).toHaveLength(2);
  const generate=vi.fn<NonNullable<Parameters<typeof askAssistant>[5]>>(async()=>({result:{answer:"Default read-only guidance",sourceIds:[],followUps:[]},inputTokens:5,outputTokens:5})),turn=key();
  await askAssistant(db,f.claimant,aiEnv,assistantAskSchema.parse({path:"/home",question:"Which version is still assigned?"}),turn,generate);
  expect(await db.prepare("SELECT agent_version_id FROM ai_invocation_evidence WHERE turn_id=?").bind(turn).first()).toMatchObject({agent_version_id:null});
  expect(JSON.stringify(generate.mock.calls[0][1])).not.toContain("Approved agent version");
  await expect(db.prepare("DELETE FROM agent_pool_revisions WHERE pool_id=?").bind(pool.id).run()).rejects.toThrow("AGENT_EVIDENCE_IMMUTABLE");
});
it("allows only one concurrent pool replacement from the same version",async()=>{
  const pool=await createAgentPool(f.owner,{name:"Concurrent routing pool",userIds:[f.claimant.userId]},key()),input={action:"replace-members",expectedVersion:1,userIds:[reviewer.userId],reason:"Reviewed concurrent replacement"};
  const outcomes=await Promise.allSettled([changeAgentPool(f.owner,String(pool.id),input,key()),changeAgentPool(f.owner,String(pool.id),input,key())]);
  expect(outcomes.filter(outcome=>outcome.status==="fulfilled")).toHaveLength(1);
  expect(outcomes.filter(outcome=>outcome.status==="rejected")).toHaveLength(1);
  expect(Number(await db.prepare("SELECT count(*) FROM agent_pool_revisions WHERE pool_id=?").bind(pool.id).first("count"))).toBe(2);
});
it("retains held evidence across expiry and starts a fresh private session without old history",async()=>{
  const generate=vi.fn<NonNullable<Parameters<typeof askAssistant>[5]>>(async()=>({result:{answer:"Private response",sourceIds:[],followUps:[]},inputTokens:5,outputTokens:5})),input=assistantAskSchema.parse({path:"/home",question:"Preserve this evidence"}),turn=key();
  await askAssistant(db,f.claimant,aiEnv,input,turn,generate);
  const evidence=await db.prepare("SELECT session_id FROM ai_invocation_evidence WHERE turn_id=?").bind(turn).first<{session_id:string}>();
  const hold={action:"hold",expectedVersion:1,reason:"Preserve private invocation for investigation"},command=key();
  await changePrivateSession(f.owner,evidence!.session_id,hold,command);await changePrivateSession(f.owner,evidence!.session_id,hold,command);
  await expect(changePrivateSession(f.claimant,evidence!.session_id,{action:"release-hold",expectedVersion:2,reason:"Unauthorized release"},key())).rejects.toMatchObject({code:"PERMISSION_REQUIRED"});
  await changePrivateSession(f.claimant,evidence!.session_id,{action:"expire",expectedVersion:2,reason:"Close my private conversation"},key());
  expect((await assistantView(db,f.claimant,aiEnv,"/home","page")).turns).toHaveLength(0);
  const second=key();await askAssistant(db,f.claimant,aiEnv,input,second,generate);
  const next=await db.prepare("SELECT session_id FROM ai_invocation_evidence WHERE turn_id=?").bind(second).first<{session_id:string}>();
  expect(next!.session_id).not.toBe(evidence!.session_id);
  expect(await db.prepare("SELECT status,legal_hold,version FROM ai_private_session_controls WHERE session_id=?").bind(evidence!.session_id).first()).toMatchObject({status:"expired",legal_hold:true,version:3});
  expect(Number(await db.prepare("SELECT count(*) FROM ai_private_session_events WHERE session_id=?").bind(evidence!.session_id).first("count"))).toBe(2);
  expect(Number(await db.prepare("SELECT count(*) FROM ai_invocation_evidence WHERE session_id=?").bind(evidence!.session_id).first("count"))).toBe(1);
  expect(generate.mock.calls[0][0].sessionId).not.toBe(generate.mock.calls[1][0].sessionId);
});
it("rejects unapproved versions and keeps authority actions out of version policy", async () => {
  const v = await approvedVersion(),
    view = await agentWorkspace(f.owner);
  expect(
    JSON.parse(
      String(view.versions.find((x) => x.id === v.id)?.prohibited_actions_json),
    ),
  ).toEqual(["approve", "sign", "pay"]);
  const draft = await createAgentVersion(
    f.owner,
    {
      agentId: view.agents[0].id,
      prompt: "A second reviewed prompt is required before use.",
      allowedSources: ["/home"],
    },
    key(),
  );
  await expect(
    createAgentAssignment(
      f.owner,
      { agentVersionId: draft.id, userId: f.claimant.userId, scopePath: "/" },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
});
it("records provider failure without exposing another user's private session", async () => {
  const v = await approvedVersion();
  await createAgentAssignment(
    f.owner,
    { agentVersionId: v.id, userId: f.claimant.userId, scopePath: "/home" },
    key(),
  );
  const turn = key();
  await expect(
    askAssistant(
      db,
      f.claimant,
      aiEnv,
      assistantAskSchema.parse({
        path: "/home",
        question: "Cause simulated failure",
      }),
      turn,
      async () => {
        throw new Error("provider unavailable");
      },
    ),
  ).rejects.toThrow("provider unavailable");
  expect(
    await db
      .prepare(
        "SELECT outcome,error FROM ai_invocation_evidence WHERE turn_id=?",
      )
      .bind(turn)
      .first<Record<string, string | number | null>>(),
  ).toMatchObject({ outcome: "failed", error: "provider unavailable" });
  expect(
    Number(
      await db
        .prepare("SELECT count(*) c FROM ai_private_sessions WHERE user_id=?")
        .bind(f.owner.userId)
        .first("c"),
    ),
  ).toBe(0);
});
