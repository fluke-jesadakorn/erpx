import { pinnedNodeRequest } from "@/server/modules/runtime/public/runtime/https";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { BetaEnvironment } from "@/server/modules/manage_erpx/public/beta-config";
import { finishAiGatewayRequest, reserveAiGatewayRequest, type AiGatewayScope } from "@/server/modules/agent/adapters/ai-gateway";

import {BusinessRuleError} from '../../shared';
import {createAiTransportApplication} from '../application/transport';
import {aiBaseUrl as baseUrlPolicy,publicAddress as publicAddressPolicy,type AiEndpoint as Endpoint,type AiMessage} from '../domain/transport';
export type AiEndpoint=Endpoint<AiGatewayScope>;
export type {AiMessage} from '../domain/transport';
const gatewayWire=Symbol('erpx-ai-gateway-transport');
export function withAiGatewayScope(endpoint: AiEndpoint, requestId: string, feature: string, branchId: string | null, usageId=requestId): AiEndpoint {
  return endpoint.gateway ? { ...endpoint, gateway: { ...endpoint.gateway, requestId, usageId, feature, branchId } } : endpoint;
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

function translate(error:unknown):never{if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
async function external<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
const application=createAiTransportApplication<AiGatewayScope>({identifiers:{next:()=>crypto.randomUUID()},gatewayPermitted:endpoint=>Boolean((endpoint as AiEndpoint&{[gatewayWire]?:boolean})[gatewayWire]),approveGateway:endpoint=>Object.assign({...endpoint},{[gatewayWire]:true}),async dns(host,type){const response=await fetch(`https://cloudflare-dns.com/dns-query?name=${encodeURIComponent(host)}&type=${type}`,{headers:{Accept:'application/dns-json'},redirect:'error',signal:AbortSignal.timeout(5000)});if(!response.ok)throw new BusinessRuleError(503,'AI_DNS_UNAVAILABLE','The provider hostname could not be verified.');return boundedJson(response,16000) as Promise<{Status?:number;Answer?:{type:number;data:string}[]}>;},async request(url,addresses,headers,body,timeoutMs){const response=await pinnedNodeRequest(url,addresses,headers,body,timeoutMs);return {ok:response.ok,status:response.status,json:limit=>external(()=>boundedJson(response,limit)),cancel:async()=>{await response.body?.cancel();}};},reserve:(scope,provider,model)=>external(()=>reserveAiGatewayRequest(scope,provider,model)),finish:(...args)=>external(()=>finishAiGatewayRequest(...args))});
export function aiBaseUrl(value:string){try{return baseUrlPolicy(value);}catch(error){return translate(error);}}
export const publicAddress=publicAddressPolicy;
export async function providerRequest(endpoint:AiEndpoint,path:'models'|'chat/completions'|'responses'|'messages',body?:unknown,timeoutMs=25000){try{return await application.providerRequest(endpoint,path,body,timeoutMs);}catch(error){return translate(error);}}
export async function generateProviderJson(endpoint:AiEndpoint,messages:AiMessage[],maxTokens=2000){try{return await application.generateProviderJson(endpoint,messages,maxTokens);}catch(error){return translate(error);}}

