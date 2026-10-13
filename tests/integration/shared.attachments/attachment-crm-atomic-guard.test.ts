import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";

import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { prepareAttachmentBinding } from "../../../src/server/modules/files/public/attachments/service";
import type { PreparedAttachmentBinding } from "../../../src/server/modules/files/public/attachments/types";
import { applyMigrations, env } from "../../support/runtime-env";

const db = env.H001_DB;
const key = () => crypto.randomUUID();

let companyId: string;
let branchId: string;
let departmentId: string;
let otherBranchId: string;
let otherDepartmentId: string;
let accountId: string;
let leadId: string;
let opportunityId: string;
let engagementId: string;
let actor: WorkContext;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  process.env.ERPX_ATTACHMENTS_V2 = "true";
  delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
});

afterAll(() => {
  delete process.env.ERPX_ATTACHMENTS_V2;
  delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
});

beforeEach(async () => {
  const suffix = key();
  companyId = `attachment-crm-company-${suffix}`;
  branchId = `attachment-crm-branch-${suffix}`;
  departmentId = `attachment-crm-department-${suffix}`;
  otherBranchId = `attachment-crm-other-branch-${suffix}`;
  otherDepartmentId = `attachment-crm-other-department-${suffix}`;
  accountId = `attachment-crm-account-${suffix}`;
  leadId = `attachment-crm-lead-${suffix}`;
  opportunityId = `attachment-crm-opportunity-${suffix}`;
  const userId = `attachment-crm-owner-${suffix}`;
  const assignmentId = `attachment-crm-assignment-${suffix}`;
  const personId = `attachment-crm-person-${suffix}`;
  engagementId = `attachment-crm-engagement-${suffix}`;
  const customerId = `attachment-crm-customer-${suffix}`;
  const now = Date.now();

  actor = {
    userId,
    assignmentId,
    role: "holding_owner",
    companyId,
    branchId,
    departmentId,
    holdingId: "h001",
    holdingSlug: "acme",
    holdingName: "Test",
    name: "CRM attachment owner",
    email: `${userId}@example.test`,
    scope: {
      kind: "holding",
      holdingId: "h001",
      holdingSlug: "acme",
      holdingName: "Test",
      companyId,
    },
    assignmentScope: "holding",
    inherited: false,
    modules: [],
    assignments: [],
    workspaces: [],
  };

  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'CRM attachment company',?)")
      .bind(companyId, companyId, companyId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'CRM attachment branch',?,?,?)")
      .bind(branchId, branchId, companyId, companyId, branchId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'CRM attachment department',?,?,?)")
      .bind(departmentId, departmentId, branchId, companyId, branchId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Other CRM branch',?,?,?)")
      .bind(otherBranchId, otherBranchId, companyId, companyId, otherBranchId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Other CRM department',?,?,?)")
      .bind(otherDepartmentId, otherDepartmentId, otherBranchId, companyId, otherBranchId),
    db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,'[]',?,0,9999999999999,'holding')")
      .bind(assignmentId, userId, actor.role, companyId),
    db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,'CRM attachment owner','TH','active','fixture',?,?)")
      .bind(personId, userId, companyId, now, now),
    db.prepare(`INSERT INTO hr_engagements(
      id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Sales','THB','fixture',?,?)`)
      .bind(engagementId, personId, companyId, branchId, departmentId, now, now),
    db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,'CRM attachment customer','THB','active',?,?)")
      .bind(customerId, companyId, customerId, now, now),
    db.prepare("INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES(?,?,'business','Scoped CRM account','another-user',?,?)")
      .bind(accountId, companyId, now, now),
    db.prepare(`INSERT INTO crm_leads(
      id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,created_by,created_at,updated_at,account_id
    ) VALUES(?,?,?,?,?,'Customer','private@example.test','Direct','Private brief',?,'2026-12-31','qualified',?,?,?,?)`)
      .bind(leadId, companyId, branchId, departmentId, "Atomic attachment opportunity", userId, userId, now, now, accountId),
    db.prepare("INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES(?,?,?,10000,'THB','2026-12-31',?,?,?)")
      .bind(opportunityId, leadId, customerId, userId, now, now),
  ]);
});

async function prepareBinding(targetType: "crm_opportunity" | "crm_account", targetId: string) {
  const token = key();
  const purposeKey = "brief";
  const uploadId = `attachment-crm-upload-${token}`;
  const sourceId = `attachment-crm-source-${token}`;
  const setId = `attachment-crm-set-${token}`;
  const linkId = `attachment-crm-link-${token}`;
  const commandId = `attachment-crm-bind-${token}`;
  const checksum = "a".repeat(64);
  const now = Date.now();

  await db.batch([
    db.prepare(`INSERT INTO upload_intents(
      id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id
    ) VALUES(?,?,?,?,?,'application/pdf',4,4,?,?,?,'ready',?,?,?,?,?)`)
      .bind(uploadId, targetType, targetId, purposeKey, "brief.pdf", checksum, `quarantine/${token}`, `source/${token}`, actor.userId, now + 60_000, now, now, companyId),
    db.prepare(`INSERT INTO attachments(
      id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at
    ) VALUES(?,?,?,?,?,?,1,?,'application/pdf',4,?,?,?,?,'application/pdf','available',?,?)`)
      .bind(sourceId, uploadId, targetType, targetId, purposeKey, `source/${token}`, "brief.pdf", checksum, actor.userId, now, companyId, now, now),
    db.prepare(`INSERT INTO attachment_sets(
      id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,
      policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,?,'record',NULL,?,1,1,'confidential','{}',0,10,'draft',?,?,?)`)
      .bind(setId, companyId, branchId, departmentId, targetType, targetId, purposeKey, actor.userId, now, now),
    db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,?,?)")
      .bind(linkId, setId, sourceId, actor.userId, now, now + 60_000),
  ]);

  const prepared = await prepareAttachmentBinding(db, actor, {
    attachmentSetIds: [setId],
    bindingContract: {
      targetType,
      bindingType: "record_document",
      contexts: [{ purposeKey, componentKind: "record", componentRef: null }],
    },
    targetId,
    expectedTargetVersion: 1,
    commandId,
    bindingId: targetId,
  });
  return { commandId, linkId, prepared, setId };
}

async function expectAtomicGuardRejects(
  prepared: PreparedAttachmentBinding,
  commandId: string,
  setId: string,
  linkId: string,
) {
  await expect(db.batch([
    ...prepared.guards,
    ...prepared.steps,
    db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
  ])).rejects.toMatchObject({ code: "23514" });
  await expect(db.prepare("SELECT state FROM attachment_sets WHERE id=?").bind(setId).first("state")).resolves.toBe("draft");
  await expect(db.prepare("SELECT state FROM attachment_links WHERE id=?").bind(linkId).first("state")).resolves.toBe("draft");
}

describe("CRM attachment binding atomic authorization", () => {
  it.each([
    {
      name: "owner reassignment",
      mutate: () => db.prepare("UPDATE crm_leads SET owner_id=?,version=version+1,updated_at=? WHERE id=?")
        .bind(`replacement-owner-${key()}`, Date.now(), leadId).run(),
    },
    {
      name: "owner eligibility suspension",
      mutate: () => db.prepare("UPDATE hr_engagements SET status='suspended',version=version+1,updated_at=? WHERE id=?")
        .bind(Date.now(), engagementId).run(),
    },
    {
      name: "Branch scope drift",
      mutate: () => db.prepare("UPDATE crm_leads SET branch_id=?,version=version+1,updated_at=? WHERE id=?")
        .bind(otherBranchId, Date.now(), leadId).run(),
    },
    {
      name: "Department scope drift",
      mutate: () => db.prepare("UPDATE crm_leads SET department_id=?,version=version+1,updated_at=? WHERE id=?")
        .bind(otherDepartmentId, Date.now(), leadId).run(),
    },
  ])("rejects $name without relying on the unchanged opportunity version", async ({ mutate }) => {
    const binding = await prepareBinding("crm_opportunity", opportunityId);
    await mutate();
    expect(await db.prepare("SELECT version FROM crm_opportunities WHERE id=?").bind(opportunityId).first("version")).toBe(1);
    await expectAtomicGuardRejects(binding.prepared, binding.commandId, binding.setId, binding.linkId);
  });

  it("rejects CRM account binding after its only scoped lead leaves the actor's Branch", async () => {
    const binding = await prepareBinding("crm_account", accountId);
    await db.prepare("UPDATE crm_leads SET branch_id=?,version=version+1,updated_at=? WHERE id=?")
      .bind(otherBranchId, Date.now(), leadId).run();
    expect(await db.prepare("SELECT version FROM crm_accounts WHERE id=?").bind(accountId).first("version")).toBe(1);
    await expectAtomicGuardRejects(binding.prepared, binding.commandId, binding.setId, binding.linkId);
  });
});
