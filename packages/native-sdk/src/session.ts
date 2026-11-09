import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type { AccountSessionResponse } from '@erpx/contracts/account-auth';
import {contributorExactOrigin, accountDevelopmentOrigin} from '@erpx/contracts/holding-hosts';
import {allowedPrivateLoopbackPort} from './loopback';

/** Explicit private transport and HTTPS public authority are separate contexts. */
export type NativeSessionClientConfiguration = Readonly<{
  privateBaseUrl: string;
  accountOrigin: string;
  holdingOrigins: readonly string[];
  timeoutMs: number;
  originProfile?: 'public' | 'contributor' | 'loopback';
  /** Explicit private production runtime opt-in; admits loopback 3101. */
  production?: boolean;
}>;
export class NativeSessionReadError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number) {
    super('The native session query could not be completed.');
    this.name = 'NativeSessionReadError';
    this.code = code;
    this.status = status;
  }
}
const fail = (code: string, status?: number): never => { throw new NativeSessionReadError(code, status); };
function exactOrigin(value: string, profile: 'public' | 'contributor' | 'loopback'): URL {
  let url: URL;
  try { url = new URL(value); } catch { return fail('NATIVE_SESSION_CONFIGURATION_INVALID'); }
  if (profile === 'loopback') {
    const port = Number(url.port);
    if (url.protocol !== 'http:' || !url.hostname.endsWith('.erpxbrowser.test') || !url.port || !allowedPrivateLoopbackPort(port, false)
      || url.username || url.password || url.pathname !== '/' || url.search || url.hash) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
    if (value !== url.origin) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
    return url;
  }
  const contributor = contributorExactOrigin(url.host, profile) === value;
  if (url.protocol !== 'https:' || url.username || url.password || (url.port && !contributor) || url.pathname !== '/' || url.search || url.hash
    || url.hostname.length > 253 || !url.hostname.includes('.') || !url.hostname.split('.').every(part => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(part)))
    return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  if (value !== url.origin) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  return url;
}
function endpoint(value: string, production: boolean): URL {
  let url: URL;
  try { url = new URL(value); } catch { return fail('NATIVE_SESSION_CONFIGURATION_INVALID'); }
  const port = Number(url.port);
  if (url.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(url.hostname) || !url.port
    || !allowedPrivateLoopbackPort(port, production)
    || url.username || url.password || url.pathname !== '/' || url.search || url.hash)
    return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  if (value !== url.origin) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  return new URL('/api/auth/get-session', url);
}
function object(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)
    || Object.keys(value).length !== keys.length || !keys.every(key => Object.hasOwn(value, key))) return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return value as Record<string, unknown>;
}
function text(value: unknown, maximum: number): string {
  if (typeof value !== 'string' || new TextEncoder().encode(value).length > maximum) return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return value;
}
function nonempty(value: unknown, maximum: number): string {
  const result = text(value, maximum);
  return result || fail('NATIVE_SESSION_RESPONSE_INVALID');
}
function date(value: unknown): string {
  const valueText = text(value, 64);
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(valueText) || !Number.isFinite(Date.parse(valueText)))
    return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return valueText;
}
function session(value: unknown): AccountSessionResponse | null {
  if (value === null) return null;
  const root = object(value, ['session', 'user']);
  const current = object(root.session, ['id', 'userId', 'expiresAt', 'createdAt', 'updatedAt']);
  const user = object(root.user, ['id', 'name', 'email', 'emailVerified', 'image']);
  const userId = text(user.id, 200), sessionUser = text(current.userId, 200);
  if (!userId || userId !== sessionUser || typeof user.emailVerified !== 'boolean') return fail('NATIVE_SESSION_RESPONSE_INVALID');
  return {
    session: { id: nonempty(current.id, 200), userId: sessionUser, expiresAt: date(current.expiresAt), createdAt: date(current.createdAt), updatedAt: date(current.updatedAt) },
    user: { id: userId, name: text(user.name, 32768), email: text(user.email, 320), emailVerified: user.emailVerified,
      image: user.image === null ? null : text(user.image, 32768) },
  };
}
async function boundedJson(response: Response, signal: AbortSignal): Promise<unknown> {
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json' || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail('NATIVE_SESSION_RESPONSE_INVALID');
  }
  const reader = response.body.getReader(), chunks: Uint8Array[] = [];
  let total = 0;
  const cancel = () => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener('abort', cancel, { once: true });
  try {
    if (signal.aborted) { cancel(); return fail('NATIVE_SESSION_ABORTED'); }
    for (;;) {
      const { value, done } = await reader.read();
      if (signal.aborted) return fail('NATIVE_SESSION_ABORTED');
      if (done) break;
      total += value.byteLength;
      if (total > 32 * 1024) { await reader.cancel(); return fail('NATIVE_SESSION_RESPONSE_LIMIT'); }
      chunks.push(value);
    }
  } finally { signal.removeEventListener('abort', cancel); reader.releaseLock(); }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  try { return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); }
  catch { return fail('NATIVE_SESSION_RESPONSE_INVALID'); }
}

/** No environment discovery, cross-request cache, fallback backend or actor DTO. */
export type NativeSessionFetch = (input: URL, init: RequestInit) => Promise<Response>;
export type NativeSessionRead = (incoming: Pick<Headers, 'get'>, signal?: AbortSignal) => Promise<AccountSessionResponse | null>;
export function createNativeSessionClient(configuration: NativeSessionClientConfiguration, fetchPort: NativeSessionFetch = directSessionHttp): NativeSessionRead {
  const url = endpoint(configuration.privateBaseUrl, configuration.production === true);
  if (!Number.isInteger(configuration.timeoutMs) || configuration.timeoutMs < 250 || configuration.timeoutMs > 30000
    || configuration.holdingOrigins.length < 1 || configuration.holdingOrigins.length > 64
    || (configuration.production !== undefined && typeof configuration.production !== 'boolean')) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  const profile=configuration.originProfile ?? 'public';
  if (profile !== 'public' && profile !== 'contributor' && profile !== 'loopback') return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  const origins = [configuration.accountOrigin, ...configuration.holdingOrigins].map(value => exactOrigin(value,profile));
  if (profile === 'contributor' && (configuration.accountOrigin !== accountDevelopmentOrigin(profile) || origins.slice(1).some(origin => contributorExactOrigin(origin.host, profile) !== origin.origin || origin.origin === configuration.accountOrigin))) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  const hosts = new Set(origins.map(origin => origin.host));
  if (hosts.size !== origins.length) return fail('NATIVE_SESSION_CONFIGURATION_INVALID');
  const timeoutMs = configuration.timeoutMs;
  return async (incoming: Pick<Headers, 'get'>, signal?: AbortSignal): Promise<AccountSessionResponse | null> => {
    const incomingHost = incoming.get('host'), cookie = incoming.get('cookie');
    const host = profile === 'loopback' && incomingHost !== null && /^127\.0\.0\.1:\d+$/.test(incomingHost)
      ? new URL(configuration.accountOrigin).host
      : incomingHost;
    if (!host || !hosts.has(host)) return fail('NATIVE_SESSION_AUTHORITY_INVALID');
    if (cookie && new TextEncoder().encode(cookie).length > 8192) return fail('NATIVE_SESSION_COOKIE_LIMIT');
    const headers = new Headers({ host, accept: 'application/json', 'cache-control': 'no-cache' });
    if (cookie) headers.set('cookie', cookie);
    // Direct node:http never enters Next patched-fetch memo/HMR/data caches.
    // Public HTTPS authority never comes from private HTTP or forwarded headers.
    const controller = new AbortController();
    const cancelled = () => controller.abort();
    signal?.addEventListener('abort', cancelled, { once: true });
    if (signal?.aborted) controller.abort();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const response = await fetchPort(url, { method: 'GET', headers, cache: 'no-store', credentials: 'omit', redirect: 'error', signal: controller.signal });
      if (response.redirected) return fail('NATIVE_SESSION_REDIRECT_FORBIDDEN');
      if (response.status !== 200) return fail('NATIVE_SESSION_UNAVAILABLE', response.status);
      return session(await boundedJson(response, controller.signal));
    } catch (error) {
      const aborted = controller.signal.aborted;
      controller.abort();
      if (error instanceof NativeSessionReadError) throw error;
      return fail(aborted ? 'NATIVE_SESSION_ABORTED' : 'NATIVE_SESSION_UNAVAILABLE');
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', cancelled); }
  };
}

/** A one-request, no-proxy/no-cookie-jar server transport. Native HTTP parsing
 * and stream cancellation stay in node:http; no custom HTTP parser or Next
 * internals are involved. No global agent/cache configuration is mutated. */
function directSessionHttp(url: URL, init: RequestInit): Promise<Response> {
  return new Promise((resolve, reject) => {
    // Next's ambient ProcessEnv type requires NODE_ENV even for ProxyEnv.
    // Node accepts this source-owned empty proxy object; no env is read/copied.
    // Node/Bun DOM stream declarations also differ in their BYOB overloads,
    // although Readable.toWeb supplies the standard stream used by Response.
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = request(url, {
      method: init.method ?? 'GET', agent, maxHeaderSize: 16384,
      headers: Object.fromEntries(new Headers(init.headers).entries()),
      signal: init.signal ?? undefined,
    }, incoming => {
      incoming.once('close', () => agent.destroy());
      try {
        const status = incoming.statusCode ?? 502;
        const headers = new Headers();
        for (let index = 0; index < incoming.rawHeaders.length; index += 2) {
          headers.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
        }
        for(const name of ['connection','keep-alive','transfer-encoding','upgrade','proxy-authenticate','proxy-authorization','te','trailer'])headers.delete(name);
        resolve(new Response([204,205,304].includes(status)?null:Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status, headers }));
      } catch (error) { incoming.destroy(); agent.destroy(); reject(error); }
    });
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.end(typeof init.body === 'string' ? init.body : undefined);
  });
}

/** Opaque Account/SSO ingress transport. All authentication decisions, cookies,
 * redirects and writes come from Rust. Unsupported paths stay native404. */
export function createNativeAccountHttpTransport(configuration:NativeSessionClientConfiguration,send:NativeSessionFetch=directSessionHttp){
 createNativeSessionClient(configuration,send); // Reuse exact authority/private-target validation without IO.
 const origins=[configuration.accountOrigin,...configuration.holdingOrigins],hostOrigins=new Map(origins.map(value=>[new URL(value).host,value]));
 const loopback=configuration.originProfile==='loopback';
 return async(incoming:Request):Promise<Response>=>{
  const sourceHost=incoming.headers.get('host'),sourceOrigin=incoming.headers.get('origin'),publicUrl=new URL(incoming.url);
  let host:string|undefined,origin:string|undefined;
  if(loopback){
    if(!sourceHost||!/^127\.0\.0\.1:\d+$/.test(sourceHost))return fail('NATIVE_ACCOUNT_AUTHORITY_INVALID');
    const accountPath=/^\/(?:api\/auth|api\/v1\/account|sso)\//.test(publicUrl.pathname);
    const targetOrigin=accountPath?configuration.accountOrigin:configuration.holdingOrigins[0];
    host=new URL(targetOrigin).host;origin=targetOrigin;
    if(sourceOrigin&&sourceOrigin!==`http://${sourceHost}`)return fail('NATIVE_ACCOUNT_ORIGIN_FORBIDDEN',403);
  }else{
    host=sourceHost??undefined;origin=sourceOrigin??undefined;
    if(!host||!hostOrigins.has(host))return fail('NATIVE_ACCOUNT_AUTHORITY_INVALID');
    if(origin&&!origins.includes(origin)||!['GET','OPTIONS'].includes(incoming.method)&&!origin)return fail('NATIVE_ACCOUNT_ORIGIN_FORBIDDEN',403);
  }
  if(!['GET','POST','DELETE','OPTIONS'].includes(incoming.method))return new Response(null,{status:405,headers:{allow:'GET, POST, DELETE, OPTIONS'}});
  if(!(/^\/api\/auth\/[A-Za-z0-9_/-]+$/.test(publicUrl.pathname)||/^\/api\/v1\/account\/device-accounts(?:\/(initialize|active|sign-in))?$/.test(publicUrl.pathname)||/^\/api\/v1\/account\/iam\/mfa\/(enroll|verify)$/.test(publicUrl.pathname)||/^\/api\/v1\/account\/security$/.test(publicUrl.pathname)||/^\/sso\/(start|authorize|callback)$/.test(publicUrl.pathname)))return new Response(null,{status:404});
  if(publicUrl.search.length>8192)return fail('NATIVE_ACCOUNT_QUERY_LIMIT',400);
  const cookie=incoming.headers.get('cookie');if(cookie&&new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_ACCOUNT_COOKIE_LIMIT',400);
  const idempotencyKey=incoming.headers.get('idempotency-key');if(idempotencyKey!==null&&(idempotencyKey.length<8||idempotencyKey.length>128))return fail('NATIVE_ACCOUNT_HEADER_LIMIT',400);
  const headers=new Headers({host,accept:incoming.headers.get('accept')??'application/json','cache-control':'no-cache'});
  if(cookie)headers.set('cookie',cookie);if(origin)headers.set('origin',origin);if(idempotencyKey!==null)headers.set('idempotency-key',idempotencyKey);
  const contentType=incoming.headers.get('content-type');if(contentType)headers.set('content-type',contentType);
  if(incoming.method==='OPTIONS')for(const name of ['access-control-request-method','access-control-request-headers']){const value=incoming.headers.get(name);if(value)headers.set(name,value);}
  const controller=new AbortController(),cancel=()=>controller.abort();incoming.signal.addEventListener('abort',cancel,{once:true});if(incoming.signal.aborted)cancel();
  const timer=setTimeout(cancel,configuration.timeoutMs);
  try{
   const bytes=['POST','DELETE'].includes(incoming.method)?await boundedAccountBytes(incoming.body,8192,controller.signal):null;
   let body:string|undefined;
   if(bytes!==null){try{body=new TextDecoder('utf-8',{fatal:true}).decode(bytes);}catch{return fail('NATIVE_ACCOUNT_BODY_INVALID',400);}}
   const url=new URL(publicUrl.pathname+publicUrl.search,endpoint(configuration.privateBaseUrl, configuration.production === true).origin);
   const response=await send(url,{method:incoming.method,headers,...(body===undefined?{}:{body}),redirect:'manual',cache:'no-store',credentials:'omit',signal:controller.signal});
   if(response.redirected)return fail('NATIVE_ACCOUNT_REDIRECT_FORBIDDEN');
   const responseBytes=await boundedAccountBytes(response.body,256*1024,controller.signal);
   const outgoing=new Headers(response.headers);outgoing.set('cache-control','private, no-store');
   return new Response([204,205,304].includes(response.status)?null:responseBytes,{status:response.status,headers:outgoing});
  }catch(error){controller.abort();if(error instanceof NativeSessionReadError)throw error;return fail('NATIVE_ACCOUNT_UNAVAILABLE');}
  finally{clearTimeout(timer);incoming.signal.removeEventListener('abort',cancel);}
 };
}
async function boundedAccountBytes(body:ReadableStream<Uint8Array>|null,limit:number,signal:AbortSignal):Promise<Uint8Array<ArrayBuffer>|null>{
 if(!body)return null;
 const reader=body.getReader(),chunks:Uint8Array[]=[];let size=0;
 const cancel=()=>{void reader.cancel().catch(()=>undefined);};signal.addEventListener('abort',cancel,{once:true});
 try{for(;;){if(signal.aborted){cancel();return fail('NATIVE_ACCOUNT_ABORTED');}const r=await reader.read();if(signal.aborted)return fail('NATIVE_ACCOUNT_ABORTED');if(r.done)break;size+=r.value.byteLength;if(size>limit){await reader.cancel();return fail('NATIVE_ACCOUNT_PAYLOAD_LIMIT',413);}chunks.push(r.value);}}
 finally{signal.removeEventListener('abort',cancel);reader.releaseLock();}
 const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}return bytes;
}
