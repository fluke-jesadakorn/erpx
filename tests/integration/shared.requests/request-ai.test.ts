import { env } from "../../support/runtime-env";
import { applyMigrations, type TestMigration } from "../../support/runtime-env";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {employeeRequestSchema,getEmployeeRequest,saveEmployeeRequest} from '@erpx/server-records/public/employee-requests';
import {
  queueRequestAi,
  runRequestAi,
  requestAiStatus,
  drainRequestAi,
} from "@erpx/server-records/public/request-ai";
import {
  activateAiConnection,
  aiConnectionSchema,
  saveAiConnection,
  selectAiProviders,
  testAiConnection,
  aiSelectionSchema,
  disableAiConnection,
} from "@erpx/server-agent/public/ai-providers";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
const aiEnv = {
  AI_CREDENTIAL_KEY_ID: "v1",
  AI_CREDENTIAL_KEYS: JSON.stringify({
    v1: btoa(String.fromCharCode(...new Uint8Array(32).fill(23))),
  }),
};
let f: Awaited<ReturnType<typeof expenseFixture>>;
const suggestion = {
  title: "Printer unavailable",
  description: "The office printer stops after printing one page.",
  category: "it",
  summary: "Check the printer reported by the requester.",
  sources: ["request"],
};
const generate = vi.fn(async () => ({
  result: suggestion,
  inputTokens: 20,
  outputTokens: 10,
}));
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});
beforeEach(async () => {
  f = await expenseFixture();
  generate.mockClear();
});
async function provider() {
  const c = await saveAiConnection(
    db,
    f.it,
    aiEnv,
    null,
    aiConnectionSchema.parse({
      name: "Service suggestions",
      baseUrl: "https://opencode.ai/zen/v1",
      apiKey: "synthetic-private-key",
      textModel: "gpt-5.6-luna",
    }),
    key(),
  );
  await testAiConnection(db, f.it, aiEnv, c.id, 1, key(), async () => ({
    result: { ok: true },
    inputTokens: 5,
    outputTokens: 2,
  }));
  await activateAiConnection(db, f.it, c.id, 2, key());
  await selectAiProviders(
    db,
    f.it,
    aiSelectionSchema.parse({
      expectedVersion: 0,
      mode: "custom",
      textConnectionId: c.id,
    }),
    key(),
  );
  return c;
}
async function request() {
  return saveEmployeeRequest(
    db,
    f.claimant,
    null,
    employeeRequestSchema.parse({
      kind: "service",
      title: "Human title",
      description: "The printer stops after one page. Please investigate.",
      location: "Office",
      branchId: f.branch,
      departmentId: f.department,
    }),
    key(),
  );
}
async function queue(id: string, version = 1, retry = false) {
  return queueRequestAi(
    db,
    f.claimant,
    aiEnv,
    id,
    { expectedVersion: version, retry },
    key(),
  );
}
it("preserves manual drafts when no company or deployment provider is available", async () => {
  const r = await request();
  expect(
    (await requestAiStatus(db, f.claimant, {}, r.id)).unavailable,
  ).toBeTruthy();
  await expect(queue(r.id)).rejects.toMatchObject({ status: 503 });
  expect((await getEmployeeRequest(db, f.claimant, r.id)).title).toBe(
    "Human title",
  );
});
it("does not call AI for a queued request after the company disables its production provider", async () => {
  await provider();
  const r = await request(), queued = await queue(r.id);
  await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'operations','pilot',1,'Test AI provider revocation',?,?)")
    .bind(f.company,f.it.userId,Date.now()).run();
  await db.prepare("UPDATE ai_company_settings SET mode='disabled',version=version+1 WHERE company_id=?").bind(f.company).run();
  vi.stubEnv("NODE_ENV", "production");
  try {
    await runRequestAi(db, aiEnv, queued.proposalId, generate);
    expect(generate).not.toHaveBeenCalled();
    expect((await getEmployeeRequest(db, f.claimant, r.id)).state).toBe("draft");
    expect((await db.prepare("SELECT state FROM request_ai_proposals WHERE id=?").bind(queued.proposalId).first<{state:string}>())?.state).toBe("failed");
  } finally {
    vi.unstubAllEnvs();
  }
});
it("does not queue or run request AI when Operations is outside the production rollout", async () => {
  await provider();
  const r = await request();
  vi.stubEnv("NODE_ENV", "production");
  try {
    await expect(queue(r.id)).rejects.toMatchObject({ code: "FEATURE_COMING_SOON" });
    vi.stubEnv("NODE_ENV", "test");
    const queued = await queue(r.id);
    vi.stubEnv("NODE_ENV", "production");
    await runRequestAi(db, aiEnv, queued.proposalId, generate);
    expect(generate).not.toHaveBeenCalled();
    expect((await getEmployeeRequest(db, f.claimant, r.id)).state).toBe("draft");
    expect((await db.prepare("SELECT state FROM request_ai_proposals WHERE id=?").bind(queued.proposalId).first<{ state: string }>())?.state).toBe("stale");
  } finally {
    vi.unstubAllEnvs();
  }
});
it("does not publish a suggestion when Operations is disabled during inference", async () => {
  await provider();
  const r = await request();
  await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'operations','pilot',1,'Test rollout race',?,?)")
    .bind(f.company,f.it.userId,Date.now()).run();
  vi.stubEnv("NODE_ENV", "production");
  try {
    const queued = await queue(r.id);
    await runRequestAi(db, aiEnv, queued.proposalId, async () => {
      await db.prepare("UPDATE company_feature_rollouts SET rollout_state='off',version=version+1 WHERE company_id=? AND module_id='operations'").bind(f.company).run();
      return { result: suggestion, inputTokens: 1, outputTokens: 1 };
    });
    const proposal = await db.prepare("SELECT state,result_json FROM request_ai_proposals WHERE id=?").bind(queued.proposalId).first<{ state: string; result_json: string | null }>();
    expect(proposal?.state).not.toBe("ready");
    expect(proposal?.result_json).toBeNull();
  } finally {
    vi.unstubAllEnvs();
  }
});
it("deduplicates queued preparation, records usage and never transitions or overwrites a human request", async () => {
  await provider();
  const r = await request(),
    queued = await queue(r.id);
  expect((await queue(r.id)).proposalId).toBe(queued.proposalId);
  await Promise.all([
    runRequestAi(db, aiEnv, queued.proposalId, generate),
    runRequestAi(db, aiEnv, queued.proposalId, generate),
  ]);
  expect(generate).toHaveBeenCalledTimes(1);
  expect((await requestAiStatus(db, f.claimant, aiEnv, r.id)).result).toEqual(
    suggestion,
  );
  const row = await getEmployeeRequest(db, f.claimant, r.id);
  expect(row.title).toBe("Human title");
  expect(row.state).toBe("draft");
  expect(row.version).toBe(1);
  expect(
    await db
      .prepare(
        "SELECT state,input_tokens,output_tokens FROM request_ai_usage WHERE id=?",
      )
      .bind(queued.proposalId)
      .first(),
  ).toMatchObject({ state: "succeeded", input_tokens: 20, output_tokens: 10 });
  const stored = await db
    .prepare(
      "SELECT actor_json,provider_json,result_json FROM request_ai_proposals WHERE id=?",
    )
    .bind(queued.proposalId)
    .first();
  expect(JSON.stringify(stored)).not.toContain("synthetic-private-key");
});
it("rejects stale preparation after human changes and after provider revisions change", async () => {
  const c = await provider(),
    r = await request(),
    queued = await queue(r.id);
  await runRequestAi(db, aiEnv, queued.proposalId, async () => {
    await db
      .prepare(
        "UPDATE employee_requests SET title=?,version=version+1 WHERE id=?",
      )
      .bind("Human correction", r.id)
      .run();
    return { result: suggestion, inputTokens: 20, outputTokens: 10 };
  });
  expect((await requestAiStatus(db, f.claimant, aiEnv, r.id)).state).toBe(
    "stale",
  );
  expect((await getEmployeeRequest(db, f.claimant, r.id)).title).toBe(
    "Human correction",
  );
  const fresh = await queue(r.id, 2);
  await disableAiConnection(
    db,
    f.it,
    c.id,
    { expectedVersion: 3, disableUses: true, remove: false },
    key(),
  );
  await runRequestAi(db, aiEnv, fresh.proposalId, generate);
  expect(generate).not.toHaveBeenCalled();
  expect(
    (await requestAiStatus(db, f.claimant, aiEnv, r.id)).result,
  ).toBeNull();
});
it("keeps failed suggestions retryable, validates output and rechecks revoked access", async () => {
  await provider();
  const r = await request(),
    queued = await queue(r.id);
  await runRequestAi(db, aiEnv, queued.proposalId, async () => ({
    result: { unsupported: true },
    inputTokens: 2,
    outputTokens: 1,
  }));
  expect((await requestAiStatus(db, f.claimant, aiEnv, r.id)).state).toBe(
    "failed",
  );
  expect((await queue(r.id, 1, true)).proposalId).toBe(queued.proposalId);
  await runRequestAi(db, aiEnv, queued.proposalId, generate);
  expect((await requestAiStatus(db, f.claimant, aiEnv, r.id)).state).toBe(
    "ready",
  );
  const r2 = await request(),
    q2 = await queue(r2.id);
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?")
    .bind(Date.now(), f.claimant.userId)
    .run();
  generate.mockClear();
  await runRequestAi(db, aiEnv, q2.proposalId, generate);
  expect(generate).not.toHaveBeenCalled();
  expect(
    (
      await db
        .prepare("SELECT state FROM request_ai_proposals WHERE id=?")
        .bind(q2.proposalId)
        .first<{ state: string }>()
    )?.state,
  ).toBe("stale");
});
it("releases exhausted worker leases and preserves the saved request", async () => {
  await provider();
  const r = await request(),
    q = await queue(r.id);
  await db
    .prepare(
      "UPDATE request_ai_proposals SET state='running',attempts=3,claim_until=1 WHERE id=?",
    )
    .bind(q.proposalId)
    .run();
  await drainRequestAi(db, aiEnv);
  expect((await requestAiStatus(db, f.claimant, aiEnv, r.id)).state).toBe(
    "failed",
  );
  expect((await getEmployeeRequest(db, f.claimant, r.id)).state).toBe("draft");
});

it("honors the shared company AI allowance without reserving work or changing the draft", async () => {
  await provider();
  const r = await request();
  await db
    .prepare(
      "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,20,5,0,1073741824,1)",
    )
    .bind(f.company)
    .run();
  await expect(queue(r.id)).rejects.toMatchObject({
    status: 429,
    code: "AI_QUOTA_EXCEEDED",
  });
  expect(
    (
      await db
        .prepare(
          "SELECT count(*) n FROM request_ai_proposals WHERE request_id=?",
        )
        .bind(r.id)
        .first<{ n: number }>()
    )?.n,
  ).toBe(0);
  expect((await getEmployeeRequest(db, f.claimant, r.id)).title).toBe(
    "Human title",
  );
});
