import "server-only";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { ownerOnboardingSlot } from "@erpx/server-manage_erpx/public/people-app-entrypoint-facts";
export async function onboardingPresentationQueries() {
  const env = await runtimeEnv();
  return { publicBetaEnabled: env.ERPX_PUBLIC_BETA === "true", slot: (userId: string) => ownerOnboardingSlot(env.CONTROL_DB, userId) };
}
