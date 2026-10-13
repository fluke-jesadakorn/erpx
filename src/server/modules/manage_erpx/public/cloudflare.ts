/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { runtimeEnv, isHoldingId, holdingResources, holdingMigrationDatabase, holdingBindingBySlug, defaultHoldingBinding, activeHoldingBindings, runtimeEnv as cloudflareEnv } from "@/server/modules/runtime/public/runtime";
export type { HoldingId, HoldingBinding } from "@/server/modules/runtime/public/runtime";
