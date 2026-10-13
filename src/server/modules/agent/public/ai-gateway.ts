/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { readAiGatewayPolicy, setAiPlatformKeyEnabled, recordAiProviderStatement, reserveAiGatewayRequest, finishAiGatewayRequest, recordVerifiedAiCost, reconcileAiProviderStatement, safeAiGatewayRows, updateAiGatewayPolicy } from "@/server/modules/agent/adapters/ai-gateway";
export type { AiGatewayScope, SafeAiGatewayRow, AiGatewayPolicyView } from "@/server/modules/agent/adapters/ai-gateway";
