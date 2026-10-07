import * as nativeHttps from "../src/server/runtime/https";
import { env } from "./runtime-env";
import { applyMigrations, type TestMigration } from "./runtime-env";
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  activateAiConnection,
  aiSnapshotGuard,
  aiConnectionSchema,
  aiProviderOverview,
  disableAiConnection,
  resolveAiProvider,
  saveAiConnection,
  selectAiProviders,
  testAiConnection,
  aiSelectionSchema,
  aiDiscoverySchema,
  discoverAiModelsInput,
} from "../src/server/ai-providers";
import {
  aiBaseUrl,
  generateProviderJson,
  openAiKey,
  publicAddress,
  sealAiKey,
} from "../src/server/ai-transport";
import { administrationAccess } from "../src/server/administration";
import {
  grantPersonAccess,
  grantSchema,
} from "../src/server/account-access";
import {
  queueExpenseAi,
  runExpenseAi,
  expenseAiStatus,
} from "../src/server/expense-ai";
import { saveDocumentPage } from "../src/server/document-derivatives";
import { getExpense } from "../src/server/expenses";
vi.mock("../src/server/runtime", () => ({
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
const synthetic = vi.fn(
  async (_endpoint: unknown, messages: { content: unknown }[]) => ({
    result: Array.isArray(messages[1].content)
      ? { color: "red" }
      : { ok: true },
    inputTokens: 5,
    outputTokens: 2,
  }),
);
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});
beforeEach(async () => {
  f = await expenseFixture();
  synthetic.mockClear();
});
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });
it("builds the simulation AI snapshot guard without deployment parameters", async () => {
  const commandId = key();
  await db.batch([
    aiSnapshotGuard(db, commandId, {
      companyId: f.company,
      connectionId: null,
      revision: 1,
      settingsVersion: 0,
      model: "thai-hotel-v1-ai-deterministic-v1",
      kind: "text",
      deploymentDigest: "simulation:thai-hotel-v1-ai-deterministic-v1",
    }),
    db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
  ]);
});
it("requires explicit company selection before any production AI provider can run", async () => {
  const deployment = {
    ...aiEnv,
    AI_BASE_URL: "https://example.invalid/v1",
    AI_MODEL: "fictional-review-model",
    AI_API_KEY: "synthetic-deployment-key",
  };
  vi.stubEnv("NODE_ENV", "production");
  try {
    expect((await aiProviderOverview(db, f.it, deployment)).selection.mode).toBe("disabled");
    await expect(resolveAiProvider(db, f.company, deployment, "text")).rejects.toMatchObject({ code: "AI_PILOT_OPT_IN_REQUIRED" });
    await expect(resolveAiProvider(db, f.company, deployment, "image")).rejects.toMatchObject({ code: "AI_PILOT_OPT_IN_REQUIRED" });
    await selectAiProviders(db, f.it, aiSelectionSchema.parse({ expectedVersion: 0, mode: "deployment" }), key());
    expect((await resolveAiProvider(db, f.company, deployment, "text")).endpoint.model).toBe("fictional-review-model");
    await expect(resolveAiProvider(db, f.company, deployment, "image")).rejects.toMatchObject({ code: "AI_IMAGE_TEST_REQUIRED" });
    await selectAiProviders(db, f.it, aiSelectionSchema.parse({ expectedVersion: 1, mode: "disabled" }), key());
    await expect(resolveAiProvider(db, f.company, deployment, "text")).rejects.toMatchObject({ code: "AI_PILOT_OPT_IN_REQUIRED" });
  } finally {
    vi.unstubAllEnvs();
  }
});
async function connection() {
  const row = await saveAiConnection(
    db,
    f.it,
    aiEnv,
    null,
    aiConnectionSchema.parse({
      name: "Custom provider",
      baseUrl: "https://opencode.ai/zen/v1",
      apiKey: "private-test-key",
      textModel: "gpt-5.6-luna",
      imageModel: "claude-sonnet-4-6",
    }),
    key(),
  );
  await testAiConnection(db, f.it, aiEnv, row.id, 1, key(), synthetic);
  await activateAiConnection(db, f.it, row.id, 2, key());
  await selectAiProviders(
    db,
    f.it,
    aiSelectionSchema.parse({
      expectedVersion: 0,
      mode: "custom",
      textConnectionId: row.id,
    }),
    key(),
  );
  return row.id;
}
it("uses only the selected IT assignment without granting billing or role administration", async () => {
  const access = await administrationAccess(f.it);
  expect(access.manageAiProviders).toBe(true);
  expect(access.owner).toBe(false);
  expect(access.sections).not.toContain("usage");
  expect(
    grantSchema.safeParse({
      role: "it_admin",
      scope: "branch",
      expiresAt: "2027-01-01",
    }).success,
  ).toBe(false);
  await expect(
    f.grant(f.claimant, "it_admin", "branch", f.branch),
  ).rejects.toThrow();
  await expect(
    saveAiConnection(
      db,
      f.claimant,
      aiEnv,
      null,
      aiConnectionSchema.parse({
        name: "No access",
        baseUrl: "https://api.provider.com",
        textModel: "text",
      }),
      key(),
    ),
  ).rejects.toMatchObject({ status: 403 });
  await expect(
    grantPersonAccess(
      f.it,
      `person-${f.claimant.userId}`,
      grantSchema.parse({
        role: "it_admin",
        scope: "holding",
        expiresAt: "2027-01-01",
      }),
      key(),
    ),
  ).rejects.toMatchObject({ status: 403 });
  // Grants are reviewed through IAM; fixture setup isolates role-selection behavior.
  const role = {id:await f.grant(f.claimant,"it_admin")};
  expect((await administrationAccess(f.claimant)).manageAiProviders).toBe(false);
  const selectedIt = {...f.claimant,role:"it_admin",assignmentId:String(role.id)};
  expect((await administrationAccess(selectedIt)).manageAiProviders).toBe(true);
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(),role.id).run();
  expect((await administrationAccess(selectedIt)).manageAiProviders).toBe(
    false,
  );
});
it("encrypts credentials with identity context, returns only masked status, and preserves active revisions", async () => {
  const id = await connection(),
    overview = await aiProviderOverview(db, f.owner, aiEnv);
  expect(JSON.stringify(overview)).not.toContain("private-test-key");
  expect(JSON.stringify(overview)).not.toContain("credential_json");
  expect(overview.connections[0].hasKey).toBe(true);
  const rev = await db
    .prepare(
      "SELECT credential_json FROM ai_connection_revisions WHERE connection_id=? AND revision=1",
    )
    .bind(id)
    .first<{ credential_json: string }>();
  expect(rev!.credential_json).not.toContain("private-test-key");
  expect(await openAiKey(aiEnv, f.company, id, rev!.credential_json)).toBe(
    "private-test-key",
  );
  await expect(
    openAiKey(aiEnv, "another-company", id, rev!.credential_json),
  ).rejects.toBeDefined();
  expect(await sealAiKey(aiEnv, f.company, id, "private-test-key")).not.toBe(
    rev!.credential_json,
  );
  await saveAiConnection(
    db,
    f.it,
    aiEnv,
    id,
    aiConnectionSchema.parse({
      expectedVersion: 3,
      name: "Edited draft",
      baseUrl: "https://opencode.ai/zen/go/v1",
      apiKey: "replacement-test-key",
      textModel: "gpt-5.5",
      imageModel: "claude-haiku-4-5",
    }),
    key(),
  );
  expect(
    (await resolveAiProvider(db, f.company, aiEnv, "text")).endpoint.model,
  ).toBe("gpt-5.6-luna");
  await expect(
    activateAiConnection(db, f.it, id, 4, key()),
  ).rejects.toMatchObject({ code: "AI_TEST_REQUIRED" });
  await testAiConnection(db, f.it, aiEnv, id, 4, key(), synthetic);
  await activateAiConnection(db, f.it, id, 5, key());
  expect(
    (await resolveAiProvider(db, f.company, aiEnv, "text")).endpoint.model,
  ).toBe("gpt-5.5");
  const audit = await db
    .prepare("SELECT purpose FROM audit_events WHERE record_id=?")
    .bind(id)
    .all();
  expect(JSON.stringify(audit.results)).not.toContain("private-test-key");
});
it("checks authority after tests and blocks stale configuration updates and silent fallback", async () => {
  const id = await connection();
  await expect(
    saveAiConnection(
      db,
      f.it,
      aiEnv,
      id,
      aiConnectionSchema.parse({
        expectedVersion: 1,
        name: "Conflict",
        baseUrl: "https://opencode.ai/zen/v1",
        textModel: "gpt-5.5",
      }),
      key(),
    ),
  ).rejects.toMatchObject({ status: 409 });
  await expect(
    disableAiConnection(
      db,
      f.it,
      id,
      { expectedVersion: 3, disableUses: false, remove: false },
      key(),
    ),
  ).rejects.toMatchObject({ code: "AI_CONNECTION_IN_USE" });
  await disableAiConnection(
    db,
    f.it,
    id,
    { expectedVersion: 3, disableUses: true, remove: false },
    key(),
  );
  await expect(
    resolveAiProvider(
      db,
      f.company,
      {
        ...aiEnv,
        AI_BASE_URL: "https://deployment.com",
        AI_API_KEY: "default",
        AI_MODEL: "default",
      },
      "text",
    ),
  ).rejects.toMatchObject({ code: "AI_DISABLED" });
  const revoke = async () => {
    await db
      .prepare(
        "UPDATE assignments SET revoked_at=1 WHERE user_id=? AND role='it_admin'",
      )
      .bind(f.it.userId)
      .run();
    return { result: { ok: true }, inputTokens: 0, outputTokens: 0 };
  };
  await expect(
    testAiConnection(db, f.it, aiEnv, id, 4, key(), revoke),
  ).rejects.toMatchObject({ status: 409 });
  await expect(aiProviderOverview(db, f.it, aiEnv)).rejects.toMatchObject({
    status: 403,
  });
});
it("rejects private URLs, DNS targets and redirects while allowing explicit public unauthenticated JSON endpoints", async () => {
  for (const url of [
    "http://provider.com",
    "https://user:key@provider.com",
    "https://localhost",
    "https://127.0.0.1",
    "https://[::1]",
    "https://10.0.0.1",
    "https://api.internal",
  ])
    expect(() => aiBaseUrl(url)).toThrow();
  for (const ip of [
    "127.0.0.1",
    "10.0.0.1",
    "172.16.0.1",
    "192.168.1.1",
    "169.254.169.254",
    "100.64.0.1",
    "::1",
    "fc00::1",
    "::ffff:127.0.0.1",
  ])
    expect(publicAddress(ip)).toBe(false);
  const fetcher = vi.fn(
    async (url: string | URL | Request, init?: RequestInit) => {
      if (String(url).includes("cloudflare-dns.com"))
        return Response.json({
          Status: 0,
          Answer: [{ type: 1, data: "8.8.8.8" }],
        });
      expect(init?.redirect).toBe("error");
      expect(init?.headers).not.toHaveProperty("Authorization");
      return Response.json({
        choices: [
          { finish_reason: "stop", message: { content: '{"ok":true}' } },
        ],
        usage: { prompt_tokens: 3, completion_tokens: 2 },
      });
    },
  );
  vi.stubGlobal("fetch", fetcher);
  vi.spyOn(nativeHttps, "pinnedNodeRequest").mockImplementation(async (url,addresses,headers,body) => {
    expect([...new Set(addresses)]).toEqual(["8.8.8.8"]);
    return fetcher(url, { headers, redirect: "error", ...(body ? { body: JSON.stringify(body) } : {}) });
  });
  expect(
    (
      await generateProviderJson(
        {
          baseUrl: "https://public.provider.com/v1",
          apiKey: null,
          model: "test",
        },
        [{ role: "user", content: "Synthetic JSON test" }],
      )
    ).result,
  ).toEqual({ ok: true });
  fetcher.mockImplementation(async () =>
    Response.json({ Status: 0, Answer: [{ type: 1, data: "127.0.0.1" }] }),
  );
  await expect(
    generateProviderJson(
      {
        baseUrl: "https://public.provider.com/v1",
        apiKey: null,
        model: "test",
      },
      [],
    ),
  ).rejects.toMatchObject({ code: "AI_PRIVATE_ENDPOINT" });
});
it("prepares cited expense fields without changing the claim and invalidates changed sources", async () => {
  await connection();
  const id = await f.claim(),
    attachment = await f.attach(id),
    queued = await queueExpenseAi(
      db,
      f.claimant,
      aiEnv,
      id,
      { expectedVersion: 1, paymentId: null, retry: false },
      key(),
    );
  const output = {
    summary: "Receipt shows the supplier charge.",
    fields: [
      {
        name: "merchant" as const,
        value: "Suggested merchant",
        citations: [{ attachmentId: attachment, page: 1 }],
      },
    ],
    missing: ["Purpose needs human review"],
    conflicts: [],
  };
  const generator = vi.fn(async () => ({
    result: output,
    inputTokens: 10,
    outputTokens: 5,
  }));
  await runExpenseAi(
    db,
    env.H001_FILES,
    aiEnv,
    String(queued.proposalId),
    generator,
  );
  expect(generator).toHaveBeenCalledTimes(1);
  expect((await getExpense(f.claimant, id)).merchant).toBe("Supplier");
  expect(
    (await expenseAiStatus(db, f.claimant, aiEnv, id, null)).proposal?.state,
  ).toBe("ready");
  await db
    .prepare("UPDATE finance_expenses SET version=version+1 WHERE id=?")
    .bind(id)
    .run();
  const status = await expenseAiStatus(db, f.claimant, aiEnv, id, null);
  expect(status.proposal).toBeNull();
  expect(status.stale).toBe(true);
  const fresh = await queueExpenseAi(
    db,
    f.claimant,
    aiEnv,
    id,
    { expectedVersion: 2, paymentId: null, retry: false },
    key(),
  );
  await db
    .prepare("UPDATE finance_expenses SET version=version+1 WHERE id=?")
    .bind(id)
    .run();
  await runExpenseAi(
    db,
    env.H001_FILES,
    aiEnv,
    String(fresh.proposalId),
    generator,
  );
  expect(generator).toHaveBeenCalledTimes(1);
  expect(
    await db
      .prepare("SELECT state FROM expense_ai_proposals WHERE id=?")
      .bind(fresh.proposalId)
      .first(),
  ).toEqual({ state: "stale" });
});
it("stores PDF pages under original access and quota, rejects missing pages and preserves drafts on AI failure", async () => {
  await connection();
  const id = await f.claim(),
    attachment = await f.attach(
      id,
      "expense_claim",
      "pdf-original",
      "application/pdf",
    );
  await expect(
    queueExpenseAi(
      db,
      f.claimant,
      aiEnv,
      id,
      { expectedVersion: 1, paymentId: null, retry: false },
      key(),
    ),
  ).rejects.toMatchObject({ code: "PDF_PREPARATION_REQUIRED" });
  const jpeg = new Uint8Array([255, 216, 255, 217]).buffer;
  const pageInput={page:1,pageCount:2,originalChecksum:"pdf-original",bytes:jpeg},pageKey=key();
  const pageFilesBefore=(await env.H001_FILES.list({prefix:`${f.claimant.holdingId}/document-pages/`})).objects.map(object=>object.key).sort();
  const trigger="page_late_"+pageKey.replace(/[^a-zA-Z0-9]/g,""),fn=trigger+"_failure";
  await db.prepare(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'late document page outbox failure' USING ERRCODE='23514'; END $$`).run();
  await db.prepare(`CREATE TRIGGER ${trigger} BEFORE INSERT ON outbox_events FOR EACH ROW WHEN (NEW.command_id='${pageKey}') EXECUTE FUNCTION ${fn}()`).run();
  try {
    await expect(saveDocumentPage(db,env.H001_FILES,f.claimant,attachment,pageInput,pageKey)).rejects.toThrow("late document page outbox failure");
    expect(await db.prepare("SELECT COUNT(*)::int n FROM attachment_documents WHERE attachment_id=?").bind(attachment).first("n")).toBe(0);
    expect(await db.prepare("SELECT COUNT(*)::int n FROM attachment_derivatives WHERE attachment_id=?").bind(attachment).first("n")).toBe(0);
    expect(await db.prepare("SELECT COUNT(*)::int n FROM attachment_storage_ledger WHERE attachment_id=? AND variant='derivative'").bind(attachment).first("n")).toBe(0);
    for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT COUNT(*)::int n FROM ${table} WHERE command_id=?`).bind(pageKey).first("n")).toBe(0);
    expect((await env.H001_FILES.list({prefix:`${f.claimant.holdingId}/document-pages/`})).objects.map(object=>object.key).sort()).toEqual(pageFilesBefore);
  } finally {await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fn}()`).run();}
  const rendered=await saveDocumentPage(db,env.H001_FILES,f.claimant,attachment,pageInput,pageKey);
  expect(await saveDocumentPage(db,env.H001_FILES,f.claimant,attachment,pageInput,pageKey)).toEqual(rendered);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT COUNT(*)::int n FROM ${table} WHERE command_id=?`).bind(pageKey).first("n")).toBe(1);
  await expect(
    queueExpenseAi(
      db,
      f.claimant,
      aiEnv,
      id,
      { expectedVersion: 1, paymentId: null, retry: false },
      key(),
    ),
  ).rejects.toMatchObject({ code: "PDF_PREPARATION_REQUIRED" });
  await saveDocumentPage(
    db,
    env.H001_FILES,
    f.claimant,
    attachment,
    { page: 2, pageCount: 2, originalChecksum: "pdf-original", bytes: jpeg },
    key(),
  );
  expect(
    await db
      .prepare(
        "SELECT count(*)::int count,COALESCE(sum(bytes),0)::int bytes FROM attachment_storage_ledger WHERE attachment_id=? AND variant='derivative' AND state='stored'",
      )
      .bind(attachment)
      .first(),
  ).toEqual({ count: 2, bytes: jpeg.byteLength * 2 });
  const proposal = await queueExpenseAi(
    db,
    f.claimant,
    aiEnv,
    id,
    { expectedVersion: 1, paymentId: null, retry: false },
    key(),
  );
  await runExpenseAi(
    db,
    env.H001_FILES,
    aiEnv,
    String(proposal.proposalId),
    async () => {
      throw Error("Provider unavailable");
    },
  );
  expect(
    (await expenseAiStatus(db, f.claimant, aiEnv, id, null)).proposal?.state,
  ).toBe("failed");
  expect((await getExpense(f.claimant, id)).status).toBe("draft");
  expect((await getExpense(f.claimant, id)).merchant).toBe("Supplier");
  await expect(
    saveDocumentPage(
      db,
      env.H001_FILES,
      { ...f.claimant, companyId: "other" },
      attachment,
      { page: 1, pageCount: 2, originalChecksum: "pdf-original", bytes: jpeg },
      key(),
    ),
  ).rejects.toBeDefined();
});

it("deduplicates queued work, leaves conflicting evidence blank and attributes both models", async () => {
  await connection();
  const id = await f.claim();
  await f.attach(id);
  await f.attach(id);
  await f.attach(id);
  const input = { expectedVersion: 1, paymentId: null, retry: false };
  const queued = await queueExpenseAi(db, f.claimant, aiEnv, id, input, key());
  expect(
    (await queueExpenseAi(db, f.claimant, aiEnv, id, input, key())).proposalId,
  ).toBe(queued.proposalId);
  let batch = 0;
  const generator = vi.fn(
    async (_endpoint: unknown, messages: { content: unknown }[]) => {
      const image = Array.isArray(messages[1].content);
      const source = image
        ? JSON.parse((messages[1].content as { text: string }[])[0].text)
        : JSON.parse(messages[1].content as string);
      const citations = image
        ? [{ attachmentId: source.pages[0].attachmentId, page: 1 }]
        : source.extractions[0].fields[0].citations;
      return {
        result: {
          summary: "Review the conflicting merchants.",
          fields: [
            {
              name: "merchant",
              value: image ? `Supplier ${++batch}` : "Supplier 1",
              citations,
            },
          ],
          missing: [],
          conflicts: [],
        },
        inputTokens: 10,
        outputTokens: 5,
      };
    },
  );
  await Promise.all([
    runExpenseAi(
      db,
      env.H001_FILES,
      aiEnv,
      String(queued.proposalId),
      generator,
    ),
    runExpenseAi(
      db,
      env.H001_FILES,
      aiEnv,
      String(queued.proposalId),
      generator,
    ),
  ]);
  expect(generator).toHaveBeenCalledTimes(3);
  const result = (await expenseAiStatus(db, f.claimant, aiEnv, id, null))
    .proposal?.result;
  expect(result?.fields[0].value).toBeNull();
  expect(result?.conflicts.join(" ")).toContain("Conflicting evidence");
  const usage = (
    await db
      .prepare(
        "SELECT model,input_tokens,output_tokens FROM ai_provider_usage WHERE company_id=? AND feature='expense' ORDER BY model",
      )
      .bind(f.company)
      .all()
  ).results;
  expect(usage).toEqual([
    { model: "claude-sonnet-4-6", input_tokens: 20, output_tokens: 10 },
    { model: "gpt-5.6-luna", input_tokens: 10, output_tokens: 5 },
  ]);
});

it("resumes an expired lease and does not overwrite a newer worker result", async () => {
  await connection();
  const id = await f.claim(),
    attachment = await f.attach(id),
    queued = await queueExpenseAi(
      db,
      f.claimant,
      aiEnv,
      id,
      { expectedVersion: 1, paymentId: null, retry: false },
      key(),
    );
  await db
    .prepare(
      "UPDATE expense_ai_proposals SET state='running',claim_until=1,lease_token='expired' WHERE id=?",
    )
    .bind(queued.proposalId)
    .run();
  const result = {
    summary: "Receipt reviewed",
    fields: [
      {
        name: "merchant",
        value: "Receipt supplier",
        citations: [{ attachmentId: attachment, page: 1 }],
      },
    ],
    missing: [],
    conflicts: [],
  };
  await runExpenseAi(
    db,
    env.H001_FILES,
    aiEnv,
    String(queued.proposalId),
    async () => {
      // Simulate the old worker finishing after a replacement already committed.
      await db.batch([
        db
          .prepare(
            "UPDATE expense_ai_proposals SET state='ready',result_json=?,lease_token=NULL WHERE id=?",
          )
          .bind(JSON.stringify(result), queued.proposalId),
        db
          .prepare(
            "UPDATE expense_ai_usage SET state='succeeded',input_tokens=88 WHERE id=?",
          )
          .bind(queued.proposalId),
      ]);
      return { result, inputTokens: 5, outputTokens: 2 };
    },
  );
  expect(
    (await expenseAiStatus(db, f.claimant, aiEnv, id, null)).proposal?.state,
  ).toBe("ready");
  expect(
    await db
      .prepare("SELECT state,input_tokens FROM expense_ai_usage WHERE id=?")
      .bind(queued.proposalId)
      .first(),
  ).toEqual({ state: "succeeded", input_tokens: 88 });
});

it("enforces company quota without losing an editable claim", async () => {
  await connection();
  const id = await f.claim();
  await f.attach(id);
  await db
    .prepare(
      "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,20,5,0,1073741824,1)",
    )
    .bind(f.company)
    .run();
  await expect(
    queueExpenseAi(
      db,
      f.claimant,
      aiEnv,
      id,
      { expectedVersion: 1, paymentId: null, retry: false },
      key(),
    ),
  ).rejects.toMatchObject({ code: "AI_QUOTA_EXCEEDED" });
  expect((await getExpense(f.claimant, id)).merchant).toBe("Supplier");
  expect((await getExpense(f.claimant, id)).status).toBe("draft");
  expect(
    await db
      .prepare("SELECT COUNT(*) n FROM expense_ai_usage WHERE company_id=?")
      .bind(f.company)
      .first("n"),
  ).toBe(0);
});

function mockCatalog(data: unknown = { data: [{ id: "minimax-m3" }, { id: "minimax-m3" }, { id: "gpt-5.6-luna" }, { id: "new-model" }, { id: "gemini-3.8-flash" }] }) {
  vi.stubGlobal("fetch", vi.fn(async () => Response.json({ Status: 0, Answer: [{ type: 1, data: "8.8.8.8" }] })));
  return vi.spyOn(nativeHttps, "pinnedNodeRequest").mockResolvedValue(Response.json(data));
}
it("discovers unsaved models without creating a revision and uses the current key", async () => {
  const request = mockCatalog();
  const result = await discoverAiModelsInput(db, f.it, aiEnv, aiDiscoverySchema.parse({ baseUrl: "https://opencode.ai/zen/go/v1", apiKey: "fresh-key" }));
  expect(result.models).toEqual(["gpt-5.6-luna", "minimax-m3"]);
  expect(result.totalModels).toBe(4);
  expect(result.fetchedAt).toBeTruthy();
  expect(request.mock.calls[0][2]).toMatchObject({ Authorization: "Bearer fresh-key" });
  expect((await aiProviderOverview(db, f.it, aiEnv)).connections).toEqual([]);
});
it("keeps saved credentials within the original URL and company during discovery and save", async () => {
  const id = await connection();
  const request = mockCatalog();
  const input = aiDiscoverySchema.parse({ connectionId: id, baseUrl: "https://opencode.ai/zen/v1" });
  await discoverAiModelsInput(db, f.it, aiEnv, input);
  expect(request.mock.calls[0][2]).toMatchObject({ Authorization: "Bearer private-test-key" });
  request.mockResolvedValue(Response.json({ data: [] }));
  await discoverAiModelsInput(db, f.it, aiEnv, { ...input, apiKey: "new-key" });
  expect(request.mock.calls[1][2]).toMatchObject({ Authorization: "Bearer new-key" });
  await expect(discoverAiModelsInput(db, f.it, aiEnv, { ...input, baseUrl: "https://other.provider.com/v1" })).rejects.toMatchObject({ code: "AI_KEY_REQUIRED" });
  await expect(discoverAiModelsInput(db, f.claimant, aiEnv, input)).rejects.toMatchObject({ status: 403 });
  await expect(discoverAiModelsInput(db, f.it, aiEnv, { ...input, connectionId: "another-company-connection" })).rejects.toMatchObject({ status: 404 });
  await expect(saveAiConnection(db, f.it, aiEnv, id, aiConnectionSchema.parse({ expectedVersion: 3, name: "Changed endpoint", baseUrl: "https://other.provider.com/v1", textModel: "text" }), key())).rejects.toMatchObject({ code: "AI_KEY_REQUIRED" });
  expect(request).toHaveBeenCalledTimes(2);
});
it("reports catalog failures and supports public catalogs without a key", async () => {
  const input = aiDiscoverySchema.parse({ baseUrl: "https://opencode.ai/zen/v1" });
  const request = mockCatalog({ data: [{ id: 123 }] });
  await expect(discoverAiModelsInput(db, f.it, aiEnv, input)).rejects.toMatchObject({ code: "AI_INVALID_RESPONSE" });
  request.mockResolvedValueOnce(Response.json({ data: [] }));
  expect((await discoverAiModelsInput(db, f.it, aiEnv, input)).models).toEqual([]);
  request.mockRejectedValueOnce(Error("timeout private-key"));
  await expect(discoverAiModelsInput(db, f.it, aiEnv, input)).rejects.toMatchObject({ code: "AI_CONNECTION_FAILED" });
  request.mockResolvedValueOnce(new Response(null, { status: 401 }));
  await expect(discoverAiModelsInput(db, f.it, aiEnv, input)).rejects.toMatchObject({ code: "AI_PROVIDER_ERROR" });
});
it("persists separate protocols in draft and active revisions without changing company defaults", async () => {
  const saved = await saveAiConnection(db, f.it, aiEnv, null, aiConnectionSchema.parse({ name: "OpenCode Zen", providerPreset: "opencode-zen", baseUrl: "https://opencode.ai/zen/v1", apiKey: "test", textModel: "gpt-5.6-luna", textProtocol: "messages", imageModel: "claude-sonnet-4-6", imageProtocol: "responses" }), key());
  let view = await aiProviderOverview(db, f.it, aiEnv);
  expect(view.selection.mode).toBe("deployment");
  expect(view.connections[0]).toMatchObject({ provider_preset: "opencode-zen", text_protocol: "responses", image_protocol: "messages" });
  await testAiConnection(db, f.it, aiEnv, saved.id, 1, key(), synthetic);
  expect(synthetic.mock.calls[0][0]).toMatchObject({ protocol: "responses" });
  expect(synthetic.mock.calls[1][0]).toMatchObject({ protocol: "messages" });
  expect(synthetic.mock.calls[0][0]).toHaveProperty("sessionId");
  expect((synthetic.mock.calls[0][0] as { sessionId: string }).sessionId).toBe((synthetic.mock.calls[1][0] as { sessionId: string }).sessionId);
  await activateAiConnection(db, f.it, saved.id, 2, key());
  view = await aiProviderOverview(db, f.it, aiEnv);
  expect(view.connections[0].active).toMatchObject({ text_protocol: "responses", image_protocol: "messages" });
  expect(view.selection.mode).toBe("deployment");
});
it("accepts omitted protocols and ignores caller protocol overrides", async () => {
  const input = aiConnectionSchema.parse({ name: "Automatic", baseUrl: "https://opencode.ai/zen/go/v1", textModel: "minimax-m3", textProtocol: "responses" });
  expect(input).not.toHaveProperty("textProtocol");
  await saveAiConnection(db, f.it, aiEnv, null, input, key());
  const view = await aiProviderOverview(db, f.it, aiEnv);
  expect(view.connections[0].text_protocol).toBe("messages");
});
it.each([
  ["https://opencode.ai/zen/v1", "future-model", ""],
  ["https://opencode.ai/zen/v1", "gemini-3.8-flash", ""],
  ["https://opencode.ai/zen/v1", "gpt-5.6-luna", "unknown-image"],
  ["https://opencode.ai/zen/v1", "toString", ""],
])("rejects unsupported endpoint/model pairs %s %s %s", async (baseUrl, textModel, imageModel) => {
  await expect(saveAiConnection(db, f.it, aiEnv, null, aiConnectionSchema.parse({ name: "Unsupported", baseUrl, textModel, imageModel, textProtocol: "responses", imageProtocol: "messages" }), key())).rejects.toMatchObject({ code: "AI_MODEL_UNSUPPORTED" });
  expect((await aiProviderOverview(db, f.it, aiEnv)).connections).toEqual([]);
});
it("preserves active custom revisions when a new custom draft is saved", async () => {
  const id = await connection();
  await db.prepare("UPDATE ai_connection_revisions SET base_url=?,text_model=? WHERE connection_id=?").bind("https://legacy.provider.com/v1", "legacy-model", id).run();
  expect((await resolveAiProvider(db, f.company, aiEnv, "text")).endpoint).toMatchObject({ baseUrl: "https://legacy.provider.com/v1", model: "legacy-model" });
  await saveAiConnection(db, f.it, aiEnv, id, aiConnectionSchema.parse({ expectedVersion: 3, name: "Legacy edit", baseUrl: "https://legacy.provider.com/v1", textModel: "legacy-model" }), key());
  expect((await aiProviderOverview(db, f.it, aiEnv)).connections[0].active_revision).toBe(1);
});

it("activates a verified text model while keeping failed image reading unavailable", async () => {
  const saved = await saveAiConnection(db, f.it, aiEnv, null, aiConnectionSchema.parse({ name: "Image check", apiKey: "synthetic-test-key", baseUrl: "https://opencode.ai/zen/v1", textModel: "gpt-5.6-luna", imageModel: "claude-sonnet-4-6" }), key());
  const result = await testAiConnection(db, f.it, aiEnv, saved.id, 1, key(), async (_endpoint, messages) => ({ result: Array.isArray(messages[1].content) ? { color: "blue" } : { ok: true }, inputTokens: 0, outputTokens: 0 }));
  expect(result).toMatchObject({ textVerified: true, imageVerified: false });
  await activateAiConnection(db, f.it, saved.id, 2, key());
  await selectAiProviders(db, f.it, aiSelectionSchema.parse({ expectedVersion: 0, mode: "custom", textConnectionId: saved.id }), key());
  expect((await resolveAiProvider(db, f.company, aiEnv, "text")).endpoint.model).toBe("gpt-5.6-luna");
  await expect(resolveAiProvider(db, f.company, aiEnv, "image")).rejects.toMatchObject({ code: "AI_CAPABILITY_UNAVAILABLE" });
});

it("filters unsafe receipt fields, preserves a Gregorian date and rejects a category in another currency", async () => {
  await connection();
  const id = await f.claim(), attachmentId = await f.attach(id);
  const queued = await queueExpenseAi(db, f.claimant, aiEnv, id, { expectedVersion: 1, paymentId: null, retry: false }, key());
  await runExpenseAi(db, env.H001_FILES, aiEnv, String(queued.proposalId), async () => ({
    result: { summary: "Synthetic bilingual receipt", fields: [
      { name: "expenseDate", value: "2026-09-09" },
      { name: "merchant", value: "ร้านทดสอบ / Test Shop" },
      { name: "receiptReference", value: "x".repeat(101) },
      { name: "currency", value: "EUR" },
      { name: "amount", value: "999999999999999999999999999999" },
      { name: "tax", value: "-7.00" },
      { name: "businessPurpose", value: "Invented customer visit" },
      { name: "categoryAccountId", value: f.accounts.expense },
    ].map(field => ({ ...field, citations: [{ attachmentId, page: 1 }] })), missing: [], conflicts: [] },
    inputTokens: 1, outputTokens: 1,
  }));
  const result = (await expenseAiStatus(db, f.claimant, aiEnv, id, null)).proposal?.result;
  expect(result?.fields.find(field => field.name === "expenseDate")?.value).toBe("2026-09-09");
  expect(result?.fields.find(field => field.name === "merchant")?.value).toBe("ร้านทดสอบ / Test Shop");
  for (const name of ["amount", "tax", "receiptReference", "categoryAccountId", "businessPurpose"]) expect(result?.fields.find(field => field.name === name)?.value).toBeNull();
  expect(result?.conflicts.join(" ")).toContain("currency");
});
it("does not queue or run expense AI when People is outside the production rollout", async () => {
  await connection();
  const id = await f.claim();
  await f.attach(id);
  const input = { expectedVersion: 1, paymentId: null, retry: false };
  vi.stubEnv("NODE_ENV", "production");
  try {
    await expect(queueExpenseAi(db, f.claimant, aiEnv, id, input, key())).rejects.toMatchObject({ code: "FEATURE_COMING_SOON" });
    vi.stubEnv("NODE_ENV", "test");
    const queued = await queueExpenseAi(db, f.claimant, aiEnv, id, input, key());
    vi.stubEnv("NODE_ENV", "production");
    const generate = vi.fn();
    await runExpenseAi(db, env.H001_FILES, aiEnv, String(queued.proposalId), generate);
    expect(generate).not.toHaveBeenCalled();
    expect((await db.prepare("SELECT state FROM expense_ai_proposals WHERE id=?").bind(queued.proposalId).first<{ state: string }>())?.state).toBe("stale");
  } finally {
    vi.unstubAllEnvs();
  }
});
it("accepts an exact color with harmless case and whitespace normalization", async () => {
  const saved = await saveAiConnection(db, f.it, aiEnv, null, aiConnectionSchema.parse({ name: "Same model", apiKey: "synthetic-test-key", baseUrl: "https://opencode.ai/zen/v1", textModel: "muse-spark-1.3-contributor-free", imageModel: "muse-spark-1.3-contributor-free" }), key());
  const result = await testAiConnection(db, f.it, aiEnv, saved.id, 1, key(), async (_endpoint, messages) => ({ result: Array.isArray(messages[1].content) ? { color: " Red " } : { ok: true }, inputTokens: 0, outputTokens: 0 }));
  expect(result).toMatchObject({ textVerified: true, imageVerified: true });
});

it.each([[0, "54.50"], [385, "50.65"]])("derives draft net from cited receipt total with %s recorded recoverable tax", async (taxMinor, expected) => {
  await connection();
  const id = await f.claim(), attachmentId = await f.attach(id);
  await db.prepare("UPDATE finance_expenses SET tax_minor=? WHERE id=?").bind(taxMinor, id).run();
  const queued = await queueExpenseAi(db, f.claimant, aiEnv, id, { expectedVersion: 1, paymentId: null, retry: false }, key());
  await runExpenseAi(db, env.H001_FILES, aiEnv, String(queued.proposalId), async () => ({
    result: { summary: "Receipt total CHF 54.50 includes VAT CHF 3.85; printed tax is not proof of recoverability.", fields: [
      { name: "receiptTotal", value: "54.50" }, { name: "amount", value: null }, { name: "tax", value: "3.85" }, { name: "currency", value: "CHF" },
    ].map(field => ({ ...field, citations: [{ attachmentId, page: 1 }] })), missing: ["amount", "tax"], conflicts: [] }, inputTokens: 1, outputTokens: 1,
  }));
  const result = (await expenseAiStatus(db, f.claimant, aiEnv, id, null)).proposal?.result;
  expect(result?.fields.find(f => f.name === "amount")).toMatchObject({ value: expected, citations: [{ attachmentId, page: 1 }] });
  expect(result?.fields.find(f => f.name === "tax")?.value).toBeNull();
  expect((await getExpense(f.claimant, id)).tax_minor).toBe(taxMinor);
});

it("re-extracts ready suggestions only on explicit retry and still deduplicates queued work", async () => {
  await connection();
  const id = await f.claim(), attachmentId = await f.attach(id);
  const input = { expectedVersion: 1, paymentId: null, retry: false };
  const first = await queueExpenseAi(db, f.claimant, aiEnv, id, input, key());
  await runExpenseAi(db, env.H001_FILES, aiEnv, String(first.proposalId), async () => ({ result: { summary: "Ready", fields: [{ name: "receiptTotal", value: "54.50", citations: [{ attachmentId, page: 1 }] }], missing: [], conflicts: [] }, inputTokens: 1, outputTokens: 1 }));
  expect((await queueExpenseAi(db, f.claimant, aiEnv, id, input, key())).proposalId).toBe(first.proposalId);
  const again = await queueExpenseAi(db, f.claimant, aiEnv, id, { ...input, retry: true }, key());
  expect(again.proposalId).not.toBe(first.proposalId);
  expect((await queueExpenseAi(db, f.claimant, aiEnv, id, { ...input, retry: true }, key())).proposalId).toBe(again.proposalId);
});
