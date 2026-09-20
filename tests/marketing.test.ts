import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  changeMarketing,
  createMarketingAsset,
  createMarketingCandidate,
  marketingDetail,
  marketingWorkspace,
} from "../src/server/marketing";
import { getCRMLead } from "../src/server/crm";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  creator: WorkContext,
  reviewer: WorkContext;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  creator = {
    ...f.owner,
    branchId: f.branch,
    departmentId: f.department,
    modules: ["marketing", "crm"],
  };
  reviewer = {
    ...f.tie,
    branchId: f.branch,
    departmentId: f.department,
    modules: ["marketing", "crm"],
    role: "holding_owner",
    assignmentId: await f.grant(f.tie, "holding_owner"),
  };
});
async function activeForm() {
  const form = await createMarketingAsset(
    creator,
    {
      kind: "form",
      name: "Service inquiry",
      configuration: {
        fields: ["name", "email", "need"],
        consentRequired: true,
      },
      preview: { headline: "Tell us what you need" },
    },
    key(),
  );
  await expect(
    changeMarketing(
      creator,
      "asset",
      String(form.id),
      {
        action: "activate",
        expectedVersion: 1,
        reason: "Self review forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeMarketing(
    reviewer,
    "asset",
    String(form.id),
    {
      action: "activate",
      expectedVersion: 1,
      reason: "Fields, consent and preview reviewed",
    },
    key(),
  );
  return form;
}
it("reviews form/content/domain/provider assets without treating previews as active", async () => {
  const form = await activeForm();
  for (const kind of ["content", "domain", "provider"] as const) {
    const x = await createMarketingAsset(
      creator,
      {
        kind,
        name: `${kind} readiness`,
        configuration: { verified: true, reference: `SIM-${kind}` },
        preview: { summary: `Reviewed ${kind} preview` },
      },
      key(),
    );
    await changeMarketing(
      reviewer,
      "asset",
      String(x.id),
      {
        action: "activate",
        expectedVersion: 1,
        reason: `Independent ${kind} evidence reviewed`,
      },
      key(),
    );
  }
  const w = await marketingWorkspace(reviewer);
  expect(w.assets.filter((x) => x.status === "active")).toHaveLength(4);
  await expect(
    db
      .prepare("UPDATE marketing_assets SET preview_json='{}' WHERE id=?")
      .bind(form.id)
      .run(),
  ).rejects.toThrow("MARKETING_ASSET_SOURCE_IMMUTABLE");
});
it("hands a consented attributed candidate to the CRM owner service exactly once", async () => {
  const form = await activeForm(),
    candidate = await createMarketingCandidate(
      creator,
      {
        formId: form.id,
        formVersion: 2,
        branchId: f.branch,
        contactName: "Prospect One",
        contactDetail: "prospect@example.test",
        consentEvidence: "Explicit form consent SIM-CONSENT-1",
        attribution: { campaign: "launch", utmSource: "newsletter" },
        brief: "Needs a field service proposal next month.",
      },
      key(),
    );
  await changeMarketing(
    creator,
    "candidate",
    String(candidate.id),
    {
      action: "record-engagement",
      expectedVersion: 1,
      eventType: "email_open",
      channel: "email",
      reason: "Provider event SIM-OPEN-1",
    },
    key(),
  );
  await expect(
    changeMarketing(
      creator,
      "candidate",
      String(candidate.id),
      {
        action: "handoff",
        expectedVersion: 2,
        reason: "Self handoff forbidden",
        dueDate: "2026-10-01",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  const command = key(),
    result = await changeMarketing(
      reviewer,
      "candidate",
      String(candidate.id),
      {
        action: "handoff",
        expectedVersion: 2,
        reason: "Consent, attribution and brief reviewed",
        dueDate: "2026-10-01",
      },
      command,
    );
  expect(
    await changeMarketing(
      reviewer,
      "candidate",
      String(candidate.id),
      {
        action: "handoff",
        expectedVersion: 2,
        reason: "Consent, attribution and brief reviewed",
        dueDate: "2026-10-01",
      },
      command,
    ),
  ).toEqual(result);
  const detail = await marketingDetail(reviewer, String(candidate.id)),
    lead = await getCRMLead(reviewer, String(result.crmLeadId));
  expect(detail.candidate).toMatchObject({
    status: "handed_off",
    version: 3,
    crm_lead_id: result.crmLeadId,
  });
  expect(detail.history.map((x) => x.event_type)).toEqual([
    "form_submit",
    "email_open",
    "crm_handoff",
  ]);
  expect(lead).toMatchObject({
    contact_name: "Prospect One",
    status: "open",
    owner_id: reviewer.userId,
  });
  expect(String(lead.source)).toContain("newsletter");
});
it("rejects stale or inactive form sources", async () => {
  const form = await createMarketingAsset(
    creator,
    {
      kind: "form",
      name: "Draft only",
      configuration: { consentRequired: true },
      preview: { headline: "Not published" },
    },
    key(),
  );
  await expect(
    createMarketingCandidate(
      creator,
      {
        formId: form.id,
        formVersion: 1,
        branchId: f.branch,
        contactName: "Blocked Prospect",
        contactDetail: "blocked@example.test",
        consentEvidence: "Consent evidence exists",
        attribution: { campaign: "draft" },
        brief: "Must not enter an inactive form.",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
});
