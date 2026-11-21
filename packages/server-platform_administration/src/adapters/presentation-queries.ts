import "server-only";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { platformExecutiveSnapshot } from "@erpx/server-platform_administration/public/platform-executive";
export async function platformExecutiveForPresentation(actor: Parameters<typeof platformExecutiveSnapshot>[0]) {
  const env = await runtimeEnv();
  return platformExecutiveSnapshot(actor, env.CONTROL_DB, env.CONTROL_READ_DB);
}
