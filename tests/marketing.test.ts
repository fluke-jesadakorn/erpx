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
const handoffFailure=vi.hoisted(()=>({enabled:false}));
vi.mock("../src/server/modules/crm/public/lead-handoff",async(importOriginal)=>{
  const actual=await importOriginal<typeof import("../src/server/modules/crm/public/lead-handoff")>();
  return {createCRMLeadInTransaction:async(...args:Parameters<typeof actual.createCRMLeadInTransaction>)=>{
    const result=await actual.createCRMLeadInTransaction(...args);
    if(handoffFailure.enabled)await args[0].exec("SELECT 1/0");
    return result;
  }};
});
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
it("requires explicit consent for lead-capture forms", async () => {
  await expect(
    createMarketingAsset(
      creator,
      {
        kind: "form",
        name: "Inquiry without consent",
        configuration: { fields: ["name", "email"] },
        preview: { headline: "Contact us" },
      },
      key(),
    ),
  ).rejects.toMatchObject({
    status: 422,
    code: "MARKETING_CONSENT_REQUIRED",
  });
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
  const engagementInput = {
      action: "record-engagement" as const,
      expectedVersion: 1,
      eventType: "email_open",
      channel: "email",
      reason: "Provider event SIM-OPEN-1",
    },
    engagementCommand = key(),
    engagementResult = await changeMarketing(
      creator,
      "candidate",
      String(candidate.id),
      engagementInput,
      engagementCommand,
    );
  expect(
    await changeMarketing(
      creator,
      "candidate",
      String(candidate.id),
      engagementInput,
      engagementCommand,
    ),
  ).toEqual(engagementResult);
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
  await createMarketingCandidate(
    creator,
    {
      formId: form.id,
      formVersion: 2,
      branchId: f.branch,
      contactName: "Prospect Two",
      contactDetail: "second-prospect@example.test",
      consentEvidence: "Explicit form consent SIM-CONSENT-2",
      attribution: { campaign: "launch", source: "newsletter" },
      brief: "Needs a follow-up after the first proposal.",
    },
    key(),
  );
  const summary = await marketingWorkspace(reviewer);
  expect(summary.campaignSummary).toEqual([
    {
      campaign: "launch",
      source: "newsletter",
      captured: 2,
      pending: 1,
      handed_off: 1,
      rejected: 0,
    },
  ]);
  expect(JSON.stringify(summary.campaignSummary)).not.toContain("Prospect One");
  expect(
    (await marketingWorkspace({ ...reviewer, departmentId: "unrelated-department" })).campaignSummary,
  ).toEqual([]);
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
it("rolls back CRM, Marketing, touch, notification and command evidence after a late handoff failure",async()=>{
 const form=await activeForm(),candidate=await createMarketingCandidate(creator,{formId:form.id,formVersion:2,branchId:f.branch,contactName:"Atomic handoff",contactDetail:"atomic@example.test",consentEvidence:"Explicit employee approved form consent",attribution:{source:"newsletter"},brief:"Needs an approved proposal"},key()),command=key(),leadId=`lead-${command}-crm`;
 handoffFailure.enabled=true;
 try{await expect(changeMarketing(reviewer,"candidate",String(candidate.id),{action:"handoff",expectedVersion:1,reason:"Independently review consent before handoff",dueDate:"2026-10-10"},command)).rejects.toThrow();}
 finally{handoffFailure.enabled=false;}
 expect(await db.prepare("SELECT status,version,crm_lead_id FROM marketing_lead_candidates WHERE id=?").bind(candidate.id).first()).toEqual({status:"pending",version:1,crm_lead_id:null});
 for(const table of ["crm_leads","crm_accounts","crm_contacts"])expect(await db.prepare(`SELECT count(*) count FROM ${table} WHERE id=?`).bind(table==="crm_leads"?leadId:table==="crm_accounts"?`account-${command}-crm`:`contact-${command}-crm`).first("count")).toBe(0);
 expect(await db.prepare("SELECT count(*) count FROM marketing_touches WHERE lead_id=?").bind(leadId).first("count")).toBe(0);
 expect(await db.prepare("SELECT count(*) count FROM staff_notifications WHERE record_id=?").bind(leadId).first("count")).toBe(0);
 for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) count FROM ${table} WHERE command_id IN (?,?)`).bind(command,`${command}-crm`).first("count")).toBe(0);
});
it("serializes concurrent candidate handoffs and replays the committed response without a second CRM record",async()=>{
 const form=await activeForm(),candidate=await createMarketingCandidate(creator,{formId:form.id,formVersion:2,branchId:f.branch,contactName:"Concurrent handoff",contactDetail:"concurrent@example.test",consentEvidence:"Explicit prospect consent captured",attribution:{source:"newsletter"},brief:"Discuss service proposal"},key()),body={action:"handoff",expectedVersion:1,reason:"Independently review current candidate",dueDate:"2026-10-10"},commands=[key(),key()];
 const outcomes=await Promise.allSettled(commands.map(command=>changeMarketing(reviewer,"candidate",String(candidate.id),body,command)));
 expect(outcomes.filter(outcome=>outcome.status==="fulfilled")).toHaveLength(1);
 const index=outcomes.findIndex(outcome=>outcome.status==="fulfilled"),completed=outcomes[index];
 if(completed.status!=="fulfilled")throw new Error("Expected one committed handoff");
 expect(await changeMarketing(reviewer,"candidate",String(candidate.id),body,commands[index])).toEqual(completed.value);
 expect(await db.prepare("SELECT count(*) count FROM marketing_touches WHERE source_kind='form' AND source_id=?").bind(candidate.id).first("count")).toBe(1);
 expect(await db.prepare("SELECT status,version FROM marketing_lead_candidates WHERE id=?").bind(candidate.id).first()).toEqual({status:"handed_off",version:2});
});
