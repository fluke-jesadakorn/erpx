import type { AiProtocol } from "@/lib/ai/providers";
import { pinnedNodeRequest } from "./runtime/https";
import { ApiError } from "./http";
import type { BetaEnvironment } from "./beta-config";

export type AiEndpoint = {
  protocol?: AiProtocol;
  sessionId?: string;
  baseUrl: string;
  apiKey: string | null;
  model: string;
};
export type AiMessage = {
  role: "system" | "user";
  content:
    | string
    | (
        | { type: "text"; text: string }
        | { type: "image_url"; image_url: { url: string } }
      )[];
};
export function aiBaseUrl(value: string) {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new ApiError(
      422,
      "AI_URL_INVALID",
      "Enter the provider's HTTPS base URL.",
    );
  }
  const host = url.hostname.toLowerCase();
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    !host.includes(".") ||
    host.endsWith(".") ||
    /^[\d.]+$/.test(host) ||
    host.includes(":") ||
    /(^|\.)(localhost|local|internal|test|invalid|example)$/.test(host)
  )
    throw new ApiError(
      422,
      "AI_URL_INVALID",
      "Use a public HTTPS hostname without credentials, a query, or a fragment.",
    );
  if (/\/(chat\/completions|responses|messages|models)\/?$/.test(url.pathname))
    throw new ApiError(
      422,
      "AI_BASE_URL_REQUIRED",
      "Enter the base URL, such as https://provider.com/v1, without /chat/completions or /models.",
    );
  return url.toString().replace(/\/$/, "");
}
export function publicAddress(address: string) {
  if (address.includes(":")) {
    try {
      address = new URL(`https://[${address}]/`).hostname.slice(1, -1);
    } catch {
      return false;
    }
    const [first, second] = address
      .split(":")
      .map((part) => parseInt(part || "0", 16));
    // Public unicast only; exclude IANA special-purpose and transition ranges.
    return (
      first >= 0x2000 &&
      first <= 0x3fff &&
      first !== 0x2002 &&
      !(first === 0x2001 && (second < 0x200 || second === 0xdb8)) &&
      !(first === 0x3fff && second < 0x1000)
    );
  }
  const parts = address.split(".").map(Number);
  if (
    parts.length !== 4 ||
    parts.some((n) => !Number.isInteger(n) || n < 0 || n > 255)
  )
    return false;
  const [a, b, c] = parts;
  return (
    a !== 0 &&
    a !== 10 &&
    a !== 127 &&
    a < 224 &&
    !(a === 100 && b >= 64 && b <= 127) &&
    !(a === 169 && b === 254) &&
    !(a === 172 && b >= 16 && b <= 31) &&
    !(a === 192 && (b === 168 || b === 0 || b === 2)) &&
    !(a === 198 && (b === 18 || b === 19 || (b === 51 && c === 100))) &&
    !(a === 203 && b === 0 && c === 113)
  );
}
async function boundedJson(
  response: Response,
  limit = 128000,
): Promise<unknown> {
  const reader = response.body?.getReader();
  if (!reader)
    throw new ApiError(
      503,
      "AI_EMPTY_RESPONSE",
      "The provider returned an empty response.",
    );
  const decoder = new TextDecoder();
  let raw = "",
    bytes = 0;
  while (true) {
    const chunk = await reader.read();
    if (chunk.done) break;
    bytes += chunk.value.byteLength;
    if (bytes > limit) {
      await reader.cancel();
      throw new ApiError(
        503,
        "AI_RESPONSE_LIMIT",
        "The provider response exceeded the supported size.",
      );
    }
    raw += decoder.decode(chunk.value, { stream: true });
  }
  try {
    return JSON.parse(raw + decoder.decode());
  } catch {
    throw new ApiError(
      503,
      "AI_INVALID_RESPONSE",
      "The provider did not return valid JSON.",
    );
  }
}
async function resolvePublicHost(host: string) {
  const answers = await Promise.all(
    ["A", "AAAA"].map(async (type) => {
      const response = await fetch(
        `https://cloudflare-dns.com/dns-query?name=${encodeURIComponent(host)}&type=${type}`,
        {
          headers: { Accept: "application/dns-json" },
          redirect: "error",
          signal: AbortSignal.timeout(5000),
        },
      );
      if (!response.ok)
        throw new ApiError(
          503,
          "AI_DNS_UNAVAILABLE",
          "The provider hostname could not be verified.",
        );
      const result = (await boundedJson(response, 16000)) as {
        Status?: number;
        Answer?: { type: number; data: string }[];
      };
      if (result.Status !== 0)
        throw new ApiError(
          422,
          "AI_HOST_UNAVAILABLE",
          "The provider hostname does not resolve publicly.",
        );
      return (result.Answer ?? [])
        .filter((a) => a.type === 1 || a.type === 28)
        .map((a) => a.data);
    }),
  );
  const addresses = answers.flat();
  if (!addresses.length || addresses.some((a) => !publicAddress(a)))
    throw new ApiError(
      422,
      "AI_PRIVATE_ENDPOINT",
      "AI connections must resolve exclusively to public addresses.",
    );
  return addresses;
}
export async function providerRequest(
  endpoint: AiEndpoint,
  path: "models" | "chat/completions" | "responses" | "messages",
  body?: unknown,
  timeoutMs = 25000,
) {
  const base = aiBaseUrl(endpoint.baseUrl),
    addresses = await resolvePublicHost(new URL(base).hostname),
    headers: Record<string, string> = {
      "Content-Type": "application/json",
      "User-Agent": "ERPX/0.1",
      ...(new URL(base).hostname === "opencode.ai" && path !== "models" ? { "x-opencode-session": endpoint.sessionId ?? crypto.randomUUID() } : {}),
      ...(path === "messages" ? { "anthropic-version": "2023-06-01" } : {}),
      ...(endpoint.apiKey
        ? path === "messages" ? { "x-api-key": endpoint.apiKey } : { Authorization: `Bearer ${endpoint.apiKey}` }
        : {}),
    };
  let response: Response;
  // Pin verified public DNS addresses while retaining the original TLS name.
  try {
    response = await pinnedNodeRequest(`${base}/${path}`, addresses, headers, body, timeoutMs);
  } catch {
    throw new ApiError(
      503,
      "AI_CONNECTION_FAILED",
      "The provider timed out, redirected, or could not be reached.",
    );
  }
  if (!response.ok) {
    await response.body?.cancel();
    const message =
      response.status === 401 || response.status === 403
        ? "The provider rejected authentication. Check the API key."
        : response.status === 404
          ? "The provider endpoint or model was not found."
          : response.status === 429
            ? "The provider rate limit or account quota was reached."
            : "The provider could not complete this request.";
    throw new ApiError(503, "AI_PROVIDER_ERROR", message);
  }
  return boundedJson(response);
}
export async function generateProviderJson(
  endpoint: AiEndpoint,
  messages: AiMessage[],
  maxTokens = 2000,
) {
  const timeoutMs = messages.some(m => Array.isArray(m.content) && m.content.some(p => p.type === "image_url")) ? 90000 : 25000;
  const protocol = endpoint.protocol ?? "chat-completions";
  let rawText: string, inputTokens: unknown, outputTokens: unknown;
  const incomplete = () => new ApiError(503, "AI_INCOMPLETE", "The provider did not finish a usable response. Retry or continue manually.");
  if (protocol === "responses") {
    const response = await providerRequest(endpoint, "responses", {
      model: endpoint.model,
      input: messages.map(m => ({ role: m.role, content: typeof m.content === "string"
        ? m.content : m.content.map(p => p.type === "text" ? { type: "input_text", text: p.text }
          : { type: "input_image", image_url: p.image_url.url, detail: "auto" }) })),
      text: { format: { type: "json_object" } }, max_output_tokens: maxTokens, store: false,
    }, timeoutMs) as { status?: string; error?: unknown; incomplete_details?: unknown; output?: { type?: string; status?: string; content?: { type?: string; text?: string }[] }[]; usage?: { input_tokens?: number; output_tokens?: number } };
    if (response.status !== "completed" || response.error || response.incomplete_details || !Array.isArray(response.output)) throw incomplete();
    const output = response.output.filter(o => o.type === "message");
    if (!output.length || output.some(o => o.status !== "completed" || !Array.isArray(o.content) || o.content.some(p => p.type !== "output_text" || typeof p.text !== "string"))) throw incomplete();
    rawText = output.flatMap(o => o.content!.map(p => p.text)).join("");
    inputTokens = response.usage?.input_tokens; outputTokens = response.usage?.output_tokens;
  } else if (protocol === "messages") {
    const parts = (content: AiMessage["content"]): unknown[] => typeof content === "string" ? [{ type: "text", text: content }] : content.map(p => {
      if (p.type === "text") return p;
      const data = /^data:(image\/(?:png|jpeg|gif|webp));base64,([A-Za-z0-9+/=]+)$/.exec(p.image_url.url);
      if (p.image_url.url.startsWith("data:") && !data)
        throw new ApiError(422, "AI_IMAGE_INVALID", "Use a supported base64 image.");
      return { type: "image", source: data ? { type: "base64", media_type: data[1], data: data[2] } : { type: "url", url: p.image_url.url } };
    });
    const response = await providerRequest(endpoint, "messages", {
      model: endpoint.model, max_tokens: maxTokens,
      system: [...messages.filter(m => m.role === "system").flatMap(m => parts(m.content)), { type: "text", text: "Return only a valid JSON object, without Markdown fences." }],
      messages: messages.filter(m => m.role !== "system").map(m => ({ role: m.role, content: parts(m.content) })),
    }, timeoutMs) as { stop_reason?: string; content?: { type?: string; text?: string }[]; usage?: { input_tokens?: number; output_tokens?: number; cache_creation_input_tokens?: number; cache_read_input_tokens?: number } };
    if (response.stop_reason !== "end_turn" || !Array.isArray(response.content) || !response.content.some(p => p.type === "text") || response.content.some(p => !["text", "thinking", "redacted_thinking"].includes(p.type ?? "") || (p.type === "text" && typeof p.text !== "string"))) throw incomplete();
    rawText = response.content.filter(p => p.type === "text").map(p => p.text).join("");
    inputTokens = tokens(response.usage?.input_tokens) + tokens(response.usage?.cache_creation_input_tokens) + tokens(response.usage?.cache_read_input_tokens);
    outputTokens = response.usage?.output_tokens;
  } else {
    const response = await providerRequest(endpoint, "chat/completions", {
      model: endpoint.model, messages, response_format: { type: "json_object" }, max_tokens: maxTokens,
    }, timeoutMs) as { choices?: { finish_reason?: string; message?: { content?: string; refusal?: unknown } }[]; usage?: { prompt_tokens?: number; completion_tokens?: number } };
    const choice = response.choices?.[0];
    if (choice?.finish_reason !== "stop" || choice.message?.refusal || typeof choice.message?.content !== "string") throw incomplete();
    rawText = choice.message.content;
    inputTokens = response.usage?.prompt_tokens; outputTokens = response.usage?.completion_tokens;
  }
  let result: unknown;
  try {
    result = JSON.parse(rawText);
    if (result === null || typeof result !== "object" || Array.isArray(result)) throw Error();
  } catch {
    throw new ApiError(503, "AI_INVALID_OUTPUT", "The model did not produce the required JSON object.");
  }
  return { result, inputTokens: tokens(inputTokens), outputTokens: tokens(outputTokens) };
}
function tokens(value: unknown) {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : 0;
}
type Sealed = { keyId: string; nonce: string; ciphertext: string };
const encode = (bytes: Uint8Array) =>
  btoa(Array.from(bytes, (b) => String.fromCharCode(b)).join(""));
const decode = (text: string) =>
  Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
async function credentialKey(env: BetaEnvironment, keyId: string) {
  try {
    const ring = JSON.parse(env.AI_CREDENTIAL_KEYS ?? "{}") as Record<
      string,
      string
    >;
    const raw = decode(ring[keyId] ?? "");
    if (raw.length !== 32) throw Error();
    return await crypto.subtle.importKey("raw", raw, "AES-GCM", false, [
      "encrypt",
      "decrypt",
    ]);
  } catch {
    throw new ApiError(
      503,
      "AI_KEY_STORAGE_UNAVAILABLE",
      "Encrypted credential storage needs deployment configuration.",
    );
  }
}
export async function sealAiKey(
  env: BetaEnvironment,
  companyId: string,
  providerId: string,
  key: string,
) {
  const keyId = env.AI_CREDENTIAL_KEY_ID ?? "v1",
    nonce = crypto.getRandomValues(new Uint8Array(12)),
    cryptoKey = await credentialKey(env, keyId);
  const ciphertext = await crypto.subtle.encrypt(
    {
      name: "AES-GCM",
      iv: nonce,
      additionalData: new TextEncoder().encode(`${companyId}:${providerId}`),
    },
    cryptoKey,
    new TextEncoder().encode(key),
  );
  return JSON.stringify({
    keyId,
    nonce: encode(nonce),
    ciphertext: encode(new Uint8Array(ciphertext)),
  } satisfies Sealed);
}
export async function openAiKey(
  env: BetaEnvironment,
  companyId: string,
  providerId: string,
  sealed: string,
) {
  try {
    const value = JSON.parse(sealed) as Sealed,
      cryptoKey = await credentialKey(env, value.keyId);
    return new TextDecoder().decode(
      await crypto.subtle.decrypt(
        {
          name: "AES-GCM",
          iv: decode(value.nonce),
          additionalData: new TextEncoder().encode(
            `${companyId}:${providerId}`,
          ),
        },
        cryptoKey,
        decode(value.ciphertext),
      ),
    );
  } catch {
    throw new ApiError(
      503,
      "AI_KEY_STORAGE_UNAVAILABLE",
      "This connection's encrypted credential cannot be opened. Check deployment key configuration.",
    );
  }
}
