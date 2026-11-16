import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "@erpx/server-runtime/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { createAssetApplication, type AssetPorts } from "@erpx/server-marketing/application/asset";
import { marketingPresentationReferences } from "@erpx/server-marketing/public/marketing-public-form";
import { assetSchema } from "@erpx/server-marketing/domain/asset";
import { withMarketingPurchasedCommand } from "@erpx/server-marketing/adapters/purchases";

/**
 * Test-only native-seeding helper for Marketing asset creation.
 *
 * The TypeScript `marketing.asset.create` writer (`packages/server-marketing/src/adapters/asset.ts`)
 * is retired: the Rust owner (`src/composition/src/marketing.rs` `create_marketing_asset`)
 * is authoritative via the native Marketing lead transport. Tests that need durable
 * asset rows seed them through this helper instead of the removed TypeScript writer.
 *
 * It reproduces the exact rows the retired writer persisted, preserving the form
 * consent validation, command replay and purchase gating so existing assertions
 * (PACKAGE_REQUIRED / PACKAGE_DRAINING / etc.) keep their meaning.
 */
export async function createMarketingAsset(
  a: WorkContext,
  raw: unknown,
  key: string,
  provided?: SqlDatabase,
): Promise<{ id: string; status: string; version: number }> {
  async function create(
    actor: WorkContext,
    commandKey: string,
    database?: SqlDatabase,
  ) {
    let resource: Promise<SqlDatabase> | null = database
      ? Promise.resolve(database)
      : null;
    const db = () =>
      resource ?? (resource = holdingResources(actor.holdingId).then((x) => x.write));
    const ports: AssetPorts = {
      clock: { now: () => Date.now() },
      async authorize() {
        requirePermission(actor, "records.write");
        if (!hasModuleAccess(actor.modules, "marketing"))
          throw new ApiError(403, "MODULE_FORBIDDEN", "Your current role cannot access Marketing.");
      },
      validatePresentation: marketingPresentationReferences,
      async replay(k, op, id, i) {
        return replayCommand<{ id: string; status: string; version: number }>(
          await db(),
          actor,
          k,
          op,
          id,
          i,
        );
      },
      async persist(command) {
        const write = await db();
        const { key: cmdKey, operation: op, id, input: i, now, result } = command;
        return executeCommand(write, actor, cmdKey, op, i, id, 1, result, [
          write
            .prepare(
              "INSERT INTO marketing_assets VALUES(?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
            )
            .bind(
              id,
              actor.companyId,
              i.kind,
              i.name,
              JSON.stringify(i.configuration),
              JSON.stringify(i.preview),
              actor.userId,
              now,
              now,
            ),
          write
            .prepare(
              "INSERT INTO marketing_asset_events VALUES(?,?,1,'create',?,'Asset drafted for independent review','draft',?)",
            )
            .bind(`marketing-asset-event-${cmdKey}`, id, actor.userId, now),
        ]);
      },
    };
    try {
      return await createAssetApplication(ports)(raw, commandKey);
    } catch (error) {
      if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
      throw error;
    }
  }

  const input = assetSchema.parse(raw);
  return withMarketingPurchasedCommand(
    a,
    {
      key,
      operation: "marketing.asset.create",
      target: { kind: "asset", id: `marketing-${input.kind}-${key}` },
      purpose: "new-work",
    },
    (db) => create(a, key, db),
  );
}