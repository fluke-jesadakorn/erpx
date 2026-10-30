import {holdingRoutingEnabled,holdingHostForHost,holdingOrigin} from '@erpx/feature-runtime/lib/holding-hosts';
import {trustedProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import 'server-only';
import { NextResponse } from 'next/server';
import { z } from 'zod';
import { configuredProductOrigin, productHostForHost } from '@erpx/feature-runtime/lib/product-hosts';
import { productIds } from '@erpx/contracts/product-registry';
import { runtimeEnv } from '../../../../packages/server-runtime/src/public/runtime';
import type { TransactionalSqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError, errorResponse } from '@erpx/server-runtime/public/http';
import { accountCredentialSession, createAuth } from '@erpx/server-identity_access/adapters/auth';
import { cookie } from '@erpx/server-identity_access/adapters/product-sso-context';
import { deviceAccountStore, deviceBrowserCodec, deviceBrowserCookie,liveDeviceParent } from '@erpx/server-identity_access/adapters/device-account-store';
import {deviceAccountReturn} from '@erpx/server-identity_access/domain/device-account-return';
import {configuredProductHandoff} from '@erpx/feature-runtime/lib/product-hosts';
import {isProductHostKey} from '@erpx/contracts/product-registry';
import {invalidateDeviceWorkContexts} from '@erpx/server-identity_access/adapters/device-account-revocation';

const privateHeaders = { 'Cache-Control': 'private, no-store', 'Referrer-Policy': 'no-referrer', 'X-Content-Type-Options': 'nosniff', Vary: 'Origin' };
const cookieOptions = { httpOnly: true, secure: true, sameSite: 'lax' as const, path: '/', maxAge: 43200 };
const selection = z.object({ handle: z.string().min(20).max(128), revision: z.number().int().nonnegative() }).strict();
const signIn = z.object({ email: z.string().email().max(320), password: z.string().min(1).max(4096), revision: z.number().int().nonnegative().optional(),callbackURL:z.string().max(4096).optional(),rememberMe:z.boolean().optional() }).strict();
function allowedOrigin(origin: string | null) {
  return !!origin && (holdingRoutingEnabled(process.env)&&!!trustedProductOrigin(origin)&&!!holdingHostForHost(new URL(origin).host)||['account', 'apps', 'admin', ...productIds].some(key => configuredProductOrigin(key) === origin));
}
function copyCookies(from: Response, into: Response) { for (const value of from.headers.getSetCookie()) into.headers.append('Set-Cookie', value); }
function staleDeviceCookies(headers:Headers,proven:readonly {session:{id:string;token:string}}[],removed?:string){
 const live=new Set(proven.filter(item=>item.session.id!==removed).map(item=>`__Secure-erpx-account.session_token_multi-${item.session.token.toLowerCase()}`));
 return (headers.get('cookie')??'').split(';').map(pair=>pair.trim().split('=',1)[0]).filter(name=>/^__Secure-erpx-account\.session_token_multi-[a-z0-9_-]+$/.test(name)&&!live.has(name));
}
function expireDeviceCookies(response:Response,names:readonly string[]){
 for(const name of names)response.headers.append('Set-Cookie',`${name}=; Max-Age=0; Path=/; HttpOnly; Secure; SameSite=Lax`);
}
function withoutCookies(headers:Headers,names:readonly string[]){
 const result=new Headers(headers),removed=new Set(names);result.set('cookie',(headers.get('cookie')??'').split(';').filter(pair=>!removed.has(pair.trim().split('=',1)[0])).join(';'));return result;
}
function afterCookies(headers: Headers, response: Response) {
  const values = new Map((headers.get('cookie') ?? '').split(';').map(value => value.trim()).filter(Boolean).map(value => {
    const at = value.indexOf('='); return [value.slice(0, at), value.slice(at + 1)] as const;
  }));
  for (const value of response.headers.getSetCookie()) {
    const item = value.split(';', 1)[0], at = item.indexOf('='), name = item.slice(0, at), content = item.slice(at + 1);
    if (!content || /max-age=0(?:;|$)/i.test(value)) values.delete(name); else values.set(name, content);
  }
  const result = new Headers(headers); result.set('cookie', [...values].map(([name, value]) => `${name}=${value}`).join('; ')); return result;
}
async function context(headers: Headers) {
  const env = await runtimeEnv(), origin = configuredProductOrigin('account')!;
  const database = env.CONTROL_DB as TransactionalSqlDatabase, codec = deviceBrowserCodec(env.BETTER_AUTH_SECRET);
  const browserId = codec.decode(cookie(headers, deviceBrowserCookie));
  const auth = await createAuth(origin);
  const devices = await auth.api.listDeviceSessions({ headers });
  const active = await accountCredentialSession(headers);
  const candidates = [...devices, ...(active && !devices.some(item => item.session.id === active.session.id) ? [active] : [])];
  const current=await Promise.all(candidates.map(item=>liveDeviceParent(database,item.session.id)));
  const proven=candidates.filter((_item,index)=>current[index]);
  return { auth, database, codec, browserId, active, proven, proof: proven.map(item => item.session.id), store: deviceAccountStore(database, browserId,Date.now,invalidateDeviceWorkContexts) };
}
function browserCookie(response: NextResponse, codec: ReturnType<typeof deviceBrowserCodec>, id: string) {
  response.cookies.set(deviceBrowserCookie, codec.encode(id), cookieOptions);
}
async function json(request: Request) {
  if (request.headers.get('content-type')?.split(';', 1)[0].trim().toLowerCase() !== 'application/json') throw new ApiError(415, 'JSON_REQUIRED', 'Submit account choices as JSON.');
  const body = await request.text(); if (Buffer.byteLength(body) > 8192) throw new ApiError(413, 'REQUEST_TOO_LARGE', 'This account request is too large.');
  try { return JSON.parse(body) as unknown; } catch { throw new ApiError(422, 'INVALID_JSON', 'Submit a valid account choice.'); }
}
function valid<T>(schema: z.ZodType<T>, raw: unknown): T {
  const result = schema.safeParse(raw); if (!result.success) throw new ApiError(422, 'VALIDATION_FAILED', 'Check the displayed account fields.'); return result.data;
}

/** Used by normal email sign-in and the drawer, with the original auth policy,
 * password verification, throttling and host-only Set-Cookie response. */
export async function accountEmailSignIn(request: Request, facade = false) {
  const raw = await json(request), input = valid(signIn, raw), before = await context(request.headers);
  const reservation = await before.store.reserve(before.proof, input.email, input.revision, before.active?.session.id ?? null);
  const store = deviceAccountStore(before.database, reservation.browserId,Date.now,invalidateDeviceWorkContexts);
  const staleCookies=staleDeviceCookies(request.headers,before.proven);
  const headers = withoutCookies(request.headers,staleCookies); headers.set('origin', configuredProductOrigin('account')!);
  const authRequest = new Request(new URL('/api/auth/sign-in/email', configuredProductOrigin('account')!), {
    method: 'POST', headers, body: JSON.stringify({ email: input.email, password: input.password,...(!facade&&input.callbackURL?{callbackURL:input.callbackURL}:{}),...(input.rememberMe===undefined?{}:{rememberMe:input.rememberMe}) }),
  });
  let signedIn: Response | undefined;
  try {
    signedIn = await before.auth.handler(authRequest);
    if (!signedIn.ok) {
      await store.release(reservation.nonce);
      const response = facade ? NextResponse.json({ error: { code: 'SIGN_IN_FAILED', message: 'Sign-in failed. Check your email, password or approved Directory sign-in.' } }, { status: signedIn.status, headers: privateHeaders }) : new NextResponse(signedIn.body, { status: signedIn.status, headers: signedIn.headers });
      browserCookie(response, before.codec, reservation.browserId);expireDeviceCookies(response,staleCookies);return response;
    }
    const updatedHeaders = afterCookies(headers, signedIn);
    const after = await context(updatedHeaders);
    if (!after.active) throw new ApiError(401, 'ACCOUNT_SIGN_IN_FAILED', 'Sign-in could not be verified.');
    const result = await store.complete(after.proof, after.active.session.id, reservation.nonce);
    const response = facade ? NextResponse.json({ data: result.state }, { headers: privateHeaders }) : new NextResponse(signedIn.body, { status: signedIn.status, headers: signedIn.headers });
    browserCookie(response, before.codec, result.browserId);
    // ResponseCookies owns its initial cookie map. Append auth cookies only
    // after setting our cookie, otherwise its serialization replaces them.
    if (facade) copyCookies(signedIn, response);
    expireDeviceCookies(response,staleCookies);
    return response;
  } catch (error) {
    await store.release(reservation.nonce);
    if (signedIn?.ok) {
      const active = await accountCredentialSession(afterCookies(request.headers, signedIn));
      if (active && !before.proof.includes(active.session.id)) await before.database.prepare('DELETE FROM session WHERE id=?').bind(active.session.id).run();
    }
    throw error;
  }
}

/** Account callback/normal multi-session operations share the same selection.
 * Only successful responses that actually set an auth cookie can select. */
export async function accountAuthenticationRequest(request:Request){
 const ctx=await context(request.headers),expired=staleDeviceCookies(request.headers,ctx.proven);
 return expired.length?new Request(request,{headers:withoutCookies(request.headers,expired)}):request;
}
export async function synchronizeAccountAuth(request: Request, response: Response) {
  if (response.status >= 400 || !response.headers.getSetCookie().some(value => /erpx-account\.session_token=/.test(value))) return response;
  const before=await context(request.headers);
  const after = await context(afterCookies(request.headers, response));
  if (!after.active) return response;
  try{
    const result = await after.store.complete(after.proof, after.active.session.id);
    const next = new NextResponse(response.body, { status: response.status, headers: response.headers });
    browserCookie(next, after.codec, result.browserId);expireDeviceCookies(next,staleDeviceCookies(request.headers,after.proven));return next;
  }catch(error){
    // A rejected callback never delivers its new credential or leaves an
    // unselected extra parent behind, including the sixth Directory account.
    if(!before.proof.includes(after.active.session.id))await after.database.prepare('DELETE FROM session WHERE id=?').bind(after.active.session.id).run();
    throw error;
  }
}

export async function deviceAccountsRequest(request: Request) {
  const origin = request.headers.get('origin'), cors:Record<string,string> = origin && allowedOrigin(origin) ? { 'Access-Control-Allow-Origin': origin, 'Access-Control-Allow-Credentials': 'true' } : {};
  let response: Response;
  try {
    if (productHostForHost(request.headers.get('host')) !== 'account') return new Response('Not found', { status: 404, headers: privateHeaders });
    if (origin && !allowedOrigin(origin) || request.method !== 'GET' && !allowedOrigin(origin)) throw new ApiError(403, 'ORIGIN_FORBIDDEN', 'Use an owned ERPX application to select an account.');
    if (request.method === 'OPTIONS') return new Response(null, { status: 204, headers: { ...privateHeaders, ...cors, 'Access-Control-Allow-Methods': 'GET, POST, DELETE, OPTIONS', 'Access-Control-Allow-Headers': 'Content-Type', 'Access-Control-Max-Age': '600' } });
    const path = new URL(request.url).pathname, action = path.slice('/api/v1/account/device-accounts'.length);
    if (request.method === 'POST' && action === '/sign-in') response = await accountEmailSignIn(request, true);
    else {
      const ctx = await context(request.headers);
      if (request.method === 'GET' && !action) response = NextResponse.json({ data: ctx.proof.length ? await ctx.store.read(ctx.proof) : { initialized: false, revision: 0, accounts: [], maximum: 5 } }, { headers: privateHeaders });
      else if (request.method === 'POST' && action === '/initialize') {
        const result = await ctx.store.initialize(ctx.proof, ctx.active?.session.id ?? null);
        const next = NextResponse.json({ data: result.state }, { headers: privateHeaders }); browserCookie(next, ctx.codec, result.browserId); response = next;
      } else if (request.method === 'POST' && action === '/active') {
        const input = valid(selection, await json(request));
        const membership = await ctx.database.prepare('SELECT session_id FROM account_device_accounts WHERE browser_id=? AND handle=?').bind(ctx.browserId, input.handle).first<{session_id:string}>();
        const selected = ctx.proven.find(item => item.session.id === membership?.session_id);
        if(!selected) throw new ApiError(401,'DEVICE_ACCOUNT_EXPIRED','This saved account expired. Sign in again.');
        const headers = new Headers(request.headers); headers.set('origin', configuredProductOrigin('account')!);
        const authResponse = await ctx.auth.api.setActiveSession({ headers, body: { sessionToken: selected.session.token }, asResponse: true });
        if(!authResponse.ok) throw new ApiError(401,'DEVICE_ACCOUNT_EXPIRED','This saved account expired. Sign in again.');
        const result = await ctx.store.activate(ctx.proof, input.handle, input.revision);
        const next = NextResponse.json({ data: result.state }, { headers: privateHeaders }); browserCookie(next, ctx.codec, result.browserId); copyCookies(authResponse, next); response = next;
      } else if (request.method === 'DELETE' && !action) {
        const input = valid(selection, await json(request)), result = await ctx.store.remove(ctx.proof, input.handle, input.revision);
        const next = NextResponse.json({ data: result.state }, { headers: privateHeaders }); browserCookie(next, ctx.codec, result.browserId);
        expireDeviceCookies(next,staleDeviceCookies(request.headers,ctx.proven,result.sessionId));
        if(ctx.active?.session.id===result.sessionId)expireDeviceCookies(next,['__Secure-erpx-account.session_token']);
        response = next;
      } else response = new Response('Not found', { status: 404, headers: privateHeaders });
      expireDeviceCookies(response,staleDeviceCookies(request.headers,ctx.proven));
    }
  } catch (error) { response = errorResponse(error); }
  for (const [key, value] of Object.entries({ ...privateHeaders, ...cors })) response.headers.set(key, value);
  return response;
}
export async function deviceAccountReturnRequest(request:Request){
 if(productHostForHost(request.headers.get('host'))!=='account')return new Response('Not found',{status:404,headers:privateHeaders});
 const url=new URL(request.url),target=deviceAccountReturn(url.pathname+url.search);
 if(!target)return new Response('Invalid account return',{status:400,headers:privateHeaders});
 const session=await(await createAuth(configuredProductOrigin('account')!)).api.getSession({headers:request.headers});
 if(!session)return NextResponse.redirect(new URL('/login?'+new URLSearchParams({next:target}),configuredProductOrigin('account')!),{status:303,headers:privateHeaders});
 const app=url.searchParams.get('app')!;if(!isProductHostKey(app))return new Response('Not found',{status:404});
 const next='/account/continue?'+new URLSearchParams({app,next:new URL(target,'https://account.invalid').searchParams.get('next')!});
 const origin=url.searchParams.get('holding')&&holdingRoutingEnabled(process.env)?holdingOrigin(url.searchParams.get('holding')!):null;
 if(origin)return NextResponse.redirect(new URL('/sso/start?'+new URLSearchParams({next}),origin),{status:303,headers:privateHeaders});
 return NextResponse.redirect(configuredProductHandoff(app,next)!,{status:303,headers:privateHeaders});
}
