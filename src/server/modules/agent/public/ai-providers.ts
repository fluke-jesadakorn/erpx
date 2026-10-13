/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { aiConnectionSchema, aiVersionSchema, aiSelectionSchema, requireAiAdmin, aiProviderOverview, saveAiConnection, testAiConnection, aiDiscoverySchema, discoverAiModelsInput, discoverAiModels, activateAiConnection, selectAiProviders, disableAiConnection, resolveAiProvider, aiSnapshotGuard, aiUsageStatement } from "@/server/modules/agent/adapters/ai-providers";
export type { AiConnectionView, AiProviderOverview, AiProviderSnapshot } from "@/server/modules/agent/adapters/ai-providers";
