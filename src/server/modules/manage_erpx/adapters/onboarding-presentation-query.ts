import "server-only";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { ownerOnboardingSlot } from "@/server/modules/manage_erpx/public/people-app-entrypoint-facts";
export async function onboardingPresentationQueries() {
  const env = await runtimeEnv();
  return { publicBetaEnabled: env.ERPX_PUBLIC_BETA === "true", slot: (userId: string) => ownerOnboardingSlot(env.CONTROL_DB, userId) };
}
