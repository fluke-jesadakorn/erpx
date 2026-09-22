export const aiProtocols = ["chat-completions", "responses", "messages"] as const;
export type AiProtocol = (typeof aiProtocols)[number];
export const aiPresets = {
  "opencode-zen": { name: "OpenCode Zen", baseUrl: "https://opencode.ai/zen/v1" },
  "opencode-go": { name: "OpenCode Go", baseUrl: "https://opencode.ai/zen/go/v1" },
  custom: { name: "Custom API", baseUrl: "" },
} as const;
export type AiPreset = keyof typeof aiPresets;
export const protocolLabels: Record<AiProtocol, string> = {
  "chat-completions": "Chat Completions",
  responses: "Responses",
  messages: "Messages (Anthropic)",
};
// Exact IDs/protocols from https://opencode.ai/docs/{zen,go}/ and
// https://models.dev/api.json (opencode/opencode-go), reviewed 2026-09-09.
// This is protocol guidance, never the source of the live model list.
const entries = (ids: string, protocol: AiProtocol | "unsupported") =>
  Object.fromEntries(ids.split(" ").map(id => [id, protocol]));
const common = {
  ...entries("gpt-6-astra gpt-5.6-sol gpt-5.6-terra gpt-5.6-luna gpt-5.5 gpt-5.5-pro gpt-5.4 gpt-5.4-pro gpt-5.4-mini gpt-5.4-nano gpt-5.3-codex gpt-5.3-codex-spark gpt-5.2 gpt-5.2-codex gpt-5.1 gpt-5.1-codex gpt-5.1-codex-max gpt-5.1-codex-mini gpt-5 gpt-5-codex gpt-5-nano grok-4.6 grok-4.5 grok-build-0.1 muse-spark-1.3 muse-spark-1.2 muse-spark-1.3-contributor-free muse-spark-1.2-contributor-free muse-spark-1.3-contributor muse-spark-1.2-contributor", "responses"),
  ...entries("claude-fable-5-1 claude-fable-5 claude-opus-5 claude-opus-4-8 claude-opus-4-7 claude-opus-4-6 claude-opus-4-5 claude-sonnet-4 claude-sonnet-5 claude-sonnet-4-6 claude-sonnet-4-5 claude-haiku-4-5 qwen3.7-max qwen3.8-max qwen3.8-flash qwen3.7-plus qwen3.6-plus qwen3.5-plus", "messages"),
  ...entries("deepseek-v4-flash-free ling-3.0-flash-fin-free nemotron-3-ultra-free nemotron-3.5-lightning-free deepseek-v4-pro deepseek-v4-flash deepseek-v4-flash-vision-exp glm-5.3-flash glm-5.3 glm-5.2 glm-5.1 glm-5 kimi-k3 kimi-k2.7-code kimi-k2.6 kimi-k2.5 longcat-2.0 mimo-v2-pro mimo-v2-omni hy3-preview mimo-v2.5 mimo-v2.5-pro mimo-v2.5-free hy4-preview hy3 omen-alpha big-pickle", "chat-completions"),
  ...entries("gemini-3.8-flash gemini-3.7-flash gemini-3.6-flash gemini-3.5-flash gemini-3.5-flash-lite gemini-3.1-pro gemini-3-flash", "unsupported"),
};
export function presetForUrl(url: string): AiPreset {
  const normalized = url.replace(/\/$/, "");
  return (Object.keys(aiPresets) as AiPreset[]).find(p => p !== "custom" && aiPresets[p].baseUrl === normalized) ?? "custom";
}
export function modelProtocol(preset: AiPreset, id: string): AiProtocol | "unsupported" | null {
  if (preset === "custom") return null;
  if (["minimax-m3", "minimax-m2.7", "minimax-m2.5"].includes(id))
    return preset === "opencode-go" ? "messages" : "chat-completions";
  return Object.hasOwn(common, id) ? common[id] : null;
}

export const unsupportedModelMessage = "This model is not supported yet. Choose another model.";
export function supportedModelProtocol(baseUrl: string, id: string): AiProtocol | null {
  if (presetForUrl(baseUrl) === "custom" && id.trim()) return "chat-completions";
  const protocol = modelProtocol(presetForUrl(baseUrl), id.trim());
  return protocol && protocol !== "unsupported" ? protocol : null;
}
export function supportedModels(baseUrl: string, ids: string[]): string[] {
  return [...new Set(ids.map(id => id.trim()))].filter(id => !!supportedModelProtocol(baseUrl, id)).sort();
}
