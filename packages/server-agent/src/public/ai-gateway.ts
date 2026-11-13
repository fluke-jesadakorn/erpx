/**
 * Retained in-process AI gateway reservation/finish and policy read for the
 * live `ai-transport` and Legal engineering oracles. Native policy, statement,
 * cost and reconciliation commands live in the Rust owner and are reached
 * through `@erpx/server-agent/public/native-ai-gateway`.
 */
export { readAiGatewayPolicy, reserveAiGatewayRequest, finishAiGatewayRequest } from "@erpx/server-agent/adapters/ai-gateway";
export type { AiGatewayScope, AiGatewayPolicyView } from "@erpx/server-agent/adapters/ai-gateway";
