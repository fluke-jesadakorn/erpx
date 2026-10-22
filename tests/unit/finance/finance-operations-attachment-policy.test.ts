import { describe, expect, it } from "vitest";
import { attachmentPolicy } from "@erpx/server-files/public/attachments/policy";
import { assertSanitizedEvidenceMetadata, financeOperationsAttachmentPolicies } from "@erpx/server-files/public/attachments/finance-operations-policy";

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
});
