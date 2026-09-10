import { afterEach, expect, it, vi } from "vitest";
import * as https from "../src/server/runtime/https";
import { generateProviderJson, type AiMessage } from "../src/server/ai-transport";
import { aiProtocols, modelProtocol, supportedModels } from "../src/lib/ai/providers";
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const messages: AiMessage[] = [ { role: "system", content: "Return JSON." }, { role: "user", content: [{ type: "text", text: "Read image" }, { type: "image_url", image_url: { url: "data:image/png;base64,YQ==" } }] } ];
function mock(response: unknown) {
  vi.stubGlobal("fetch", vi.fn(async () => Response.json({ Status: 0, Answer: [{ type: 1, data: "8.8.8.8" }] })));
  return vi.spyOn(https, "pinnedNodeRequest").mockResolvedValue(Response.json(response));
}
const good = {
  "chat-completions": { choices: [{ finish_reason: "stop", message: { content: '{"ok":true}' } }], usage: { prompt_tokens: 4, completion_tokens: 2 } },
  responses: { status: "completed", output: [{ type: "message", status: "completed", content: [{ type: "output_text", text: '{"ok":true}' }] }], usage: { input_tokens: 4, output_tokens: 2 } },
  messages: { stop_reason: "end_turn", content: [{ type: "text", text: '{"ok":true}' }], usage: { input_tokens: 1, cache_read_input_tokens: 2, cache_creation_input_tokens: 1, output_tokens: 2 } },
};
for (const protocol of aiProtocols) {
  it(`normalizes ${protocol} images, authentication, JSON and usage`, async () => {
    const request = mock(good[protocol]);
    expect(await generateProviderJson({ baseUrl: "https://opencode.ai/zen/v1", apiKey: "private", model: "synthetic", protocol }, messages)).toEqual({ result: { ok: true }, inputTokens: 4, outputTokens: 2 });
    const [url, , headers, body, timeoutMs] = request.mock.calls[0];
    expect(timeoutMs).toBe(90000);
    expect(url).toBe(`https://opencode.ai/zen/v1/${protocol === "chat-completions" ? "chat/completions" : protocol}`);
    expect(headers).toMatchObject(protocol === "messages" ? { "x-api-key": "private", "anthropic-version": "2023-06-01" } : { Authorization: "Bearer private" });
    if (protocol === "responses") expect(body).toMatchObject({ store: false, input: [{ role: "system" }, { content: [{ type: "input_text" }, { type: "input_image", image_url: "data:image/png;base64,YQ==" }] }] });
    if (protocol === "messages") expect(body).toMatchObject({ messages: [{ role: "user", content: [{ type: "text" }, { type: "image", source: { type: "base64", media_type: "image/png", data: "YQ==" } }] }] });
  });
}
it.each([
  ["responses", { status: "incomplete", output: [] }],
  ["responses", { status: "completed", output: [{ type: "message", status: "completed", content: [{ type: "refusal" }] }] }],
  ["messages", { stop_reason: "max_tokens", content: [{ type: "text", text: "{}" }] }],
  ["messages", { stop_reason: "refusal", content: [] }],
  ["chat-completions", { choices: [{ finish_reason: "length", message: { content: "{}" } }] }],
] as const)("rejects incomplete/refused %s output", async (protocol, response) => {
  mock(response);
  await expect(generateProviderJson({ baseUrl: "https://api.provider.com/v1", model: "test", apiKey: null, protocol }, messages)).rejects.toMatchObject({ code: "AI_INCOMPLETE" });
});
it.each(["not json", "[]", "null", '"text"'])("rejects invalid JSON objects: %s", async text => {
  mock({ stop_reason: "end_turn", content: [{ type: "text", text }] });
  await expect(generateProviderJson({ baseUrl: "https://api.provider.com/v1", model: "test", apiKey: null, protocol: "messages" }, messages)).rejects.toMatchObject({ code: "AI_INVALID_OUTPUT" });
});
it("keeps Go and Zen protocol guidance separate and leaves new models unknown", () => {
  expect(modelProtocol("opencode-go", "minimax-m3")).toBe("messages");
  expect(modelProtocol("opencode-zen", "minimax-m3")).toBe("chat-completions");
  expect(modelProtocol("opencode-zen", "gemini-3.8-flash")).toBe("unsupported");
  expect(modelProtocol("opencode-go", "brand-new-model")).toBeNull();
});
it("sends the stable session and honest client identity only to OpenCode inference", async () => {
  const request = mock(good.messages);
  const endpoint = { baseUrl: "https://opencode.ai/zen/go/v1", model: "minimax-m3", apiKey: "test-key", protocol: "messages" as const, sessionId: "stable-workflow-session" };
  await generateProviderJson(endpoint, messages);
  expect(request.mock.calls[0][2]).toMatchObject({ "x-opencode-session": "stable-workflow-session", "User-Agent": "ERPX/0.1", "x-api-key": "test-key" });
  request.mockResolvedValueOnce(Response.json(good.messages));
  await generateProviderJson({ ...endpoint, baseUrl: "https://api.provider.com/v1" }, messages);
  expect(request.mock.calls[1][2]).not.toHaveProperty("x-opencode-session");
});

it("includes all newly reconciled catalog IDs and keeps documented Qwen protocols", () => {
  const ids = ["muse-spark-1.3-contributor-free", "muse-spark-1.2-contributor-free", "deepseek-v4-flash-free", "ling-3.0-flash-fin-free", "nemotron-3-ultra-free", "nemotron-3.5-lightning-free", "claude-sonnet-4"];
  expect(supportedModels("https://opencode.ai/zen/v1", ids)).toEqual([...ids].sort());
  expect(modelProtocol("opencode-zen", ids[0])).toBe("responses");
  expect(modelProtocol("opencode-zen", ids[1])).toBe("responses");
  expect(modelProtocol("opencode-zen", "qwen3.7-max")).toBe("messages");
  expect(modelProtocol("opencode-go", "qwen3.7-max")).toBe("messages");
  expect(modelProtocol("opencode-go", "qwen3.8-flash")).toBe("messages");
  expect(supportedModels("https://opencode.ai/zen/go/v1", ["mimo-v2-pro", "mimo-v2-omni", "hy3-preview"])).toHaveLength(3);
});
