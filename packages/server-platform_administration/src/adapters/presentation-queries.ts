import "server-only";
import { runtimeEnv, holdingResources, activeHoldingBindings } from "../../../../packages/server-runtime/src/public/runtime";
import { resourceSlots } from "@erpx/feature-runtime/lib/holding-registry";
import { platformProvisioningSnapshot, summarizePlatformOperations } from "@erpx/server-platform_administration/public/platform-operations";
import { platformExecutiveSnapshot } from "@erpx/server-platform_administration/public/platform-executive";
export async function platformOperationsForPresentation(actor: Parameters<typeof summarizePlatformOperations>[0]) {
  const env = await runtimeEnv();
  const provisioning = await platformProvisioningSnapshot(env.CONTROL_DB, resourceSlots(env));
  const bindings = (await activeHoldingBindings()).filter(binding => binding.id !== "h003");
  const operations = await summarizePlatformOperations(actor, env.CONTROL_DB, await Promise.all(bindings.map(async binding => ({
    id: binding.id, slug: binding.slug, displayName: binding.displayName, db: (await holdingResources(binding.id)).write,
  }))));
  return { provisioning, operations };
}
export async function platformExecutiveForPresentation(actor: Parameters<typeof platformExecutiveSnapshot>[0]) {
  const env = await runtimeEnv();
  return platformExecutiveSnapshot(actor, env.CONTROL_DB, env.CONTROL_READ_DB);
}
