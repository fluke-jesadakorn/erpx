import * as nativeHttps from "@erpx/server-runtime/adapters/runtime/https";
import { env } from "../../support/runtime-env";
import { applyMigrations, type TestMigration } from "../../support/runtime-env";
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  aiSnapshotGuard,
  aiProviderOverview,
  resolveAiProvider,
} from "@erpx/server-agent/public/ai-providers";
import {
  aiBaseUrl,
  generateProviderJson,
  openAiKey,
  publicAddress,
  sealAiKey,
} from "@erpx/server-agent/public/ai-transport";
import { seedAiProvider, seedAiSelection } from "./ai-provider-seed";
import { createNativeAgentAiProviders } from "@erpx/feature-platform_administration/adapters/server/native-agent-ai-providers";
import { administrationAccess } from "@erpx/server-runtime/public/administration";
import {
  grantPersonAccess,
  grantSchema,
} from "@erpx/server-identity_access/public/account-access";
import {
  queueExpenseAi,
  runExpenseAi,
  expenseAiStatus,
} from "@erpx/server-people_payroll/public/expense-ai";
import { saveDocumentPage } from "@erpx/server-files/public/document-derivatives";
import {getExpense} from '@erpx/server-people_payroll/public/expense-read';
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
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});
beforeEach(async () => {
  f = await expenseFixture();
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
    await seedAiSelection(db, f.it, { mode: "deployment" });
    expect((await resolveAiProvider(db, f.company, deployment, "text")).endpoint.model).toBe("fictional-review-model");
    await expect(resolveAiProvider(db, f.company, deployment, "image")).rejects.toMatchObject({ code: "AI_IMAGE_TEST_REQUIRED" });
    await seedAiSelection(db, f.it, { mode: "disabled", version: 2 });
    await expect(resolveAiProvider(db, f.company, deployment, "text")).rejects.toMatchObject({ code: "AI_PILOT_OPT_IN_REQUIRED" });
  } finally {
    vi.unstubAllEnvs();
  }
});
async function connection() {
  return (
    await seedAiProvider(db, aiEnv, f.it, {
      name: "Custom provider",
      baseUrl: "https://opencode.ai/zen/v1",
      apiKey: "private-test-key",
      textModel: "gpt-5.6-luna",
      imageModel: "claude-sonnet-4-6",
    })
  ).id;
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
    aiProviderOverview(db, f.claimant, aiEnv),
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
it("encrypts credentials with identity context and returns only masked status", async () => {
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
  expect(
    (await resolveAiProvider(db, f.company, aiEnv, "text")).endpoint.model,
  ).toBe("gpt-5.6-luna");
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

async function expenseAiPurchaseGrant(state:'active'|'draining'='active'){
 const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants');
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-expense-ai-purchase-provider'}),obligations:async()=>{throw Error('Concrete owner withdrawal is required');}});
 for(const packageId of ['people','people-expenses']){
  const current=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId===packageId);if(packageId==='people'&&current)continue;
  await store.apply({expectedVersion:current?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state:packageId==='people'?'active':state,startsAt:0,endsAt:null,version:(current?.version??0)+1,reason:'Original Expense queued work purchase fixture',evidence:'test:expense-ai-boundary'}});
 }
}
it('completes actually queued Expense suggestions while draining with the captured native author and unchanged financial source',async()=>{
 await connection();await expenseAiPurchaseGrant();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{
  const id=await f.claim(),attachment=await f.attach(id),input={expectedVersion:1,paymentId:null,retry:false},command=key();
  const created=await queueExpenseAi(db,f.claimant,aiEnv,id,input,command);expect(await queueExpenseAi(db,f.claimant,aiEnv,id,input,command)).toEqual(created);
  const original=await db.prepare('SELECT to_jsonb(e)::text image FROM finance_expenses e WHERE id=?').bind(id).first<string>('image'),job=await db.prepare('SELECT actor_json FROM expense_ai_proposals WHERE id=?').bind(created.proposalId).first<{actor_json:string}>();
  expect(JSON.parse(job!.actor_json)).toMatchObject({userId:f.claimant.userId,assignmentId:f.claimant.assignmentId,companyId:f.company,iam:{queuedAssignmentVersion:expect.any(Number)}});
  await expenseAiPurchaseGrant('draining');
  await expect(queueExpenseAi(db,f.claimant,aiEnv,id,{...input,retry:true},key())).rejects.toMatchObject({status:403,code:'PACKAGE_DRAINING'});
  const generate=vi.fn(async()=>({result:{summary:'Original evidence read without a business transition',fields:[{name:'merchant',value:'Suggested merchant',citations:[{attachmentId:attachment,page:1}]}],missing:[],conflicts:[]},inputTokens:2,outputTokens:1}));
  await runExpenseAi(db,env.H001_FILES,aiEnv,String(created.proposalId),generate);expect(generate).toHaveBeenCalledTimes(1);
  expect((await expenseAiStatus(db,f.claimant,aiEnv,id,null)).proposal?.state).toBe('ready');expect((await expenseAiStatus(db,f.claimant,aiEnv,id,null)).allowed).toBe(false);
  expect(await db.prepare('SELECT to_jsonb(e)::text image FROM finance_expenses e WHERE id=?').bind(id).first<string>('image')).toBe(original);
  expect(await db.prepare('SELECT state,lease_until,input_tokens,output_tokens FROM expense_ai_usage WHERE id=?').bind(created.proposalId).first()).toMatchObject({state:'succeeded',lease_until:0,input_tokens:2,output_tokens:1});
  expect(await queueExpenseAi(db,f.claimant,aiEnv,id,input,command)).toEqual(created);
 }finally{vi.unstubAllEnvs();}
});
it('clears a revoked original Expense queued author through native failure bookkeeping without invoking a provider',async()=>{
 await connection();await expenseAiPurchaseGrant();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{
  const id=await f.claim();await f.attach(id);const created=await queueExpenseAi(db,f.claimant,aiEnv,id,{expectedVersion:1,paymentId:null,retry:false},key());
  const original=await db.prepare('SELECT to_jsonb(e)::text image FROM finance_expenses e WHERE id=?').bind(id).first<string>('image');
  // Adverse native authority fact, never a manufactured positive grant.
  await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=? AND company_id=?').bind(Date.now(),f.claimant.assignmentId,f.company).run();
  const generate=vi.fn(async()=>{throw Error('Revoked original author reached a provider');});await runExpenseAi(db,env.H001_FILES,aiEnv,String(created.proposalId),generate);expect(generate).not.toHaveBeenCalled();
  expect(await db.prepare('SELECT state,result_json,claim_until,lease_token,attempts FROM expense_ai_proposals WHERE id=?').bind(created.proposalId).first()).toMatchObject({state:'stale',result_json:null,claim_until:null,lease_token:null,attempts:1});
  expect(await db.prepare('SELECT state,lease_until FROM expense_ai_usage WHERE id=?').bind(created.proposalId).first()).toEqual({state:'failed',lease_until:0});
  expect(await db.prepare('SELECT to_jsonb(e)::text image FROM finance_expenses e WHERE id=?').bind(id).first<string>('image')).toBe(original);
 }finally{vi.unstubAllEnvs();}
});


it('keeps queued payment AI as a real duty after independent unused-preparation cancellation until the original worker closes it without provider IO',async()=>{
 const {expenseDetail,postExpense,reverseExpense}=await import('@erpx/server-people_payroll/public/expenses'),{createExpensePayment,cancelExpensePayment}=await import('@erpx/server-people_payroll/public/expense-payments'),{withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{packageGrantStore,readCompanyPackageGrants,packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants');
 await connection();await expenseAiPurchaseGrant();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ATTACHMENTS_V2','false');
 const scope={companyId:f.company,packageId:'people-expenses',packageVersion:1},observe=()=>withOwnerPackageObligations(db,scope,tx=>readOwnerPackageObligations(db,tx,{...scope,now:Date.now()}));
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-expense-payment-ai-closure-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});
 const withdrawal=async()=>{const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==='people-expenses')!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual cancelled payment source and queued provider duty have settled',evidence:'test:expense-payment-ai-closure'}}]};};
 try{
  const id=await f.claim();await f.throughAccounts(id);await f.action(f.manager,id,'approve');await postExpense(f.manager,id,{expectedVersion:Number((await getExpense(f.manager,id)).version),periodId:f.period,entryDate:'2026-09-02'},key());
  const posted=await getExpense(f.payer,id),draftId=String((await createExpensePayment(f.payer,id,Number(posted.version),key())).draftId);await f.attach(draftId,'expense_payment');
  const queued=await queueExpenseAi(db,f.payer,aiEnv,id,{expectedVersion:1,paymentId:draftId,retry:false},key());expect(queued.state).toBe('queued');expect(await db.prepare('SELECT state FROM expense_ai_usage WHERE id=?').bind(queued.proposalId).first<string>('state')).toBe('reserved');
  await reverseExpense(f.manager,id,{expectedVersion:Number((await getExpense(f.manager,id)).version),periodId:f.period,entryDate:'2026-09-03',reason:'Original unpaid claim is no longer owed'},key());
  await expenseAiPurchaseGrant('draining');const cancelled=await cancelExpensePayment(f.payer,id,draftId,{expectedVersion:1,expenseVersion:Number((await getExpense(f.payer,id)).version),reason:'Independent Treasury verified the preparation was not paid',paymentNotExecuted:true},key());expect(cancelled).toMatchObject({id:draftId,status:'cancelled',version:2});
  expect((await expenseDetail(f.payer,id)).paymentDrafts.find(row=>row.id===draftId)).toMatchObject({status:'cancelled'});expect((await observe()).observations.find(row=>row.moduleId==='people_payroll')!.outstanding).toBeGreaterThan(0);
  const request=await withdrawal();await expect(withOwnerPackageObligations(db,scope,tx=>store.batch.applyIn(tx,request,async()=>{}))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
  const business=()=>db.prepare("SELECT jsonb_build_object('expense',to_jsonb(e),'draft',to_jsonb(d),'payments',(SELECT COALESCE(jsonb_agg(to_jsonb(p) ORDER BY p.id),'[]') FROM finance_payments p WHERE p.company_id=e.company_id),'journals',(SELECT COALESCE(jsonb_agg(to_jsonb(j) ORDER BY j.id),'[]') FROM finance_journals j WHERE j.company_id=e.company_id))::text image FROM finance_expenses e JOIN expense_payment_drafts d ON d.expense_id=e.id AND d.company_id=e.company_id WHERE e.id=? AND e.company_id=?").bind(id,f.company).first<string>('image'),before=await business();
  const generate=vi.fn(async()=>{throw Error('Cancelled payment preparation reached a provider');});await runExpenseAi(db,env.H001_FILES,aiEnv,String(queued.proposalId),generate);expect(generate).not.toHaveBeenCalled();
  expect(await db.prepare('SELECT state,claim_until,lease_token,attempts FROM expense_ai_proposals WHERE id=?').bind(queued.proposalId).first()).toEqual({state:'failed',claim_until:null,lease_token:null,attempts:1});expect(await db.prepare('SELECT state,lease_until,input_tokens,output_tokens FROM expense_ai_usage WHERE id=?').bind(queued.proposalId).first()).toEqual({state:'failed',lease_until:0,input_tokens:0,output_tokens:0});expect(await business()).toBe(before);
  expect((await observe()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);const settled=await withdrawal(),preview=await withOwnerPackageObligations(db,scope,tx=>store.batch.previewIn(tx,settled));expect(await withOwnerPackageObligations(db,scope,tx=>store.batch.applyIn(tx,settled,async()=>{}))).toEqual(preview);
 }finally{vi.unstubAllEnvs();}
});

it("native client forwards the full Administration payload and exact command path", async () => {
  const calls: {
    url: string;
    method: string;
    headers: Record<string, string>;
    body?: string;
  }[] = [];
  const client = createNativeAgentAiProviders(
    JSON.stringify({
      privateBaseUrl: "http://127.0.0.1:44993",
      timeoutMs: 5000,
      holdings: [
        {
          origin: "https://acme.example.test",
          holdingSlug: "acme",
          holdingId: "h001",
        },
      ],
    }),
    async (url, method, headers, body) => {
      calls.push({
        url: url.href,
        method,
        headers: Object.fromEntries(headers),
        body,
      });
      return Response.json({
        data:
          method === "GET"
            ? {
                connections: [],
                selection: { mode: "disabled" },
                deploymentAvailable: false,
                keyStorageReady: false,
                usage: [],
              }
            : { id: "ai-1", version: 2 },
      });
    },
  );
  const headers = new Headers({ host: "acme.example.test" });
  expect((await client.overview(headers, "acme")).selection.mode).toBe(
    "disabled",
  );
  expect(calls[0].url).toBe(
    "http://127.0.0.1:44993/api/v1/holdings/acme/ai-providers",
  );
  expect(calls[0].method).toBe("GET");
  expect(calls[0].headers.origin).toBe("https://acme.example.test");
  await client.command(
    headers,
    "acme",
    "/ai-1/test",
    '{"expectedVersion":1}',
    "provider-key-1",
  );
  expect(calls[1].url).toBe(
    "http://127.0.0.1:44993/api/v1/holdings/acme/ai-providers/ai-1/test",
  );
  expect(calls[1].headers["idempotency-key"]).toBe("provider-key-1");
  expect(calls[1].headers.origin).toBe("https://acme.example.test");
});
