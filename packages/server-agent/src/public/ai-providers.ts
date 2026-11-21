/**
 * Provider runtime and retained Administration read.
 *
 * The feature-admin AI-provider route is served by the mounted native owner.
 * Credential sealing and provider transport remain A1-deferred there. This
 * module retains the in-process resolution/guard/usage surface consumed by
 * other owners until their AI drains move to native ports.
 */
export {
  aiProviderOverview,
  resolveAiProvider,
  aiSnapshotGuard,
  aiUsageStatement,
} from "@erpx/server-agent/adapters/ai-runtime";
export type {
  AiConnectionView,
  AiProviderOverview,
  AiProviderSnapshot,
} from "@erpx/server-agent/domain/providers";
