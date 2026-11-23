import { describe, expect, it, vi } from "vitest";
import { attachmentPolicy } from "@erpx/server-files/public/attachments/policy";
import { assertSanitizedEvidenceMetadata, financeOperationsAttachmentPolicies } from "@erpx/server-files/public/attachments/finance-operations-policy";
import { FinanceAttachmentFinanceFacts } from "@erpx/server-finance/public/attachments-finance-facts";
import { attachmentServiceFinanceFacts } from "@erpx/server-finance/public/attachments-service-facts";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

vi.mock("@erpx/server-workflow/public/command-assertions", () => ({
  assertion: (db: unknown, commandId: string, step: string, query: string, args: (string | number | null)[]) => ({
    __attachmentGuard: true,
    commandId,
    step,
    query,
    args,
  }),
}));

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

describe("finance attachment version writers (retire.finance.attachment-version-writers oracle)", () => {
  const financeTable: Record<string, string> = {
    finance_billing_correction: "finance_billing_corrections",
    finance_business_bill: "finance_business_billing_runs",
    finance_plan_edition: "finance_plan_editions",
    finance_book_rule: "finance_book_rules",
    finance_book_adjustment: "finance_book_adjustments",
    finance_bank_match_group: "finance_bank_match_groups",
    finance_collection_case: "finance_collection_cases",
    finance_business_contract: "finance_business_contracts",
    finance_recognition_run: "finance_recognition_runs",
    finance_instrument: "finance_instrument_editions",
    finance_instrument_run: "finance_instrument_runs",
    finance_asset_policy: "finance_asset_policy_editions",
    finance_asset_policy_run: "finance_asset_policy_runs",
    finance_group_report: "finance_group_report_editions",
    finance_ownership_interest: "finance_ownership_interests",
    finance_acquisition: "finance_acquisitions",
    finance_intercompany_pair: "finance_intercompany_document_pairs",
    finance_journal: "finance_journals",
    finance_bank_statement: "finance_bank_statements",
    finance_bank_transaction: "finance_bank_transactions",
    finance_period: "finance_periods",
    finance_accrual_schedule: "finance_accrual_schedules",
    finance_asset: "finance_assets",
    finance_tax_identity: "finance_tax_identities",
    finance_lease: "finance_leases",
    finance_loan: "finance_loans",
    finance_revenue_contract: "finance_revenue_contracts",
    finance_payment: "finance_payments",
    finance_refund_obligation: "finance_refund_obligations",
    finance_supplier_batch: "finance_supplier_batches",
  };
  const versionColumn: Record<string, string> = { finance_period: "close_version" };

  function stubDb() {
    const prepared: string[] = [];
    const bound: unknown[][] = [];
    const db = {
      prepare: (sql: string) => {
        prepared.push(sql);
        return {
          bind: (...values: unknown[]) => {
            bound.push(values);
            return {
              first: async () => null,
              all: async () => ({ results: [], success: true, meta: { changes: 0, duration: 0 } }),
              run: async () => ({ results: [], success: true, meta: { changes: 0, duration: 0 } }),
            };
          },
        };
      },
    } as unknown as SqlDatabase;
    return { db, prepared, bound };
  }

  it("keeps every allow-listed finance target guard on its exact owner table and version column", () => {
    expect(Object.keys(financeTable)).toHaveLength(30);
    for (const [targetType, table] of Object.entries(financeTable)) {
      const { db } = stubDb();
      const facts = FinanceAttachmentFinanceFacts(db);
      const guard = facts.versionGuard("command-1", targetType, "target-1", "company-1", 2, 0) as unknown as {
        query: string;
        args: unknown[];
      } | null;
      expect(guard, `${targetType} guard`).not.toBeNull();
      const column = versionColumn[targetType] ?? "version";
      expect(guard!.query).toContain(`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND ${column}=?`);
      expect(guard!.args).toEqual(["target-1", "company-1", 2]);
    }
  });

  it("routes every allow-listed finance version step to the native owner write with no local SQL", async () => {
    expect(Object.keys(financeTable)).toHaveLength(30);
    for (const targetType of Object.keys(financeTable)) {
      const { db, prepared } = stubDb();
      const facts = FinanceAttachmentFinanceFacts(db);
      const step = facts.versionStep(targetType, "target-1", 2, 77) as Promise<unknown>;
      expect(step, `${targetType} step`).toBeInstanceOf(Promise);
      // No transport is configured in this unit lane: the native owner write
      // fails closed rather than falling back to a TypeScript UPDATE.
      await expect(step, `${targetType} native delegation`).rejects.toThrow(
        "The native Finance owner write is unavailable.",
      );
      expect(prepared, `${targetType} must author no local SQL`).toHaveLength(0);
    }
  });

  it("fails closed for unknown finance targets without touching the guard transport", () => {
    const { db, prepared } = stubDb();
    const facts = FinanceAttachmentFinanceFacts(db);
    for (const targetType of ["finance_unknown", "finance_journal; DROP TABLE finance_journals", "", "invoice_payment"]) {
      expect(facts.versionGuard("command-1", targetType, "target-1", "company-1", 1, 0)).toBeNull();
      expect(facts.versionStep(targetType, "target-1", 1, 77)).toBeNull();
    }
    expect(prepared).toHaveLength(0);
  });

  it("keeps service invoice guards scoped and routes steps to the native owner write", async () => {
    const { db, prepared } = stubDb();
    const facts = attachmentServiceFinanceFacts(db);
    const actor = { companyId: "company-1" } as never;
    for (const targetType of ["invoice_payment", "supplier_bill"]) {
      const guard = facts.versionGuard("command-1", targetType, "target-1", actor, 4, 1) as unknown as {
        query: string;
        args: unknown[];
      } | null;
      expect(guard, `${targetType} guard`).not.toBeNull();
      expect(guard!.query).toBe("EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=?)");
      expect(guard!.args).toEqual(["target-1", "company-1", 4]);
      const step = facts.versionStep(targetType, "target-1", 4, 78) as Promise<unknown>;
      expect(step, `${targetType} step`).toBeInstanceOf(Promise);
      await expect(step, `${targetType} native delegation`).rejects.toThrow(
        "The native Finance owner write is unavailable.",
      );
      expect(prepared).toHaveLength(0);
    }
  });

  it("fails closed for unknown service targets without preparing a statement", () => {
    const { db, prepared } = stubDb();
    const facts = attachmentServiceFinanceFacts(db);
    const actor = { companyId: "company-1" } as never;
    for (const targetType of ["finance_journal", "invoice_payment; DROP TABLE finance_invoices", ""]) {
      expect(facts.versionGuard("command-1", targetType, "target-1", actor, 1, 0)).toBeNull();
      expect(facts.versionStep(targetType, "target-1", 1, 78)).toBeNull();
    }
    expect(prepared).toHaveLength(0);
  });
});
