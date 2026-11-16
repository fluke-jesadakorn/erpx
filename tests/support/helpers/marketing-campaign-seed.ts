import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "@erpx/server-runtime/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { createMarketingCampaignApplication } from "@erpx/server-marketing/application/campaigns";
import { marketingCampaignCommand } from "@erpx/server-marketing/domain/campaigns";
import {
  requireMarketingPurchasedRead,
  withMarketingPurchasedCommand,
} from "@erpx/server-marketing/adapters/purchases";

/**
 * Test-only native-seeding helper for Marketing campaign creation.
 *
 * The TypeScript `marketing.campaign.create` writer is retired: the Rust owner
 * (`src/composition/src/marketing.rs` `create_marketing_campaign`) is authoritative
 * via the native Marketing lead transport. Tests that need durable campaign rows
 * seed them through this helper instead of the removed TypeScript writer.
 */
export async function createMarketingCampaign(
  a: WorkContext,
  raw: unknown,
  key: string,
): Promise<{ id: string; code: string | null; name: string | null; version: number }> {
  const input = marketingCampaignCommand.parse(raw);
  return withMarketingPurchasedCommand(
    a,
    {
      key,
      operation: `marketing.campaign.${input.action}`,
      target: { kind: "campaign", id: `campaign-${key}` },
      purpose: "new-work",
    },
    async (db) => {
      let resource: Promise<SqlDatabase> | null = Promise.resolve(db);
      const database = () => resource!;
      const application = createMarketingCampaignApplication({
        clock: { now: () => Date.now() },
        authorize: (write) => {
          requirePermission(a, write ? "records.write" : "records.read");
          if (!hasModuleAccess(a.modules, "marketing"))
            throw new ApiError(403, "MODULE_FORBIDDEN", "Marketing access required.");
        },
        authorizeFinance: () => {
          requirePermission(a, "finance.read");
        },
        campaigns: async () => [],
        costSources: async () => [],
        replay: async (k, op, id, inp) =>
          replayCommand(await database(), a, k, op, id, inp),
        commit: async (mutation, result) => {
          const { input: i, key: cmdKey, id, operation: op, now } = mutation;
          if (i.action !== "create") {
            throw new ApiError(503, "NATIVE_MARKETING_LEAD_UNAVAILABLE", "Campaign create is native-only.");
          }
          const write = await database();
          return executeCommand(write, a, cmdKey, op, i, id, 1, result, [
            write
              .prepare(
                "INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES(?,?,?,?,?,?)",
              )
              .bind(id, a.companyId, i.code, i.name, a.userId, now),
          ]);
        },
      });
      try {
        const res = await application.change(raw, key);
        return res as { id: string; code: string | null; name: string | null; version: number };
      } catch (error) {
        if (error instanceof BusinessRuleError)
          throw new ApiError(error.status, error.code, error.message);
        throw error;
      }
    },
  );
}