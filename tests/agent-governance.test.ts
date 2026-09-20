import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  agentWorkspace,
  changeAgentRecord,
  createAgent,
  createAgentAssignment,
  createAgentPool,
  createAgentVersion,
} from "../src/server/agent-governance";
import { askAssistant } from "../src/server/workspace-assistant";
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
async function approvedVersion() {
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
      allowedSources: ["/home", "/work"],
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
  const generate = vi.fn(async () => ({
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
  await expect(
    db
      .prepare("DELETE FROM ai_invocation_evidence WHERE turn_id=?")
      .bind(turn)
      .run(),
  ).rejects.toThrow("AGENT_EVIDENCE_IMMUTABLE");
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
