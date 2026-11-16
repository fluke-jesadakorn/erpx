import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "@erpx/server-runtime/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { withMarketingPurchasedCommand } from "@erpx/server-marketing/adapters/purchases";

/**
 * Test-only native-seeding helper for Marketing asset review transitions
 * (activate / reject / retire).
 */
export async function reviewMarketingAsset(
  a: WorkContext,
  assetId: string,
  review: { action: "activate" | "reject" | "retire"; expectedVersion: number; reason: string },
  key: string,
): Promise<{ id: string; status: string; version: number }> {
  const status =
    review.action === "activate" ? "active" : review.action === "reject" ? "rejected" : "retired";
  return withMarketingPurchasedCommand(
    a,
    {
      key,
      operation: `marketing.asset.${review.action}`,
      target: { kind: "asset", id: assetId },
      existing: true,
      purpose: review.action === "activate" ? "new-work" : "settle-existing",
    },
    async (db) => {
      const write = db;
      const now = Date.now();
      return executeCommand(write, a, key, `marketing.asset.${review.action}`, { action: review.action }, assetId, review.expectedVersion + 1, { id: assetId, status, version: review.expectedVersion + 1 }, [
        write
          .prepare("UPDATE marketing_assets SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?")
          .bind(status, a.userId, now, assetId),
        write
          .prepare("INSERT INTO marketing_asset_events VALUES(?,?,?,?,?,?,?,?)")
          .bind(`marketing-asset-event-${key}`, assetId, review.expectedVersion + 1, review.action, a.userId, review.reason, status, now),
      ]);
    },
  );
}