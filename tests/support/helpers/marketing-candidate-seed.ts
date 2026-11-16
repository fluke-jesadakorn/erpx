import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "@erpx/server-runtime/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { withMarketingPurchasedCommand } from "@erpx/server-marketing/adapters/purchases";

export async function createMarketingCandidate(
  a: WorkContext,
  raw: unknown,
  key: string,
): Promise<{ id: string; status: string; version: number }> {
  const input = (raw as { formId: string; formVersion: number; branchId: string; contactName: string; contactDetail: string; consentEvidence: string; attribution?: Record<string, string>; brief: string });
  return withMarketingPurchasedCommand(
    a,
    {
      key,
      operation: "marketing.candidate.create",
      target: { kind: "candidate", id: `marketing-candidate-${key}` },
      purpose: "new-work",
      sources: [{ kind: "asset", id: input.formId }],
    },
    async (db) => {
      const now = Date.now();
      const id = `marketing-candidate-${key}`;
      return executeCommand(db, a, key, "marketing.candidate.create", input, id, 1, { id, status: "pending", version: 1 }, [
        db
          .prepare("INSERT INTO marketing_lead_candidates VALUES(?,?,?,?,?,?,?,?,?,?,?,'pending',NULL,?,NULL,1,?,?)")
          .bind(id, a.companyId, input.branchId, a.departmentId, input.formId, input.formVersion, input.contactName, input.contactDetail, input.consentEvidence, JSON.stringify(input.attribution ?? {}), input.brief, a.userId, now, now),
        db
          .prepare("INSERT INTO marketing_engagement_events VALUES(?,?,1,'form_submit','web',?,?,?)")
          .bind(`marketing-engagement-${key}`, id, input.consentEvidence, a.userId, now),
      ]);
    },
  );
}