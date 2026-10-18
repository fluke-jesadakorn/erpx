import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {z} from 'zod';
import {BusinessRuleError,type IdentifierSource} from '../../shared';
import {aiBaseUrl,publicAddress,billableUnits,tokens,type AiEndpoint,type AiMessage} from '../domain/transport';
export interface AiWireResponse{ok:boolean;status:number;json(limit?:number):Promise<unknown>;cancel():Promise<void>;}
export interface AiTransportPorts<Gateway>{identifiers:IdentifierSource;gatewayPermitted(endpoint:AiEndpoint<Gateway>):boolean;approveGateway(endpoint:AiEndpoint<Gateway>):AiEndpoint<Gateway>;dns(host:string,type:'A'|'AAAA'):Promise<{Status?:number;Answer?:{type:number;data:string}[]}>;request(url:string,addresses:string[],headers:Record<string,string>,body:unknown,timeoutMs:number):Promise<AiWireResponse>;reserve(scope:Gateway,provider:string,model:string):Promise<void>;finish(scope:Gateway,status:'succeeded'|'failed',inputTokens:number,outputTokens:number,units:Record<string,number>):Promise<void>;}
export function createAiTransportApplication<Gateway>(ports:AiTransportPorts<Gateway>){
 async function resolvePublicHost(host:string){const answers=await Promise.all((['A','AAAA']as const).map(async type=>{const result=await ports.dns(host,type);if(result.Status!==0)throw new BusinessRuleError(422,'AI_HOST_UNAVAILABLE','The provider hostname does not resolve publicly.');return (result.Answer??[]).filter(a=>a.type===1||a.type===28).map(a=>a.data);}));const addresses = answers.flat();
  if (!addresses.length || addresses.some((a) => !publicAddress(a)))
    throw new BusinessRuleError(
      422,
      "AI_PRIVATE_ENDPOINT",
      "AI connections must resolve exclusively to public addresses.",
    );
  return addresses;
}
 async function providerRequest(endpoint:AiEndpoint<Gateway>,path:'models'|'chat/completions'|'responses'|'messages',body?:unknown,timeoutMs=25000){
  if (endpoint.gateway && !ports.gatewayPermitted(endpoint))
    throw new BusinessRuleError(409,"AI_GATEWAY_REQUIRED","This AI request must pass through the ERPX gateway.");
  const base = aiBaseUrl(endpoint.baseUrl),
    addresses = await resolvePublicHost(new URL(base).hostname),
    headers: Record<string, string> = {
      "Content-Type": "application/json",
      "User-Agent": "ERPX/0.1",
      ...(new URL(base).hostname === "opencode.ai" && path !== "models" ? { "x-opencode-session": endpoint.sessionId ?? ports.identifiers.next() } : {}),
      ...(path === "messages" ? { "anthropic-version": "2023-06-01" } : {}),
      ...(endpoint.apiKey
        ? path === "messages" ? { "x-api-key": endpoint.apiKey } : { Authorization: `Bearer ${endpoint.apiKey}` }
        : {}),
    };
  let response: AiWireResponse;
  // Pin verified public DNS addresses while retaining the original TLS name.
  try {
    response = await ports.request(canonicalUiHref(`${base}/${path}`), addresses, headers, body, timeoutMs);
  } catch {
    throw new BusinessRuleError(
      503,
      "AI_CONNECTION_FAILED",
      "The provider timed out, redirected, or could not be reached.",
    );
  }
  if (!response.ok) {
    let providerMessage: string | undefined;
    if (response.status === 403 && new URL(base).hostname === "opencode.ai") {
      try {
        const body = await response.json(4096);
        providerMessage = z.object({ error: z.object({ message: z.string() }) }).safeParse(body).data?.error.message;
      } catch {
        // Keep the generic provider error when the response body is unavailable.
      }
    } else {
      await response.cancel();
    }
    const message =
      providerMessage === "OpenCode's free tier can only be used from within OpenCode"
        ? "OpenCode free models only work inside OpenCode. Choose another provider for free AI in ERPX."
        : response.status === 401 || response.status === 403
          ? "The provider rejected authentication. Check the API key."
          : response.status === 404
            ? "The provider endpoint or model was not found."
            : response.status === 429
              ? "The provider rate limit or account quota was reached."
              : "The provider could not complete this request.";
    throw new BusinessRuleError(503, "AI_PROVIDER_ERROR", message);
  }
  return response.json();
}
 async function generateProviderJsonCore(
  endpoint: AiEndpoint<Gateway>,
  messages: AiMessage[],
  maxTokens = 2000,
) {
  const wireEndpoint: AiEndpoint<Gateway> = endpoint.gateway ? ports.approveGateway(endpoint) : endpoint;
  const timeoutMs = endpoint.timeoutMs ? Math.max(5000,Math.min(120000,endpoint.timeoutMs)) : messages.some(m => Array.isArray(m.content) && m.content.some(p => p.type === "image_url")) ? 90000 : 25000;
  const protocol = endpoint.protocol ?? "chat-completions";
  let rawText: string, inputTokens: unknown, outputTokens: unknown, providerUsage: unknown;
  const incomplete = () => new BusinessRuleError(503, "AI_INCOMPLETE", "The provider did not finish a usable response. Retry or continue manually.");
  if (protocol === "responses") {
    const response = await providerRequest(wireEndpoint, "responses", {
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
    inputTokens = response.usage?.input_tokens; outputTokens = response.usage?.output_tokens; providerUsage=response.usage;
  } else if (protocol === "messages") {
    const parts = (content: AiMessage["content"]): unknown[] => typeof content === "string" ? [{ type: "text", text: content }] : content.map(p => {
      if (p.type === "text") return p;
      const data = /^data:(image\/(?:png|jpeg|gif|webp));base64,([A-Za-z0-9+/=]+)$/.exec(p.image_url.url);
      if (p.image_url.url.startsWith("data:") && !data)
        throw new BusinessRuleError(422, "AI_IMAGE_INVALID", "Use a supported base64 image.");
      return { type: "image", source: data ? { type: "base64", media_type: data[1], data: data[2] } : { type: "url", url: p.image_url.url } };
    });
    const response = await providerRequest(wireEndpoint, "messages", {
      model: endpoint.model, max_tokens: maxTokens,
      system: [...messages.filter(m => m.role === "system").flatMap(m => parts(m.content)), { type: "text", text: "Return only a valid JSON object, without Markdown fences." }],
      messages: messages.filter(m => m.role !== "system").map(m => ({ role: m.role, content: parts(m.content) })),
    }, timeoutMs) as { stop_reason?: string; content?: { type?: string; text?: string }[]; usage?: { input_tokens?: number; output_tokens?: number; cache_creation_input_tokens?: number; cache_read_input_tokens?: number } };
    if (response.stop_reason !== "end_turn" || !Array.isArray(response.content) || !response.content.some(p => p.type === "text") || response.content.some(p => !["text", "thinking", "redacted_thinking"].includes(p.type ?? "") || (p.type === "text" && typeof p.text !== "string"))) throw incomplete();
    rawText = response.content.filter(p => p.type === "text").map(p => p.text).join("");
    inputTokens = tokens(response.usage?.input_tokens) + tokens(response.usage?.cache_creation_input_tokens) + tokens(response.usage?.cache_read_input_tokens);
    outputTokens = response.usage?.output_tokens; providerUsage=response.usage;
  } else {
    const response = await providerRequest(wireEndpoint, "chat/completions", {
      model: endpoint.model, messages, response_format: { type: "json_object" }, max_tokens: maxTokens,
    }, timeoutMs) as { choices?: { finish_reason?: string; message?: { content?: string; refusal?: unknown } }[]; usage?: { prompt_tokens?: number; completion_tokens?: number } };
    const choice = response.choices?.[0];
    if (choice?.finish_reason !== "stop" || choice.message?.refusal || typeof choice.message?.content !== "string") throw incomplete();
    rawText = choice.message.content;
    inputTokens = response.usage?.prompt_tokens; outputTokens = response.usage?.completion_tokens; providerUsage=response.usage;
  }
  let result: unknown;
  try {
    result = JSON.parse(rawText);
    if (result === null || typeof result !== "object" || Array.isArray(result)) throw Error();
  } catch {
    throw new BusinessRuleError(503, "AI_INVALID_OUTPUT", "The model did not produce the required JSON object.");
  }
  return { result, inputTokens: tokens(inputTokens), outputTokens: tokens(outputTokens), providerUsage };
}
 async function generateProviderJson(endpoint: AiEndpoint<Gateway>,messages: AiMessage[],maxTokens=2000) {
  const scope=endpoint.gateway;
  if (!scope) {
    const {result,inputTokens,outputTokens}=await generateProviderJsonCore(endpoint,messages,maxTokens);
    return {result,inputTokens,outputTokens};
  }
  const provider=new URL(aiBaseUrl(endpoint.baseUrl)).hostname;
  await ports.reserve(scope,provider,endpoint.model);
  try {
    const {result,inputTokens,outputTokens,providerUsage}=await generateProviderJsonCore(endpoint,messages,maxTokens);
    await ports.finish(scope,"succeeded",inputTokens,outputTokens,billableUnits(providerUsage));
    return {result,inputTokens,outputTokens};
  } catch (error) {
    await ports.finish(scope,"failed",0,0,{}).catch(() => {});
    throw error;
  }
}
 return {providerRequest,generateProviderJson};}

