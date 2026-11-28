import "server-only";
import { runtimeEnv } from "../../../../../platform/runtime/server/src/public/runtime";
import { ownerOnboardingSlot } from "@erpx/manage-erpx-server/public/people-app-entrypoint-facts";
export async function onboardingPresentationQueries() {
  const env = await runtimeEnv();
  return { publicBetaEnabled: env.ERPX_PUBLIC_BETA === "true", slot: (userId: string) => ownerOnboardingSlot(env.CONTROL_DB, userId) };
}
