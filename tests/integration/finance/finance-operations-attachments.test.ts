import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { attachmentBindingState, enableAttachmentV2ForTest, seedAttachmentSet } from "../../support/helpers/attachment-v2";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";
import {
  attachmentPolicy,
  authorizeAttachmentTarget,
} from "../../../src/server/modules/files/public/attachments/policy";
import {
  assertSanitizedEvidenceMetadata,
  authorizeFinanceOperationsAttachmentTarget,
  financeOperationsAttachmentPolicies,
} from "../../../src/server/modules/files/public/attachments/finance-operations-policy";
import {
  attestAttachmentSetSanitized,
  bindRecordAttachmentSet,
} from "../../../src/server/modules/files/public/attachments/service";

let fixture: Awaited<ReturnType<typeof expenseFixture>>;

function withPermissions(actor: WorkContext, permissions: string[]): WorkContext {
  return {
    ...actor,
    iam: {
      roleRevision: 1,
      identityVersion: 1,
      permissions,
      grantablePermissions: [],
      principalId: `test:${actor.userId}`,
      personId: null,
      freshUntil: null,
    },
  };
}

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  enableAttachmentV2ForTest();
});

beforeEach(async () => {
  fixture = await expenseFixture();
});

describe("Finance, hospitality, Organization and system attachment policy", () => {
  it("keeps its owner/purpose/component inventory exact and excludes projections and sensitive material", () => {
    const combinations = financeOperationsAttachmentPolicies.flatMap((policy) =>
      policy.componentKinds.map((kind) => `${policy.targetType}:${policy.purposeKey}:${kind}`),
    );
    expect(new Set(combinations).size).toBe(combinations.length);
    expect(combinations).toEqual(expect.arrayContaining([
      "finance_journal:journal_support:record",
      "finance_period:close_workpaper:action",
      "pos_sale:void_evidence:action",
      "pos_refund:refund_evidence:action",
      "pos_shift:cash_variance:action",
      "pms_guest:identity_document:record",
      "pms_task:room_photo:action",
      "pms_task:damage_photo:action",
      "pms_lost_property:handover_evidence:action",
      "pms_property:ota_reconciliation:action",
      "overtime_request:actual_evidence:action",
      "time_entry:corrected_entry_proof:action",
      "organization_node:branch_permit:record",
      "iam_review:sanitized_review_evidence:action",
      "company_release_acceptance:sanitized_release_evidence:record",
    ]));
    expect(combinations).toContain("finance_group_report:finance_support:record");
    expect(combinations.some((entry) => entry.split(":")[0]!=="finance_group_report" && /ledger|report|today|executive/i.test(entry))).toBe(false);
    for (const purpose of ["password", "private_key", "secret", "config_dump", "database_dump"])
      expect(() => attachmentPolicy("iam_review", purpose, "record")).toThrow(/not registered/i);
  });

  it("rejects dangerous filenames and labels even when their generic MIME is otherwise allowed", () => {
    const policy = attachmentPolicy("iam_review", "sanitized_review_evidence", "record");
    for (const [filename, caption] of [
      ["production.env", "sanitized review"],
      ["id_rsa.pem", "review artifact"],
      ["migration.pdf", "database dump backup"],
      ["config-export.pdf", "configuration dump"],
      ["access-review.pdf", "contains passwords"],
    ] as const)
      expect(() => assertSanitizedEvidenceMetadata(policy, filename, caption)).toThrow(/must never be uploaded/i);
    expect(() => assertSanitizedEvidenceMetadata(policy, "access-review.pdf", "Identifiers redacted; sanitized findings only")).not.toThrow();
  });

  it("fails closed across permission, Company and Branch scope boundaries", async () => {
    const journalId = `journal-${key()}`;
    await db.prepare(`INSERT INTO finance_journals(
      id,company_id,chart_id,period_id,journal_number,entry_date,currency,memo,status,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,'2026-09-27','THB','Attachment authorization fixture','draft',?,?,?)`)
      .bind(journalId, fixture.company, fixture.chart, fixture.period, `ATT-${key()}`, fixture.owner.userId, Date.now(), Date.now()).run();
    await expect(authorizeAttachmentTarget(
      db,
      withPermissions(fixture.owner, []),
      attachmentPolicy("finance_journal", "journal_support", "record"),
      journalId,
      "write",
    )).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_FORBIDDEN" });
    await expect(authorizeAttachmentTarget(
      db,
      { ...fixture.owner, companyId: `foreign-${key()}` },
      attachmentPolicy("finance_journal", "journal_support", "record"),
      journalId,
      "read",
    )).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });

    const scopedActor = withPermissions({ ...fixture.owner, branchId: fixture.branch, assignmentScope: "branch", scope: { kind: "branch", holdingId: fixture.owner.holdingId, holdingSlug: fixture.owner.holdingSlug, holdingName: fixture.owner.holdingName, companyId: fixture.company, branchId: fixture.branch, branchSlug: "finance-fixture", branchName: "Finance fixture" } }, ["pos.read", "pos.sell"]);
    const fakeDb = {
      prepare: () => ({
        bind: () => ({
          first: async () => ({ company_id: fixture.company, branch_id: `other-${key()}`, department_id: null, version: 1 }),
        }),
      }),
    } as unknown as SqlDatabase;
    await expect(authorizeFinanceOperationsAttachmentTarget(
      fakeDb,
      scopedActor,
      attachmentPolicy("pos_sale", "void_evidence", "action"),
      "sale-1",
      "write",
    )).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_SCOPE_FORBIDDEN" });
  });

  it("binds a Finance record atomically and rejects a stale record-document revision", async () => {
    const journalId = `journal-${key()}`;
    await db.prepare(`INSERT INTO finance_journals(
      id,company_id,chart_id,period_id,journal_number,entry_date,currency,memo,status,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,'2026-09-27','THB','Attachment binding fixture','draft',?,?,?)`)
      .bind(journalId, fixture.company, fixture.chart, fixture.period, `ATT-${key()}`, fixture.owner.userId, Date.now(), Date.now()).run();
    const first = await seedAttachmentSet(db, fixture.owner, {
      targetType: "finance_journal",
      targetId: journalId,
      componentKind: "record",
      purposeKey: "journal_support",
      expectedTargetVersion: 1,
    });
    await bindRecordAttachmentSet(db, fixture.owner, first.setId, 1, key());
    expect(await attachmentBindingState(db, first.setId)).toMatchObject({
      set_state: "bound",
      link_state: "bound",
      binding_type: "record_document",
      binding_id: journalId,
    });
    expect(await db.prepare("SELECT version FROM finance_journals WHERE id=?").bind(journalId).first("version")).toBe(2);

    const stale = await seedAttachmentSet(db, fixture.owner, {
      targetType: "finance_journal",
      targetId: journalId,
      componentKind: "record",
      purposeKey: "journal_support",
      expectedTargetVersion: 1,
    });
    await expect(bindRecordAttachmentSet(db, fixture.owner, stale.setId, 1, key()))
      .rejects.toMatchObject({ status: 409, code: "ATTACHMENT_TARGET_CHANGED" });
    expect(await attachmentBindingState(db, stale.setId)).toMatchObject({ set_state: "draft", link_state: "draft" });
  });

  it("requires a server-recorded sanitization review before immutable IAM evidence can bind", async () => {
    const reviewId = `iam-review-${key()}`;
    await db.prepare(`INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,version,reason)
      VALUES(?,?,'access_review','{}'::jsonb,?,?,?,'pending',1,'Attachment sanitization fixture')`)
      .bind(reviewId, fixture.company, fixture.owner.userId, fixture.owner.userId, Date.now()).run();
    const files = await seedAttachmentSet(db, fixture.owner, {
      targetType: "iam_review",
      targetId: reviewId,
      componentKind: "record",
      purposeKey: "sanitized_review_evidence",
      expectedTargetVersion: 1,
    });
    await expect(bindRecordAttachmentSet(db, fixture.owner, files.setId, 1, key()))
      .rejects.toMatchObject({ status: 422, code: "ATTACHMENT_SANITIZATION_ATTESTATION_REQUIRED" });
    const attestation={
      confirmed: true,
      note: "Reviewed final artifact; credentials and system exports were removed.",
    } as const,attestationKey=key();
    const attested=await attestAttachmentSetSanitized(db, fixture.owner, files.setId, attestation, attestationKey);
    await expect(attestAttachmentSetSanitized(db, fixture.owner, files.setId, attestation, attestationKey)).resolves.toEqual(attested);
    await expect(attestAttachmentSetSanitized(db, fixture.owner, files.setId, {...attestation,note:"Reviewed final artifact; a different sanitization statement was recorded."}, attestationKey))
      .rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});

    const secondReviewId=`iam-review-${key()}`;
    await db.prepare(`INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,version,reason)
      VALUES(?,?,'access_review','{}'::jsonb,?,?,?,'pending',1,'Second attachment sanitization fixture')`)
      .bind(secondReviewId,fixture.company,fixture.owner.userId,fixture.owner.userId,Date.now()).run();
    const secondFiles=await seedAttachmentSet(db,fixture.owner,{targetType:"iam_review",targetId:secondReviewId,componentKind:"record",purposeKey:"sanitized_review_evidence",expectedTargetVersion:1});
    await expect(attestAttachmentSetSanitized(db,fixture.owner,secondFiles.setId,attestation,attestationKey))
      .rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});
    await bindRecordAttachmentSet(db, fixture.owner, files.setId, 1, key());
    expect(await attachmentBindingState(db, files.setId)).toMatchObject({ set_state: "bound", link_state: "bound" });
    expect(await db.prepare("SELECT version FROM iam_reviews WHERE id=?").bind(reviewId).first("version")).toBe(2);
  });
});
